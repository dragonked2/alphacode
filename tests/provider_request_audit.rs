//! Child-process isolation verifies the real opt-in file sink without changing
//! process environment under concurrently running tests. All HTTP is loopback.
use std::process::Command;

#[test]
fn audit_matches_loopback_requests() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "audit_child", "--nocapture"])
        .env("ALPHACODE_PROVIDER_AUDIT_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let files: Vec<_> = std::fs::read_dir(directory.path()).unwrap().collect();
    assert_eq!(files.len(), 1);
    let data = std::fs::read_to_string(files[0].as_ref().unwrap().path()).unwrap();
    let events: Vec<serde_json::Value> = data
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 4, "one catalog + three inference dispatches");
    assert_eq!(events[0]["kind"], "models");
    assert!(events[0]["prompt"].is_null());
    assert_eq!(events[1]["prompt"], events[2]["prompt"]);
    assert_ne!(events[2]["prompt"], events[3]["prompt"]);
    assert_eq!(events[1]["session"], "fixture-task");
    assert!(!data.contains("SECRET"));
    assert!(!data.contains("/v1"));
}

#[test]
#[ignore = "run by audit_matches_loopback_requests in a child process"]
fn audit_child() {
    use alphacode::alphacode_provider_core::{
        request_audit::{self, AuditedSend},
        transport::send_with_initial_response_timeout,
    };
    use std::io::{Read, Write};
    use std::time::Duration;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..5 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            assert!(socket.read(&mut [0; 4096]).unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        listener
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        client
            .get(format!("{base}/v1/models?key=SECRET"))
            .audited_send()
            .await
            .unwrap();
        request_audit::scope("fixture-task".into(), async {
            for _ in 0..2 {
                let request = client
                    .post(format!("{base}/v1/chat/completions"))
                    .header("Authorization", "SECRET");
                request_audit::spawn(async move {
                    send_with_initial_response_timeout(request, Duration::from_secs(5))
                        .await
                        .unwrap();
                })
                .await
                .unwrap();
            }
        })
        .await;
        // HTTP tools use the same retry transport but are not provider calls.
        alphacode::alphacode_provider_core::retry::send_with_retry(
            &client,
            client.get(format!("{base}/target-site")).build().unwrap(),
            &alphacode::alphacode_provider_core::retry::RetryPolicy::for_http_tools(),
            "webfetch",
        )
        .await
        .unwrap();
        request_audit::scope("fixture-task".into(), async {
            client
                .post(format!("{base}/v1/responses"))
                .audited_send()
                .await
                .unwrap();
        })
        .await;
    });
    let listener = server.join().unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
