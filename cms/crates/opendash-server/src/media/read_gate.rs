//! Async zero-TOCTOU read gate and drain/swap serialization for `MediaStore`.

use super::*;

impl Drop for MediaReadLease {
    fn drop(&mut self) {
        let mut guard = self.gate.state.lock().expect("media read gate poisoned");
        if let Some(count) = guard.active_readers.get_mut(&self.media_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                guard.active_readers.remove(&self.media_id);
                self.gate.notify.notify_waiters();
            }
        }
    }
}

impl Drop for MediaDeleteGateGuard {
    fn drop(&mut self) {
        let mut guard = self.gate.state.lock().expect("media read gate poisoned");
        guard.deleting.remove(&self.media_id);
        self.gate.notify.notify_waiters();
    }
}

impl Drop for MediaReplaceGateGuard {
    fn drop(&mut self) {
        let mut guard = self.gate.state.lock().expect("media read gate poisoned");
        guard.replacing.remove(&self.media_id);
        self.gate.notify.notify_waiters();
    }
}

impl MediaStore {
    pub async fn acquire_read_lease(&self, id: &str) -> ApiResult<MediaReadLease> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let notified = self.read_gate.notify.notified();
            tokio::pin!(notified);
            {
                let mut guard = self
                    .read_gate
                    .state
                    .lock()
                    .expect("media read gate poisoned");
                if guard.deleting.contains(id) {
                    return Err(ApiError::not_found("Không tìm thấy media."));
                }
                if !guard.replacing.contains(id) {
                    *guard.active_readers.entry(id.to_string()).or_insert(0) += 1;
                    return Ok(MediaReadLease {
                        gate: Arc::clone(&self.read_gate),
                        media_id: id.to_string(),
                    });
                }
                // Register waiter while still holding the mutex to prevent lost wakeups.
                notified.as_mut().enable();
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(ApiError::service_unavailable(
                    "Media đang được cập nhật; vui lòng thử lại sau.",
                ));
            }
            let _ = tokio::time::timeout(deadline.saturating_duration_since(now), notified).await;
        }
    }

    pub fn has_active_reader(&self, id: &str) -> bool {
        let guard = self
            .read_gate
            .state
            .lock()
            .expect("media read gate poisoned");
        guard.active_readers.get(id).copied().unwrap_or(0) > 0
    }

    /// Waits up to `timeout` for existing readers to finish BEFORE flipping `replacing`.
    /// Public readers are never stalled during this pre-check.
    #[allow(dead_code)]
    pub async fn wait_for_no_active_readers_before_swap(
        &self,
        id: &str,
        timeout: Duration,
    ) -> bool {
        let deadline = tokio::time::Instant::now() + timeout.min(Duration::from_secs(2));
        loop {
            let notified = self.read_gate.notify.notified();
            tokio::pin!(notified);
            {
                let guard = self
                    .read_gate
                    .state
                    .lock()
                    .expect("media read gate poisoned");
                if guard.active_readers.get(id).copied().unwrap_or(0) == 0 {
                    return true;
                }
                notified.as_mut().enable();
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let _ = tokio::time::timeout(deadline.saturating_duration_since(now), notified).await;
        }
    }

    #[allow(dead_code)]
    pub fn try_begin_replace_swap(&self, id: &str) -> Option<MediaReplaceGateGuard> {
        let mut guard = self
            .read_gate
            .state
            .lock()
            .expect("media read gate poisoned");
        if guard.deleting.contains(id)
            || guard.replacing.contains(id)
            || guard.active_readers.get(id).copied().unwrap_or(0) > 0
        {
            return None;
        }
        guard.replacing.insert(id.to_string());
        Some(MediaReplaceGateGuard {
            gate: Arc::clone(&self.read_gate),
            media_id: id.to_string(),
        })
    }

    pub async fn begin_delete_drain(
        &self,
        id: &str,
        timeout: Duration,
    ) -> Result<MediaDeleteGateGuard, MediaDeleteGateGuard> {
        let effective = timeout.min(Duration::from_secs(5));
        let deadline = tokio::time::Instant::now() + effective;
        {
            let mut guard = self
                .read_gate
                .state
                .lock()
                .expect("media read gate poisoned");
            guard.deleting.insert(id.to_string());
        }
        let gate_guard = MediaDeleteGateGuard {
            gate: Arc::clone(&self.read_gate),
            media_id: id.to_string(),
        };

        loop {
            let notified = self.read_gate.notify.notified();
            tokio::pin!(notified);
            {
                let guard = self
                    .read_gate
                    .state
                    .lock()
                    .expect("media read gate poisoned");
                if guard.active_readers.get(id).copied().unwrap_or(0) == 0
                    && !guard.replacing.contains(id)
                {
                    return Ok(gate_guard);
                }
                notified.as_mut().enable();
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(gate_guard);
            }
            let _ = tokio::time::timeout(deadline.saturating_duration_since(now), notified).await;
        }
    }
}
