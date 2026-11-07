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
}

/// Wraps a registered descriptor with its owner and provider lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedDescriptor<T> {
    pub owner: ResourceOwner,
    pub status: ProviderStatus,
    pub descriptor: T,
}

/// Deterministic in-memory catalog of owned descriptors keyed by canonical ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryCatalog<T> {
    entries: BTreeMap<String, OwnedDescriptor<T>>,
}

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

    /// Registers or updates a descriptor under `key` with the given `owner`.
    pub fn register(&mut self, key: impl Into<String>, owner: ResourceOwner, descriptor: T) {
        self.entries.insert(
            key.into(),
            OwnedDescriptor {
                owner,
                status: ProviderStatus::Active,
                descriptor,
            },
        );
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
    fn preserves_registered_resources_when_plugin_or_theme_deactivates() {
        let mut catalog = RegistryCatalog::new();
        let plugin_owner = ResourceOwner::Plugin("seo-helper".into());
        let user_owner = ResourceOwner::User;

        catalog.register("portfolio", plugin_owner.clone(), "Portfolio CPT");
        catalog.register("company_news", user_owner.clone(), "News CPT");

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
}
