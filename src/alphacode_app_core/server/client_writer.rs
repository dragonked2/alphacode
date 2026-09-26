use crate::alphacode_app_core::protocol::{ServerEvent, encode_event};
use anyhow::Result;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

pub(super) async fn write_direct_event(
    writer: &Arc<Mutex<crate::transport::WriteHalf>>,
    event: &ServerEvent,
) -> Result<()> {
    let json = encode_event(event);
    let mut w = writer.lock().await;
    w.write_all(json.as_bytes()).await?;
    // Make the complete newline-delimited frame visible before the next
    // protocol await. This matters for Windows named pipes, where a write may
    // be buffered independently of the following event-loop turn.
    w.flush().await?;
    Ok(())
}
