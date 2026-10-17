//! Length-prefixed IPC messaging protocol for agent sidecar communication.

use crate::error::DaemonError;
use canon_core::model::SliceMode;
use canon_core::output::OutputFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// IPC request sent from agent tool hook or CLI client to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// Heartbeat ping.
    Ping,
    /// Advisory post-read tool hook request.
    PostReadHook {
        /// Target file path inspected.
        file: PathBuf,
    },
    /// Request AST slice for file.
    Slice {
        /// Target file path.
        file: PathBuf,
        /// Slicing mode.
        mode: SliceMode,
        /// Desired output format.
        format: OutputFormat,
    },
    /// Request graceful shutdown of daemon.
    Shutdown,
}

/// IPC response sent from daemon back to agent tool or CLI client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Heartbeat response.
    Pong,
    /// Advisory context block returned out-of-band for post-read hooks.
    AdvisoryContext {
        /// Target file.
        file: String,
        /// Advisory markdown block.
        advisory_block: String,
    },
    /// Slicing output content.
    SliceResult {
        /// Formatted context content.
        content: String,
    },
    /// Operation error response.
    Error {
        /// Error message.
        message: String,
    },
    /// Acknowledgment of shutdown signal.
    ShutdownAck,
}

/// Writes a length-prefixed JSON message onto an async stream.
pub async fn write_message<W, T>(stream: &mut W, message: &T) -> Result<(), DaemonError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = serde_json::to_vec(message).map_err(|e| DaemonError::Protocol(e.to_string()))?;
    let len = payload.len() as u32;

    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(DaemonError::Io)?;
    stream.write_all(&payload).await.map_err(DaemonError::Io)?;
    stream.flush().await.map_err(DaemonError::Io)?;
    Ok(())
}

/// Reads a length-prefixed JSON message from an async stream.
pub async fn read_message<R, T>(stream: &mut R) -> Result<T, DaemonError>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(DaemonError::Io)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 64 * 1024 * 1024 {
        return Err(DaemonError::Protocol(format!(
            "frame size {} exceeds 64MB limit",
            len
        )));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await.map_err(DaemonError::Io)?;

    let msg = serde_json::from_slice(&buf).map_err(|e| DaemonError::Protocol(e.to_string()))?;
    Ok(msg)
}
