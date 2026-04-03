use tokio::sync::mpsc;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AuctionEvent {
    pub timestamp: i64,
    pub request_id: String,
    pub status: String,
    pub bidder_count: usize,
    pub bid_count: usize,
    pub total_revenue: f64,
}

pub trait AnalyticsBackend: Send + Sync {
    fn log_auction(&self, event: AuctionEvent);
}

pub struct NoopAnalytics;
impl AnalyticsBackend for NoopAnalytics {
    fn log_auction(&self, _: AuctionEvent) {}
}

pub struct FileAnalytics {
    sender: mpsc::UnboundedSender<AuctionEvent>,
}

impl FileAnalytics {
    pub fn new(path: &str) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let path = path.to_string();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            // Open file in append mode
            if let Ok(mut file) = tokio::fs::OpenOptions::new()
                .create(true).append(true).open(&path).await {
                while let Some(event) = rx.recv().await {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = file.write_all(format!("{}\n", json).as_bytes()).await;
                        let _ = file.flush().await;
                    }
                }
            }
        });
        Self { sender: tx }
    }
}

impl AnalyticsBackend for FileAnalytics {
    fn log_auction(&self, event: AuctionEvent) {
        let _ = self.sender.send(event);
    }
}
