//! Unified CMS Registry and resource ownership primitives.
//!
//! Ensures that CPTs, field schemas, blocks, templates, and settings track
//! their declaring owner (`Core`, `User`, `Theme`, `Plugin`) and remain
//! manageable when a theme changes or a plugin is disabled.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Identifies who declared or owns a registered CMS resource.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ResourceOwner {
    Core,
    User,
    Theme(String),
    Plugin(String),
}

/// Lifecycle availability of the provider that registered a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    /// The owning core/user/theme/plugin is currently active.
    Active,
    /// The owning theme was switched or the owning plugin was disabled.
    /// Schema and existing content remain preserved and editable.
    ProviderInactive,
    /// The provider package/revision is no longer present on disk/registry.
    Missing,
    /// The provider package/revision failed validation or integrity checks.
    Invalid,
}

/// Whether the owning provider still declares this resource in its current revision,
/// or withdrew the declaration while retaining site-owned schema and data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationState {
    #[default]
    Declared,
    Withdrawn,
}

/// Wraps a registered descriptor with its owner, provider availability, and declaration state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedDescriptor<T> {
    pub owner: ResourceOwner,
    pub status: ProviderStatus,
    #[serde(default)]
    pub declaration_state: DeclarationState,
    pub descriptor: T,
}

/// Deterministic in-memory catalog of owned descriptors keyed by canonical ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryCatalog<T> {
    entries: BTreeMap<String, OwnedDescriptor<T>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    AlreadyRegistered,
    OwnerConflict,
    NotFound,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AlreadyRegistered => "Resource already registered",
            Self::OwnerConflict => "Resource belongs to another owner",
            Self::NotFound => "Resource not found",
        })
    }
}
impl std::error::Error for RegistryError {}

impl<T> Default for RegistryCatalog<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

impl<T> RegistryCatalog<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new resource. Updates and provider activation are explicit operations.
    pub fn register(
        &mut self,
        key: impl Into<String>,
        owner: ResourceOwner,
        descriptor: T,
    ) -> Result<(), RegistryError> {
        let key = key.into();
        if let Some(existing) = self.entries.get(&key) {
            return Err(if existing.owner == owner {
                RegistryError::AlreadyRegistered
            } else {
                RegistryError::OwnerConflict
            });
        }
        self.entries.insert(
            key,
            OwnedDescriptor {
                owner,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Declared,
                descriptor,
            },
        );
        Ok(())
    }

    /// Inserts or replaces an entry hydrated directly from persistent storage.
    pub fn insert_loaded(&mut self, key: impl Into<String>, entry: OwnedDescriptor<T>) {
        self.entries.insert(key.into(), entry);
    }

    /// Explicitly declares or re-declares a resource for `owner` without
    /// altering another owner's claim (even if inactive or withdrawn).
    pub fn redeclare(
        &mut self,
        key: impl Into<String>,
        owner: ResourceOwner,
        status: ProviderStatus,
        descriptor: T,
    ) -> Result<(), RegistryError> {
        let key = key.into();
        if let Some(existing) = self.entries.get(&key) {
            if existing.owner != owner {
                return Err(RegistryError::OwnerConflict);
            }
        }
        self.entries.insert(
            key,
            OwnedDescriptor {
                owner,
                status,
                declaration_state: DeclarationState::Declared,
                descriptor,
            },
        );
        Ok(())
    }

    /// Marks an owned resource as `Withdrawn` while retaining its descriptor and data.
    pub fn withdraw(&mut self, key: &str, owner: &ResourceOwner) -> Result<(), RegistryError> {
        let entry = self.entries.get_mut(key).ok_or(RegistryError::NotFound)?;
        if &entry.owner != owner {
            return Err(RegistryError::OwnerConflict);
        }
        entry.declaration_state = DeclarationState::Withdrawn;
        Ok(())
    }

    /// Marks all entries owned by `target_owner` whose keys are not in `active_keys`
    /// as `DeclarationState::Withdrawn`, retaining their descriptors.
    pub fn withdraw_owner_except(
        &mut self,
        target_owner: &ResourceOwner,
        active_keys: &[&str],
    ) -> usize {
        let mut count = 0;
        for (key, entry) in self.entries.iter_mut() {
            if &entry.owner == target_owner
                && !active_keys.contains(&key.as_str())
                && entry.declaration_state != DeclarationState::Withdrawn
            {
                entry.declaration_state = DeclarationState::Withdrawn;
                count += 1;
            }
        }
        count
    }

    /// Updates provider metadata only; never implicitly activates a provider.
    pub fn update(
        &mut self,
        key: &str,
        owner: &ResourceOwner,
        descriptor: T,
    ) -> Result<(), RegistryError> {
        let entry = self.entries.get_mut(key).ok_or(RegistryError::NotFound)?;
        if &entry.owner != owner {
            return Err(RegistryError::OwnerConflict);
        }
        entry.descriptor = descriptor;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&OwnedDescriptor<T>> {
        self.entries.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &OwnedDescriptor<T>)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Marks all entries owned by `target_owner` as `ProviderInactive` without
    /// deleting their definitions or user data.
    pub fn deactivate_owner(&mut self, target_owner: &ResourceOwner) -> usize {
        let mut count = 0;
        for entry in self.entries.values_mut() {
            if &entry.owner == target_owner && entry.status != ProviderStatus::ProviderInactive {
                entry.status = ProviderStatus::ProviderInactive;
                count += 1;
            }
        }
        count
    }

    /// Restores entries owned by `target_owner` back to `ProviderStatus::Active`.
    pub fn activate_owner(&mut self, target_owner: &ResourceOwner) -> usize {
        let mut count = 0;
        for entry in self.entries.values_mut() {
            if &entry.owner == target_owner && entry.status != ProviderStatus::Active {
                entry.status = ProviderStatus::Active;
                count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_owner_takeover_and_keeps_inactive_status_on_update() {
        let mut catalog = RegistryCatalog::new();
        let owner = ResourceOwner::Plugin("first".into());
        catalog
            .register("portfolio", owner.clone(), "original")
            .unwrap();
        assert_eq!(
            catalog.register("portfolio", ResourceOwner::User, "stolen"),
            Err(RegistryError::OwnerConflict)
        );
        assert_eq!(
            catalog.register("portfolio", owner.clone(), "replacement"),
            Err(RegistryError::AlreadyRegistered)
        );
        catalog.deactivate_owner(&owner);
        assert_eq!(
            catalog.update("portfolio", &ResourceOwner::User, "stolen"),
            Err(RegistryError::OwnerConflict)
        );
        assert_eq!(catalog.get("portfolio").unwrap().descriptor, "original");
        catalog
            .update("portfolio", &owner, "new definition")
            .unwrap();
        assert_eq!(
            catalog.get("portfolio").unwrap().status,
            ProviderStatus::ProviderInactive
        );
        assert_eq!(
            catalog.update("missing", &owner, "new"),
            Err(RegistryError::NotFound)
        );
        catalog.activate_owner(&owner);
        assert_eq!(
            catalog.get("portfolio").unwrap().descriptor,
            "new definition"
        );
    }

    #[test]
    fn preserves_registered_resources_when_plugin_or_theme_deactivates() {
        let mut catalog = RegistryCatalog::new();
        let plugin_owner = ResourceOwner::Plugin("seo-helper".into());
        let user_owner = ResourceOwner::User;

        catalog
            .register("portfolio", plugin_owner.clone(), "Portfolio CPT")
            .unwrap();
        catalog
            .register("company_news", user_owner.clone(), "News CPT")
            .unwrap();

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.deactivate_owner(&plugin_owner), 1);

        // Resource is preserved even when the plugin is disabled
        let portfolio = catalog.get("portfolio").expect("portfolio must remain");
        assert_eq!(portfolio.status, ProviderStatus::ProviderInactive);
        assert_eq!(portfolio.descriptor, "Portfolio CPT");

        // User-owned resource remains active
        let news = catalog
            .get("company_news")
            .expect("company_news must remain");
        assert_eq!(news.status, ProviderStatus::Active);

        // Re-enabling the plugin restores active status
        assert_eq!(catalog.activate_owner(&plugin_owner), 1);
        assert_eq!(
            catalog.get("portfolio").unwrap().status,
            ProviderStatus::Active
        );
    }

    #[test]
    fn declaration_state_is_independent_from_provider_status() {
        let mut catalog = RegistryCatalog::new();
        let theme_owner = ResourceOwner::Theme("theme-a".into());
        catalog
            .redeclare(
                "portfolio",
                theme_owner.clone(),
                ProviderStatus::Active,
                "v1",
            )
            .unwrap();
        catalog
            .redeclare("event", theme_owner.clone(), ProviderStatus::Active, "v1")
            .unwrap();

        // Omitting `event` withdraws it without losing ownership or descriptor
        assert_eq!(catalog.withdraw_owner_except(&theme_owner, &["portfolio"]), 1);
        let event = catalog.get("event").unwrap();
        assert_eq!(event.declaration_state, DeclarationState::Withdrawn);
        assert_eq!(event.status, ProviderStatus::Active);

        // Bulk activate_owner does NOT un-withdraw `event`
        catalog.deactivate_owner(&theme_owner);
        catalog.activate_owner(&theme_owner);
        assert_eq!(
            catalog.get("event").unwrap().declaration_state,
            DeclarationState::Withdrawn
        );
        assert_eq!(
            catalog.get("event").unwrap().status,
            ProviderStatus::Active
        );

        // Other owner still cannot claim a withdrawn resource
        assert_eq!(
            catalog.redeclare(
                "event",
                ResourceOwner::Theme("theme-b".into()),
                ProviderStatus::Active,
                "hijack"
            ),
            Err(RegistryError::OwnerConflict)
        );
    }
}
