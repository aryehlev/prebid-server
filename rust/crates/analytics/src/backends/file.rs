//! File-based analytics backend that appends one JSON line per event.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex;

use crate::events::{
    AmpObject, AuctionObject, CookieSyncObject, NotificationEvent, SetUidObject, VideoObject,
};
use crate::module::PbsAnalyticsModule;

/// Configuration for [`FileAnalyticsBackend`].
#[derive(Debug, Clone)]
pub struct FileAnalyticsConfig {
    /// Absolute or relative path of the log file.
    pub path: PathBuf,
    /// Placeholder for a future rotation size threshold in bytes. Currently
    /// unused but accepted so config parsing can round trip.
    pub rotate_bytes: Option<u64>,
}

impl FileAnalyticsConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            rotate_bytes: None,
        }
    }
}

/// Appends one JSON line per event to a local file.
///
/// All writes go through a `tokio::sync::Mutex`-protected buffered writer so
/// concurrent callers cannot interleave bytes within a single log record.
pub struct FileAnalyticsBackend {
    config: FileAnalyticsConfig,
    writer: Arc<Mutex<BufWriter<File>>>,
}

impl FileAnalyticsBackend {
    /// Open the backing file (creating it if needed) and return a ready to
    /// use backend.
    pub async fn new(config: FileAnalyticsConfig) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.path)
            .await?;
        Ok(Self {
            config,
            writer: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    /// Convenience constructor that takes a path directly.
    pub async fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::new(FileAnalyticsConfig::new(path.as_ref().to_path_buf())).await
    }

    /// Configured file path.
    pub fn path(&self) -> &Path {
        &self.config.path
    }

    async fn append<T: Serialize>(&self, kind: &'static str, evt: &T) {
        let line = match serde_json::to_string(evt) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(kind, error = %e, "failed to serialize analytics event");
                return;
            }
        };
        let mut guard = self.writer.lock().await;
        if let Err(e) = guard.write_all(line.as_bytes()).await {
            tracing::warn!(kind, error = %e, "failed to write analytics event");
            return;
        }
        if let Err(e) = guard.write_all(b"\n").await {
            tracing::warn!(kind, error = %e, "failed to write analytics newline");
            return;
        }
        if let Err(e) = guard.flush().await {
            tracing::warn!(kind, error = %e, "failed to flush analytics writer");
        }
    }
}

#[async_trait]
impl PbsAnalyticsModule for FileAnalyticsBackend {
    fn name(&self) -> &str {
        "file"
    }
    async fn log_auction_object(&self, evt: &AuctionObject) {
        self.append("auction", evt).await;
    }
    async fn log_video_object(&self, evt: &VideoObject) {
        self.append("video", evt).await;
    }
    async fn log_cookie_sync_object(&self, evt: &CookieSyncObject) {
        self.append("cookie_sync", evt).await;
    }
    async fn log_set_uid_object(&self, evt: &SetUidObject) {
        self.append("set_uid", evt).await;
    }
    async fn log_amp_object(&self, evt: &AmpObject) {
        self.append("amp", evt).await;
    }
    async fn log_notification_event(&self, evt: &NotificationEvent) {
        self.append("notification", evt).await;
    }
    async fn shutdown(&self) {
        let mut guard = self.writer.lock().await;
        if let Err(e) = guard.flush().await {
            tracing::warn!(error = %e, "failed to flush analytics writer on shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn file_backend_writes_json_lines() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("analytics.jsonl");
        let backend = FileAnalyticsBackend::open(&path).await.expect("open");

        let evt1 = AuctionObject::new("req-1", "acct-1", 200, serde_json::json!({"a": 1}));
        let evt2 = AuctionObject::new("req-2", "acct-1", 204, serde_json::json!({"b": 2}));
        backend.log_auction_object(&evt1).await;
        backend.log_auction_object(&evt2).await;
        backend.shutdown().await;

        let mut f = tokio::fs::File::open(&path).await.expect("reopen");
        let mut contents = String::new();
        f.read_to_string(&mut contents).await.expect("read");

        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "contents were: {contents}");
        let back1: AuctionObject = serde_json::from_str(lines[0]).expect("parse 1");
        let back2: AuctionObject = serde_json::from_str(lines[1]).expect("parse 2");
        assert_eq!(back1.request_id, "req-1");
        assert_eq!(back2.request_id, "req-2");
    }
}
