//! Cooperative cancellation token.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

/// Lightweight cancellation token for cooperative task cancellation.
#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    /// Create a new, uncancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Trigger cancellation and notify all waiting tasks.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Check synchronously whether cancellation has been triggered.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Wait asynchronously until cancellation is triggered.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}
