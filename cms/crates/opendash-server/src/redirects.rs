use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use opendash_core::redirects::{MAX_NOT_FOUND_PATH_BYTES, sanitize_referer, sanitize_user_agent};
use sqlx::SqlitePool;
use tokio::sync::{Mutex as AsyncMutex, MutexGuard, Notify};

use crate::{
    operations::SiteCoordinator,
    storage::redirects::{
        TelemetryFlushBatch, TelemetryNotFoundHit, TelemetryRedirectHit, flush_telemetry_batch,
    },
};

pub const MAX_BUFFER_ENTRIES: usize = 512;
pub const FLUSH_NOTIFY_THRESHOLD: usize = 128;
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(2);
pub const MIN_FLUSH_COOLDOWN: Duration = Duration::from_secs(1);

#[derive(Default)]
struct TelemetryMaps {
    pending_redirect_hits: HashMap<i64, TelemetryRedirectHit>,
    pending_404s: HashMap<String, TelemetryNotFoundHit>,
}

impl TelemetryMaps {
    fn is_empty(&self) -> bool {
        self.pending_redirect_hits.is_empty() && self.pending_404s.is_empty()
    }

    fn total_len(&self) -> usize {
        self.pending_redirect_hits.len() + self.pending_404s.len()
    }
}

pub fn utc_millis_now() -> String {
    let zdt = jiff::Timestamp::now().to_zoned_datetime(jiff::tz::TimeZone::UTC);
    jiff::fmt::strtime::format("%Y-%m-%dT%H:%M:%S%.3fZ", &zdt)
        .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".to_owned())
}

pub struct RedirectTelemetryBuffer {
    db: SqlitePool,
    coordinator: Arc<SiteCoordinator>,
    maps: Mutex<TelemetryMaps>,
    flush_mutex: AsyncMutex<()>,
    last_flush_at: Mutex<Option<Instant>>,
    notify: Notify,
    close_notify: Notify,
    closed: AtomicBool,
}

pub struct RedirectTelemetryDropGuard {
    buffer: Arc<RedirectTelemetryBuffer>,
}

impl Drop for RedirectTelemetryDropGuard {
    fn drop(&mut self) {
        self.buffer.closed.store(true, Ordering::Release);
        self.buffer.notify.notify_waiters();
        self.buffer.close_notify.notify_waiters();
    }
}

impl RedirectTelemetryBuffer {
    pub fn spawn_with_coordinator(
        db: SqlitePool,
        coordinator: Arc<SiteCoordinator>,
    ) -> (Arc<Self>, RedirectTelemetryDropGuard) {
        let buffer = Arc::new(Self {
            db,
            coordinator,
            maps: Mutex::new(TelemetryMaps::default()),
            flush_mutex: AsyncMutex::new(()),
            last_flush_at: Mutex::new(None),
            notify: Notify::new(),
            close_notify: Notify::new(),
            closed: AtomicBool::new(false),
        });
        let worker_buf = Arc::clone(&buffer);
        tokio::spawn(async move {
            worker_buf.run_worker().await;
        });
        let guard = RedirectTelemetryDropGuard {
            buffer: Arc::clone(&buffer),
        };
        (buffer, guard)
    }

    async fn wait_closed(&self) {
        loop {
            let notified = self.close_notify.notified();
            if self.closed.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    async fn run_worker(self: Arc<Self>) {
        loop {
            let notified = self.notify.notified();
            if self.closed.load(Ordering::Acquire) {
                break;
            }
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(FLUSH_INTERVAL) => {}
            }
            if self.closed.load(Ordering::Acquire) {
                break;
            }
            let remaining = {
                let guard = self
                    .last_flush_at
                    .lock()
                    .expect("last_flush_at mutex poisoned");
                guard
                    .map(|t| MIN_FLUSH_COOLDOWN.saturating_sub(t.elapsed()))
                    .unwrap_or(Duration::ZERO)
            };
            if !remaining.is_zero() {
                tokio::select! {
                    _ = tokio::time::sleep(remaining) => {}
                    _ = self.wait_closed() => {
                        break;
                    }
                }
            }
            if self.closed.load(Ordering::Acquire) {
                break;
            }
            let _ = self.flush_now().await;
        }
    }

    pub fn record_redirect_hit(&self, redirect_id: i64) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let now = utc_millis_now();
        let should_notify = {
            let mut maps = self.maps.lock().expect("telemetry maps poisoned");
            if let Some(existing) = maps.pending_redirect_hits.get_mut(&redirect_id) {
                existing.count = existing.count.saturating_add(1);
                existing.last_hit_at = now;
                maps.total_len() >= FLUSH_NOTIFY_THRESHOLD
            } else if maps.pending_redirect_hits.len() < MAX_BUFFER_ENTRIES {
                maps.pending_redirect_hits.insert(
                    redirect_id,
                    TelemetryRedirectHit {
                        count: 1,
                        last_hit_at: now,
                    },
                );
                maps.total_len() >= FLUSH_NOTIFY_THRESHOLD
            } else {
                true
            }
        };
        if should_notify {
            self.notify.notify_one();
        }
    }

    pub fn record_404(&self, path: &str, referer: Option<&str>, user_agent: Option<&str>) {
        if self.closed.load(Ordering::Acquire)
            || path.is_empty()
            || path.len() > MAX_NOT_FOUND_PATH_BYTES
        {
            return;
        }
        let now = utc_millis_now();
        let clean_ref = sanitize_referer(referer);
        let clean_ua = sanitize_user_agent(user_agent);
        let should_notify = {
            let mut maps = self.maps.lock().expect("telemetry maps poisoned");
            if let Some(existing) = maps.pending_404s.get_mut(path) {
                existing.count = existing.count.saturating_add(1);
                existing.last_seen_at = now;
                if clean_ref.is_some() {
                    existing.last_referrer = clean_ref;
                }
                if clean_ua.is_some() {
                    existing.last_user_agent = clean_ua;
                }
                maps.total_len() >= FLUSH_NOTIFY_THRESHOLD
            } else if maps.pending_404s.len() < MAX_BUFFER_ENTRIES {
                maps.pending_404s.insert(
                    path.to_owned(),
                    TelemetryNotFoundHit {
                        count: 1,
                        first_seen_at: now.clone(),
                        last_seen_at: now,
                        last_referrer: clean_ref,
                        last_user_agent: clean_ua,
                    },
                );
                maps.total_len() >= FLUSH_NOTIFY_THRESHOLD
            } else {
                true
            }
        };
        if should_notify {
            self.notify.notify_one();
        }
    }

    pub fn drain_snapshot(&self) -> TelemetryFlushBatch {
        let mut maps = self.maps.lock().expect("telemetry maps poisoned");
        TelemetryFlushBatch {
            redirect_hits: std::mem::take(&mut maps.pending_redirect_hits),
            not_found_hits: std::mem::take(&mut maps.pending_404s),
        }
    }

    pub async fn lock_flush(&self) -> MutexGuard<'_, ()> {
        self.flush_mutex.lock().await
    }

    pub fn evict_redirect_id_locked(&self, _flush_guard: &MutexGuard<'_, ()>, redirect_id: i64) {
        let mut maps = self.maps.lock().expect("telemetry maps poisoned");
        maps.pending_redirect_hits.remove(&redirect_id);
    }

    pub fn evict_not_found_paths_locked(
        &self,
        _flush_guard: &MutexGuard<'_, ()>,
        paths: &[String],
    ) {
        if paths.is_empty() {
            return;
        }
        let mut maps = self.maps.lock().expect("telemetry maps poisoned");
        for path in paths {
            maps.pending_404s.remove(path);
        }
    }

    pub fn clear_pending_404s_locked(&self, _flush_guard: &MutexGuard<'_, ()>) {
        let mut maps = self.maps.lock().expect("telemetry maps poisoned");
        maps.pending_404s.clear();
    }

    /// Lock order: `MutationLease -> flush_mutex -> TelemetryMaps -> SQLite`.
    pub async fn flush_now(&self) -> Result<(), ()> {
        let empty = {
            let maps = self.maps.lock().expect("telemetry maps poisoned");
            maps.is_empty()
        };
        if empty {
            return Ok(());
        }
        let Ok(_lease) = self.coordinator.admit_mutation() else {
            // Respect active maintenance windows (`MaintenanceActive`) without
            // acquiring `flush_mutex` or discarding RAM telemetry.
            return Ok(());
        };
        let _flush_guard = self.flush_mutex.lock().await;
        let snapshot = self.drain_snapshot();
        if snapshot.is_empty() {
            return Ok(());
        }
        {
            let mut last = self
                .last_flush_at
                .lock()
                .expect("last_flush_at mutex poisoned");
            *last = Some(Instant::now());
        }
        flush_telemetry_batch(&self.db, &snapshot)
            .await
            .map_err(|err| {
                eprintln!("redirect telemetry flush error: {err:?}");
            })
    }
}
