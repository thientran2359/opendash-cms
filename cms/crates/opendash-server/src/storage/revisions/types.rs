use crate::auth::SessionContext;
use crate::storage::entries::{OwnerFence, PluginActionFence};

/// Attribution metadata recorded alongside each persisted revision row.
#[derive(Debug, Clone, Copy)]
pub enum RevisionActor<'a> {
    User(&'a SessionContext),
    Plugin(&'a str),
    Theme(&'a str),
    Baseline,
    Import,
    System,
}

impl RevisionActor<'_> {
    pub fn into_parts(self) -> (Option<i64>, String) {
        match self {
            Self::User(ctx) => (Some(ctx.user.id), ctx.user.username.clone()),
            Self::Plugin(id) => (None, format!("plugin:{id}")),
            Self::Theme(id) => (None, format!("theme:{id}")),
            Self::Baseline => (None, "baseline".to_owned()),
            Self::Import => (None, "import".to_owned()),
            Self::System => (None, "system".to_owned()),
        }
    }
}

pub fn actor_from_owner_fence<'a>(fence: &'a OwnerFence) -> RevisionActor<'a> {
    match fence {
        OwnerFence::Plugin { plugin_id, .. } => RevisionActor::Plugin(plugin_id.as_str()),
        OwnerFence::Theme { theme_id, .. } => RevisionActor::Theme(theme_id.as_str()),
    }
}

pub fn actor_from_plugin_fence<'a>(fence: &'a PluginActionFence) -> RevisionActor<'a> {
    actor_from_owner_fence(&fence.owner_fence)
}
