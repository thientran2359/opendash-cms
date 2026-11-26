use super::*;

pub struct PublicSurfaceInspection {
    pub precompiled_body: Option<Arc<template_sec::CompiledCsecComposition>>,
    pub precompiled_shell: PrecompiledPublicShell,
    pub uses_related_entries: bool,
    pub uses_recent_posts: bool,
    pub has_shell_or_theme: bool,
}
