//! Regression coverage for repository-supplied MCP startup commands.
use super::*;
use std::sync::Arc;

fn marker_config(marker: &std::path::Path, shared: bool) -> McpServerConfig {
    #[cfg(windows)]
    let (command, args) = (
        "cmd",
        vec![
            "/C".to_string(),
            format!("echo spawned > \"{}\"", marker.display()),
        ],
    );
    #[cfg(not(windows))]
    let (command, args) = (
        "/bin/sh",
        vec![
            "-c".to_string(),
            "echo spawned > \"$1\"".to_string(),
            "sh".to_string(),
            marker.display().to_string(),
        ],
    );
    serde_json::from_value(serde_json::json!({"command":command,"args":args,"shared":shared}))
        .unwrap()
}

#[tokio::test]
async fn repository_commands_never_spawn_through_any_connection_path() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("unexpected-spawn.txt");
    for shared in [false, true] {
        let mut server = marker_config(&marker, shared);
        server.project_source = Some(temp.path().join(".mcp.json"));
        let mut config = McpConfig::default();
        config.servers.insert("project-docs".into(), server.clone());
        let pool = Arc::new(SharedMcpPool::new(config.clone()));
        let manager = McpManager::with_config(config);
        assert_eq!(manager.connect_all().await.unwrap(), (0, vec![]));
        assert_eq!(pool.connect_all().await, (0, vec![]));
        let error = manager.connect("project-docs", &server).await.unwrap_err();
        assert!(error.to_string().contains("Project MCP server blocked"));
        let error = pool
            .connect_server("project-docs", &server)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("Project MCP server blocked"));
        assert!(
            McpClient::connect_in_dir("project-docs".into(), &server, Some(temp.path()))
                .await
                .is_err()
        );
        assert!(!marker.exists(), "repository command executed");
        assert!(manager.connected_servers().await.is_empty());
        assert!(pool.connected_servers().await.is_empty());
    }
}

#[test]
fn all_repository_config_formats_are_blocked_and_cannot_forge_provenance() {
    let _guard = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("repo");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let previous = std::env::var_os("ALPHACODE_HOME");
    crate::alphacode_core::env::set_var("ALPHACODE_HOME", &home);
    std::fs::write(
        home.join("mcp.json"),
        r#"{"servers":{"trusted-global":{"command":"trusted-global"}}}"#,
    )
    .unwrap();
    for relative in [".alphacode/mcp.json", ".mcp.json", ".claude/mcp.json"] {
        let path = project.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"mcpServers":{"project-docs":{"command":"untrusted-command","shared":false,"project_source":null}}}"#).unwrap();
        let loaded = McpConfig::load_for_dir(Some(&project));
        let server = &loaded.servers["project-docs"];
        assert!(server.is_enabled());
        assert!(server.is_project_local());
        assert!(!server.can_auto_connect());
        assert!(server.ensure_connection_allowed().is_err());
        assert!(loaded.servers["trusted-global"].can_auto_connect());
        let serialized = serde_json::to_value(server).unwrap();
        assert!(serialized.get("project_source").is_none());
        std::fs::remove_file(path).unwrap();
    }
    assert!(!McpConfig::load().servers.contains_key("project-docs"));
    match previous {
        Some(value) => crate::alphacode_core::env::set_var("ALPHACODE_HOME", value),
        None => crate::alphacode_core::env::remove_var("ALPHACODE_HOME"),
    }
}
