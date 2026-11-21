//! MCP Client - handles communication with a single MCP server

use super::protocol::*;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

type ResponseSenders = HashMap<u64, oneshot::Sender<JsonRpcResponse>>;

struct PendingResponses {
    senders: Mutex<Option<ResponseSenders>>,
    closed: CancellationToken,
}

impl PendingResponses {
    fn new() -> Self {
        Self {
            senders: Mutex::new(Some(HashMap::new())),
            closed: CancellationToken::new(),
        }
    }

    fn register(
        &self,
        id: u64,
        tx: oneshot::Sender<JsonRpcResponse>,
    ) -> Result<PendingResponse<'_>> {
        let mut pending = self
            .senders
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        pending
            .as_mut()
            .context("MCP transport closed")?
            .insert(id, tx);
        Ok(PendingResponse { pending: self, id })
    }

    fn take(&self, id: u64) -> Option<oneshot::Sender<JsonRpcResponse>> {
        self.senders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()?
            .remove(&id)
    }

    fn close(&self) {
        self.senders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        self.closed.cancel();
    }
}

struct PendingResponse<'a> {
    pending: &'a PendingResponses,
    id: u64,
}

impl Drop for PendingResponse<'_> {
    fn drop(&mut self) {
        self.pending.take(self.id);
    }
}

/// Shared communication handle for an MCP server.
/// Multiple sessions can hold clones of this and send concurrent requests.
/// Request/response correlation by ID ensures no interference.
#[derive(Clone)]
pub struct McpHandle {
    pub(crate) name: String,
    request_id: Arc<AtomicU64>,
    pending: Arc<PendingResponses>,
    writer_tx: mpsc::Sender<String>,
    server_info: Arc<std::sync::RwLock<Option<ServerInfo>>>,
    capabilities: Arc<std::sync::RwLock<ServerCapabilities>>,
    tools: Arc<std::sync::RwLock<Vec<McpToolDef>>>,
}

impl McpHandle {
    fn new(name: String, pending: Arc<PendingResponses>, writer_tx: mpsc::Sender<String>) -> Self {
        Self {
            name,
            request_id: Arc::new(AtomicU64::new(1)),
            pending,
            writer_tx,
            server_info: Arc::new(std::sync::RwLock::new(None)),
            capabilities: Arc::new(std::sync::RwLock::new(ServerCapabilities::default())),
            tools: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Send a request and wait for response
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<JsonRpcResponse> {
        self.request_with_timeout(method, params, std::time::Duration::from_secs(30))
            .await
    }

    async fn request_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: std::time::Duration,
    ) -> Result<JsonRpcResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest::new(id, method, params);
        let msg = serde_json::to_string(&request)? + "\n";
        let (tx, rx) = oneshot::channel();
        let _pending = self.pending.register(id, tx)?;
        let response = tokio::time::timeout(timeout, async {
            tokio::select! {
                biased;
                _ = self.pending.closed.cancelled() => anyhow::bail!("MCP transport closed"),
                sent = self.writer_tx.send(msg) => sent.context("Failed to send request")?,
            }
            rx.await.context("MCP transport closed before the response")
        })
        .await
        .context("MCP request timed out")??;

        if let Some(err) = &response.error {
            anyhow::bail!("MCP error {}: {}", err.code, err.message);
        }

        Ok(response)
    }

    /// Call a tool
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolCallResult> {
        let arguments = if arguments.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            arguments
        };
        let params = ToolCallParams {
            name: name.to_string(),
            arguments,
        };

        let response = self
            .request("tools/call", Some(serde_json::to_value(params)?))
            .await?;

        let result = response.result.context("No result from tool call")?;
        let tool_result: ToolCallResult = serde_json::from_value(result)?;

        Ok(tool_result)
    }

    /// Get the server name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get server info
    pub fn server_info(&self) -> Option<ServerInfo> {
        self.server_info
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Get available tools
    pub fn tools(&self) -> Vec<McpToolDef> {
        self.tools
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Refresh the list of available tools
    pub async fn refresh_tools(&self) -> Result<()> {
        let response = self.request("tools/list", None).await?;

        if let Some(result) = response.result {
            let tools_result: ToolsListResult = serde_json::from_value(result)?;
            *self
                .tools
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = tools_result.tools;
        }

        Ok(())
    }
}

/// MCP Client - owns the child process and provides shared handles.
/// Only one McpClient exists per MCP server process, but many McpHandle
/// clones can be distributed to different sessions.
pub struct McpClient {
    handle: McpHandle,
    child: Child,
}

impl McpClient {
    /// Connect to an MCP server, inheriting the current process working directory
    pub async fn connect(name: String, config: &McpServerConfig) -> Result<Self> {
        Self::connect_in_dir(name, config, None).await
    }

    /// Connect to an MCP server, optionally running it in `working_dir`.
    ///
    /// The working directory is only applied when it exists; otherwise the
    /// subprocess falls back to inheriting the current process cwd (issue #557).
    pub async fn connect_in_dir(
        name: String,
        config: &McpServerConfig,
        working_dir: Option<&std::path::Path>,
    ) -> Result<Self> {
        config.ensure_connection_allowed()?;
        let working_dir = working_dir.filter(|dir| dir.is_dir());
        crate::logging::info(&format!(
            "MCP: Connecting to '{}' ({} arguments) cwd={:?}",
            name,
            config.args.len(),
            working_dir
        ));

        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.extend(config.env.clone());

        let mut command = Command::new(&config.command);
        crate::alphacode_base::platform::hide_helper_console(command.as_std_mut());
        command
            .args(&config.args)
            .envs(&env)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {}", config.command))?;

        let stdin = child.stdin.take().context("No stdin")?;
        let stdout = child.stdout.take().context("No stdout")?;
        let stderr = child.stderr.take().context("No stderr")?;

        // Spawn stderr reader
        let server_name = name.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            crate::logging::warn(&format!(
                                "MCP [{}] stderr: {}",
                                server_name, trimmed
                            ));
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Setup channels
        let pending = Arc::new(PendingResponses::new());
        let (writer_tx, mut writer_rx) = mpsc::channel::<String>(32);

        // Spawn writer task
        let mut stdin = stdin;
        let pending_writer = Arc::clone(&pending);
        tokio::spawn(async move {
            tokio::select! {
                _ = pending_writer.closed.cancelled() => {}
                () = async {
                    while let Some(msg) = writer_rx.recv().await {
                        if stdin.write_all(msg.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                            break;
                        }
                    }
                } => {}
            }
            pending_writer.close();
        });

        // Spawn reader task
        let pending_clone = Arc::clone(&pending);
        let reader_name = name.clone();
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                let read = tokio::select! {
                    _ = pending_clone.closed.cancelled() => break,
                    read = reader.read_line(&mut line) => read,
                };
                match read {
                    Ok(0) => {
                        crate::logging::debug(&format!("MCP [{}]: stdout EOF", reader_name));
                        break;
                    }
                    Ok(_) => {
                        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                            if let Some(id) = response.id
                                && let Some(tx) = pending_clone.take(id)
                            {
                                let _ = tx.send(response);
                            }
                        } else {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                crate::logging::debug(&format!(
                                    "MCP [{}] non-JSON output: {}",
                                    reader_name, trimmed
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        crate::logging::warn(&format!("MCP [{}] read error: {}", reader_name, e));
                        break;
                    }
                }
            }
            pending_clone.close();
        });

        let handle = McpHandle::new(name.clone(), pending, writer_tx);

        let mut client = Self { handle, child };

        client
            .initialize()
            .await
            .with_context(|| format!("MCP server '{}' failed to initialize", name))?;

        client
            .handle
            .refresh_tools()
            .await
            .with_context(|| format!("MCP server '{}' failed to list tools", name))?;

        crate::logging::info(&format!(
            "MCP: Connected to '{}' with {} tools",
            name,
            client.handle.tools().len()
        ));

        Ok(client)
    }

    /// Get a shareable handle to this client
    pub fn handle(&self) -> McpHandle {
        self.handle.clone()
    }

    /// Initialize the MCP connection
    async fn initialize(&mut self) -> Result<()> {
        let params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "alphacode".to_string(),
                version: crate::alphacode_build_meta::pkg_version().to_string(),
            },
        };

        let response = self
            .handle
            .request("initialize", Some(serde_json::to_value(params)?))
            .await?;

        if let Some(result) = response.result {
            let init_result: InitializeResult = serde_json::from_value(result)?;
            *self
                .handle
                .server_info
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = init_result.server_info;
            *self
                .handle
                .capabilities
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = init_result.capabilities;
        }

        // Send initialized notification
        let notif = JsonRpcRequest::new(0, "notifications/initialized", None);
        let msg = serde_json::to_string(&notif)? + "\n";
        self.handle.writer_tx.send(msg).await?;

        Ok(())
    }

    /// Check if server is still running
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }

    /// Shutdown the server
    pub async fn shutdown(&mut self) {
        let _ = self
            .handle
            .writer_tx
            .try_send("{\"jsonrpc\":\"2.0\",\"method\":\"shutdown\"}\n".to_string());

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let _ = self.child.kill().await;
        self.handle.pending.close();
    }

    // === Legacy compatibility methods that delegate to handle ===

    pub fn name(&self) -> &str {
        &self.handle.name
    }

    pub fn server_info(&self) -> Option<ServerInfo> {
        self.handle.server_info()
    }

    pub fn tools(&self) -> Vec<McpToolDef> {
        self.handle.tools()
    }

    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolCallResult> {
        self.handle.call_tool(name, arguments).await
    }

    pub async fn refresh_tools(&self) -> Result<()> {
        self.handle.refresh_tools().await
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.handle.pending.close();
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;
    use std::time::Duration;

    fn handle() -> (McpHandle, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(1);
        (
            McpHandle::new("test".into(), Arc::new(PendingResponses::new()), tx),
            rx,
        )
    }

    #[tokio::test]
    async fn aborting_a_request_discards_its_response_slot() {
        let (handle, mut receiver) = handle();
        let caller = handle.clone();
        let task = tokio::spawn(async move { caller.request("tools/list", None).await });
        let message = receiver.recv().await.unwrap();
        let id = serde_json::from_str::<Value>(&message).unwrap()["id"]
            .as_u64()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(handle.pending.take(id).is_none());
    }

    #[tokio::test]
    async fn request_timeout_includes_writer_backpressure_and_cleans_up() {
        let (handle, mut receiver) = handle();
        handle.writer_tx.try_send("already queued".into()).unwrap();
        let error = handle
            .request_with_timeout("tools/list", None, Duration::ZERO)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(handle.pending.take(1).is_none());
        assert_eq!(receiver.try_recv().unwrap(), "already queued");
        assert!(receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn closed_transport_wakes_pending_calls_and_rejects_new_ones() {
        let (handle, mut receiver) = handle();
        let caller = handle.clone();
        let task = tokio::spawn(async move { caller.request("tools/list", None).await });
        receiver.recv().await.unwrap();
        handle.pending.close();
        let error = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("transport closed"));
        assert!(
            handle
                .request("tools/list", None)
                .await
                .unwrap_err()
                .to_string()
                .contains("transport closed")
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::McpClient;
    use crate::mcp::protocol::McpServerConfig;

    /// A minimal fake stdio MCP server (shell script) that reports its own
    /// process cwd as the serverInfo name.
    fn fake_server_config() -> McpServerConfig {
        let script = r#"
while IFS= read -r line; do
  case "$line" in
    *'"initialize"'*)
      printf '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"%s","version":"0"}}}\n' "$PWD"
      ;;
    *'"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}\n'
      ;;
  esac
done
"#;
        McpServerConfig {
            project_source: None,
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), script.to_string()],
            env: Default::default(),
            shared: false,
            transport: None,
            url: None,
            enabled: None,
            disabled: None,
        }
    }

    #[tokio::test]
    async fn connect_in_dir_sets_subprocess_cwd() {
        // Issue #557: owned MCP servers must run in the session project dir.
        let dir = tempfile::tempdir().expect("tempdir");
        let expected = dir.path().canonicalize().expect("canonicalize");

        let client = McpClient::connect_in_dir(
            "cwd-test".to_string(),
            &fake_server_config(),
            Some(dir.path()),
        )
        .await
        .expect("connect");

        let reported = client.server_info().expect("server info").name;
        assert_eq!(
            std::path::Path::new(&reported)
                .canonicalize()
                .expect("canonicalize reported"),
            expected
        );
    }

    #[tokio::test]
    async fn connect_in_dir_missing_dir_falls_back_to_inherited_cwd() {
        let client = McpClient::connect_in_dir(
            "cwd-fallback-test".to_string(),
            &fake_server_config(),
            Some(std::path::Path::new("/nonexistent/alphacode-557")),
        )
        .await
        .expect("connect should fall back to inherited cwd");

        let reported = client.server_info().expect("server info").name;
        assert!(!reported.is_empty());
    }
}
