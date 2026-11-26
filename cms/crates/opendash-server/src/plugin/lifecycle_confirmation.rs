//! Bounded, single-use, host-owned confirmation store for destructive or public-exposing
//! plugin entry lifecycle mutations (`content_purge` and immediate-mode public `content_restore`).
//!
//! Never holds a SQLite transaction, admission lease, Wasm worker, guest payload, or secret.
//! All items are discarded on restart, expiry (120s), or maintenance admission epoch advance.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;

use crate::{auth::Role, storage::entry_lifecycle::LifecycleCommand};

pub const MAX_GLOBAL_PENDING_CONFIRMATIONS: usize = 32;
pub const CONFIRMATION_TTL: Duration = Duration::from_secs(120);
pub const MAX_CONFIRMATION_ITEM_BYTES: usize = 64 * 1024;

/// Invocation origin captured when the confirmation was created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleActionOrigin {
    Generic,
    ProviderRow { type_key: String, entry_id: i64 },
}

/// Host-generated bounded confirmation descriptor serialized on HTTP 409 `confirmation_required`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginLifecycleConfirmation {
    pub confirmation_id: String,
    pub operation: String,
    pub type_key: String,
    pub entry_id: i64,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub version: i64,
    pub expires_at: String,
}

/// Stored pending confirmation record owned exclusively by the host.
#[derive(Debug, Clone)]
pub struct PendingLifecycleConfirmation {
    pub confirmation_id: String,
    pub command: LifecycleCommand,
    pub site_id: String,
    pub session_token_hash: String,
    pub actor_id: i64,
    pub actor_role: Role,
    pub plugin_id: String,
    pub action_id: String,
    pub expected_generation: i64,
    pub origin: LifecycleActionOrigin,
    #[allow(dead_code)]
    pub correlation_id: String,
    pub initiating_maintenance_epoch: u64,
    pub display: PluginLifecycleConfirmation,
    expires_at_instant: Instant,
    estimated_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationStoreError {
    ItemTooLarge,
    ScopeAlreadyPending,
    GlobalCapacityReached,
    LockPoisoned,
}

#[derive(Debug, Default)]
pub struct LifecycleConfirmationStore {
    entries: Mutex<HashMap<String, PendingLifecycleConfirmation>>,
}

fn estimate_pending_bytes(item: &PendingLifecycleConfirmation) -> usize {
    let origin_bytes = match &item.origin {
        LifecycleActionOrigin::Generic => 8,
        LifecycleActionOrigin::ProviderRow { type_key, .. } => 16 + type_key.len(),
    };
    256 + item.confirmation_id.len()
        + item.command.type_key.len()
        + item.site_id.len()
        + item.session_token_hash.len()
        + item.plugin_id.len()
        + item.action_id.len()
        + item.correlation_id.len()
        + item.display.confirmation_id.len()
        + item.display.operation.len()
        + item.display.type_key.len()
        + item.display.locale.len()
        + item.display.title.len()
        + item.display.slug.len()
        + item.display.expires_at.len()
        + origin_bytes
}

fn prune_locked(
    map: &mut HashMap<String, PendingLifecycleConfirmation>,
    now: Instant,
    current_epoch: u64,
) {
    map.retain(|_, item| {
        now < item.expires_at_instant && item.initiating_maintenance_epoch == current_epoch
    });
}

#[allow(clippy::too_many_arguments)]
pub struct NewPendingConfirmation {
    pub command: LifecycleCommand,
    pub site_id: String,
    pub session_token_hash: String,
    pub actor_id: i64,
    pub actor_role: Role,
    pub plugin_id: String,
    pub action_id: String,
    pub expected_generation: i64,
    pub origin: LifecycleActionOrigin,
    pub correlation_id: String,
    pub initiating_maintenance_epoch: u64,
    pub locale: String,
    pub title: String,
    pub slug: String,
}

impl LifecycleConfirmationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a bounded single-use confirmation item after pruning expired and obsolete-epoch items.
    pub fn insert(
        &self,
        current_epoch: u64,
        input: NewPendingConfirmation,
    ) -> Result<PluginLifecycleConfirmation, ConfirmationStoreError> {
        self.insert_with_ttl(current_epoch, input, CONFIRMATION_TTL)
    }

    pub fn insert_with_ttl(
        &self,
        current_epoch: u64,
        input: NewPendingConfirmation,
        ttl: Duration,
    ) -> Result<PluginLifecycleConfirmation, ConfirmationStoreError> {
        let now = Instant::now();
        let confirmation_id = format!("plc_{}", crate::auth::generate_random_token());
        let operation = match input.command.operation {
            crate::storage::entry_lifecycle::LifecycleOperation::Purge => "purge",
            crate::storage::entry_lifecycle::LifecycleOperation::Restore => "restore",
            crate::storage::entry_lifecycle::LifecycleOperation::Trash => "trash",
        }
        .to_owned();
        let expires_utc = jiff::Timestamp::now()
            .checked_add(
                jiff::Span::new()
                    .seconds(i64::try_from(ttl.as_secs()).unwrap_or(120))
                    .milliseconds(i64::from(ttl.subsec_millis())),
            )
            .unwrap_or_else(|_| jiff::Timestamp::now())
            .strftime("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let display = PluginLifecycleConfirmation {
            confirmation_id: confirmation_id.clone(),
            operation,
            type_key: input.command.type_key.clone(),
            entry_id: input.command.entry_id,
            locale: input.locale,
            title: opendash_core::hooks::sanitize_plain_text(&input.title, 200),
            slug: opendash_core::hooks::sanitize_plain_text(&input.slug, 200),
            version: input.command.expected_version,
            expires_at: expires_utc,
        };

        let mut pending = PendingLifecycleConfirmation {
            confirmation_id: confirmation_id.clone(),
            command: input.command,
            site_id: input.site_id,
            session_token_hash: input.session_token_hash,
            actor_id: input.actor_id,
            actor_role: input.actor_role,
            plugin_id: input.plugin_id,
            action_id: input.action_id,
            expected_generation: input.expected_generation,
            origin: input.origin,
            correlation_id: input.correlation_id,
            initiating_maintenance_epoch: input.initiating_maintenance_epoch,
            display: display.clone(),
            expires_at_instant: now + ttl,
            estimated_bytes: 0,
        };
        pending.estimated_bytes = estimate_pending_bytes(&pending);
        if pending.estimated_bytes > MAX_CONFIRMATION_ITEM_BYTES {
            return Err(ConfirmationStoreError::ItemTooLarge);
        }

        let mut guard = self
            .entries
            .lock()
            .map_err(|_| ConfirmationStoreError::LockPoisoned)?;
        prune_locked(&mut guard, now, current_epoch);

        if guard.values().any(|existing| {
            existing.actor_id == pending.actor_id
                && existing.session_token_hash == pending.session_token_hash
                && existing.plugin_id == pending.plugin_id
                && existing.action_id == pending.action_id
        }) {
            return Err(ConfirmationStoreError::ScopeAlreadyPending);
        }
        if guard.len() >= MAX_GLOBAL_PENDING_CONFIRMATIONS {
            return Err(ConfirmationStoreError::GlobalCapacityReached);
        }

        guard.insert(confirmation_id, pending);
        Ok(display)
    }

    /// Atomically consumes a matching single-use confirmation item.
    /// Wrong site/session/actor/plugin/action does not consume another principal's pending item
    /// and never discloses target metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn consume(
        &self,
        confirmation_id: &str,
        site_id: &str,
        session_token_hash: &str,
        actor_id: i64,
        plugin_id: &str,
        action_id: &str,
        current_epoch: u64,
    ) -> Result<Option<PendingLifecycleConfirmation>, ConfirmationStoreError> {
        let now = Instant::now();
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| ConfirmationStoreError::LockPoisoned)?;
        prune_locked(&mut guard, now, current_epoch);

        let matches = guard.get(confirmation_id).is_some_and(|item| {
            item.site_id == site_id
                && item.session_token_hash == session_token_hash
                && item.actor_id == actor_id
                && item.plugin_id == plugin_id
                && item.action_id == action_id
                && item.initiating_maintenance_epoch == current_epoch
                && now < item.expires_at_instant
        });
        if !matches {
            return Ok(None);
        }
        Ok(guard.remove(confirmation_id))
    }

    #[cfg(test)]
    pub fn len_unpruned(&self) -> usize {
        self.entries.lock().map(|g| g.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::entry_lifecycle::LifecycleOperation;

    fn sample_input(
        actor_id: i64,
        session: &str,
        plugin: &str,
        action: &str,
        epoch: u64,
    ) -> NewPendingConfirmation {
        NewPendingConfirmation {
            command: LifecycleCommand {
                operation: LifecycleOperation::Purge,
                type_key: "post".into(),
                entry_id: 42,
                expected_version: 3,
            },
            site_id: "site-1".into(),
            session_token_hash: session.into(),
            actor_id,
            actor_role: Role::Admin,
            plugin_id: plugin.into(),
            action_id: action.into(),
            expected_generation: 1,
            origin: LifecycleActionOrigin::Generic,
            correlation_id: "corr-1".into(),
            initiating_maintenance_epoch: epoch,
            locale: "vi".into(),
            title: "Target Post".into(),
            slug: "target-post".into(),
        }
    }

    #[test]
    fn enforces_scope_limit_global_cap_single_use_and_epoch_pruning() {
        let store = LifecycleConfirmationStore::new();
        let first = store
            .insert(0, sample_input(1, "sess-a", "plug", "purge_it", 0))
            .unwrap();
        // Same (actor, session, plugin, action) rejected while pending.
        assert_eq!(
            store.insert(0, sample_input(1, "sess-a", "plug", "purge_it", 0)),
            Err(ConfirmationStoreError::ScopeAlreadyPending)
        );
        // Wrong session cannot consume or delete owner's item.
        assert!(
            store
                .consume(
                    &first.confirmation_id,
                    "site-1",
                    "sess-b",
                    1,
                    "plug",
                    "purge_it",
                    0
                )
                .unwrap()
                .is_none()
        );
        // Matching consume succeeds once; replay returns None.
        let consumed = store
            .consume(
                &first.confirmation_id,
                "site-1",
                "sess-a",
                1,
                "plug",
                "purge_it",
                0,
            )
            .unwrap()
            .expect("matching confirmation");
        assert_eq!(consumed.command.entry_id, 42);
        assert!(
            store
                .consume(
                    &first.confirmation_id,
                    "site-1",
                    "sess-a",
                    1,
                    "plug",
                    "purge_it",
                    0
                )
                .unwrap()
                .is_none()
        );

        // Fill up to global cap of 32 distinct scopes; 33rd fails.
        for i in 0..MAX_GLOBAL_PENDING_CONFIRMATIONS {
            store
                .insert(0, sample_input(1, &format!("s-{i}"), "plug", "purge_it", 0))
                .unwrap();
        }
        assert_eq!(
            store.insert(0, sample_input(1, "s-overflow", "plug", "purge_it", 0)),
            Err(ConfirmationStoreError::GlobalCapacityReached)
        );

        // Advancing maintenance epoch prunes all epoch-0 items before capacity check.
        let after_maint = store
            .insert(1, sample_input(1, "s-after", "plug", "purge_it", 1))
            .unwrap();
        assert_eq!(store.len_unpruned(), 1);
        assert!(
            store
                .consume(
                    &after_maint.confirmation_id,
                    "site-1",
                    "s-after",
                    1,
                    "plug",
                    "purge_it",
                    2
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(store.len_unpruned(), 0);

        // Expired TTL is pruned and rejected.
        let short = store
            .insert_with_ttl(
                2,
                sample_input(1, "s-short", "plug", "purge_it", 2),
                Duration::from_millis(10),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(25));
        assert!(
            store
                .consume(
                    &short.confirmation_id,
                    "site-1",
                    "s-short",
                    1,
                    "plug",
                    "purge_it",
                    2
                )
                .unwrap()
                .is_none()
        );
    }
}
