//! Sinks for diff entries.

use crate::diff::{DiffEntry, DiffKind};
use crate::error::ParityError;
use async_trait::async_trait;
use serde::Serialize;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// A sink that receives batches of [`DiffEntry`] values tagged with a
/// request-correlation id.
#[async_trait]
pub trait DiffReporter: Send + Sync {
    /// Report a batch of diff entries associated with the given request id.
    async fn report(&self, entries: &[DiffEntry], request_id: &str) -> Result<(), ParityError>;
}

/// Reporter that emits each entry via `tracing::info!`.
#[derive(Debug, Default, Clone)]
pub struct TracingReporter;

#[async_trait]
impl DiffReporter for TracingReporter {
    async fn report(&self, entries: &[DiffEntry], request_id: &str) -> Result<(), ParityError> {
        for entry in entries {
            tracing::info!(
                request_id = request_id,
                path = entry.path.as_str(),
                kind = ?entry.kind,
                "parity diff entry",
            );
        }
        Ok(())
    }
}

/// On-disk, append-only JSON-lines reporter.
///
/// Each call to [`FileReporter::report`] appends one JSON object per
/// [`DiffEntry`]; the appender is serialized via an internal `Mutex` so it is
/// safe to share across tasks.
#[derive(Debug)]
pub struct FileReporter {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileReporter {
    /// Create a reporter that appends to `path` (file is created if missing).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), lock: Mutex::new(()) }
    }
}

#[derive(Debug, Serialize)]
struct FileRecord<'a> {
    request_id: &'a str,
    path: &'a str,
    kind: &'static str,
    ts: String,
}

fn kind_str(k: &DiffKind) -> &'static str {
    match k {
        DiffKind::Added => "added",
        DiffKind::Removed => "removed",
        DiffKind::Changed => "changed",
        DiffKind::TypeChanged => "type_changed",
    }
}

#[async_trait]
impl DiffReporter for FileReporter {
    async fn report(&self, entries: &[DiffEntry], request_id: &str) -> Result<(), ParityError> {
        let _guard = self.lock.lock().await;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        let ts = chrono::Utc::now().to_rfc3339();
        for entry in entries {
            let record = FileRecord {
                request_id,
                path: entry.path.as_str(),
                kind: kind_str(&entry.kind),
                ts: ts.clone(),
            };
            let mut line = serde_json::to_vec(&record)?;
            line.push(b'\n');
            file.write_all(&line).await?;
        }
        file.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffKind;

    #[tokio::test]
    async fn file_reporter_appends_lines() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("parity-test-{}.jsonl", std::process::id()));
        let _ = tokio::fs::remove_file(&path).await;

        let reporter = FileReporter::new(&path);
        let entries = vec![
            DiffEntry { path: "a.b".into(), kind: DiffKind::Added },
            DiffEntry { path: "c".into(), kind: DiffKind::Removed },
        ];
        reporter.report(&entries, "req-1").await.unwrap();
        reporter.report(&[DiffEntry { path: "x".into(), kind: DiffKind::Changed }], "req-2")
            .await
            .unwrap();

        let contents = tokio::fs::read_to_string(&path).await.unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3, "expected three lines, got {contents:?}");
        assert!(lines[0].contains("req-1"));
        assert!(lines[0].contains("\"added\""));
        assert!(lines[1].contains("\"removed\""));
        assert!(lines[2].contains("req-2"));
        assert!(lines[2].contains("\"changed\""));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn tracing_reporter_ok() {
        let reporter = TracingReporter;
        reporter
            .report(
                &[DiffEntry { path: "a".into(), kind: DiffKind::Added }],
                "req-1",
            )
            .await
            .unwrap();
    }
}
