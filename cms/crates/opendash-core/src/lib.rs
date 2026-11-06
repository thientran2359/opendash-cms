//! Shared CMS domain types and validation.
//! Keep this crate independent of HTTP, databases, and plugin execution.

pub mod content;
pub mod plugin;
pub mod theme;

pub const CMS_NAME: &str = "OpenDash";
