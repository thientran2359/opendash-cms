use super::*;

pub struct PublicSurfaceInspection {
    pub precompiled_body: Option<Arc<template_sec::CompiledCsecComposition>>,
    pub precompiled_shell: PrecompiledPublicShell,
    pub uses_related_entries: bool,
    pub uses_recent_posts: bool,
    pub uses_adjacent_global: bool,
    pub uses_adjacent_same_taxonomy: bool,
    pub related_limit: usize,
    pub requested_menu_slots: BTreeSet<String>,
    pub requested_menu_keys: BTreeSet<String>,
    pub loop_projection: opendash_core::builder::LoopProjection,
    pub has_shell_or_theme: bool,
}

#[derive(Default)]
pub(in crate::storage) struct SurfaceScanAcc {
    pub uses_related_entries: bool,
    pub uses_recent_posts: bool,
    pub uses_adjacent_global: bool,
    pub uses_adjacent_same_taxonomy: bool,
    pub related_limit: usize,
    pub requested_menu_slots: BTreeSet<String>,
    pub requested_menu_keys: BTreeSet<String>,
    pub loop_projection: opendash_core::builder::LoopProjection,
    pub inspected_parts: BTreeMap<String, Option<Arc<template_sec::CompiledCsecComposition>>>,
    pub part_expansions: usize,
}

impl SurfaceScanAcc {
    pub(in crate::storage) fn merge_loop_projection(
        &mut self,
        other: &opendash_core::builder::LoopProjection,
    ) {
        self.loop_projection.needs_cover |= other.needs_cover;
        self.loop_projection.needs_excerpt |= other.needs_excerpt;
        self.loop_projection.needs_terms |= other.needs_terms;
        self.loop_projection.needs_primary_category |= other.needs_primary_category;
        self.loop_projection.needs_reading_time |= other.needs_reading_time;
        self.loop_projection.needs_all_fields |= other.needs_all_fields;
        self.loop_projection
            .custom_field_keys
            .extend(other.custom_field_keys.iter().cloned());
    }

    pub(in crate::storage) fn merge_theme_compiled(
        &mut self,
        compiled: &template_sec::CompiledCsecComposition,
    ) {
        self.uses_related_entries |= compiled.uses_related_entries;
        self.uses_recent_posts |= compiled.uses_recent_posts;
        if compiled.uses_related_entries {
            self.related_limit = self.related_limit.max(6);
        }
        self.loop_projection = opendash_core::builder::LoopProjection::all();
    }

    pub(in crate::storage) fn into_inspection(
        self,
        precompiled_body: Option<Arc<template_sec::CompiledCsecComposition>>,
        precompiled_shell: PrecompiledPublicShell,
        has_shell_or_theme: bool,
    ) -> PublicSurfaceInspection {
        PublicSurfaceInspection {
            precompiled_body,
            precompiled_shell,
            uses_related_entries: self.uses_related_entries,
            uses_recent_posts: self.uses_recent_posts,
            uses_adjacent_global: self.uses_adjacent_global,
            uses_adjacent_same_taxonomy: self.uses_adjacent_same_taxonomy,
            related_limit: self.related_limit,
            requested_menu_slots: self.requested_menu_slots,
            requested_menu_keys: self.requested_menu_keys,
            loop_projection: self.loop_projection,
            has_shell_or_theme,
        }
    }
}

fn scan_builder_nodes_template_requirements(
    nodes: &[opendash_core::builder::BlockNode],
    acc: &mut SurfaceScanAcc,
) {
    for node in nodes {
        match node.block_id.as_str() {
            "core/entry-nav" => {
                let same_tax = node
                    .props
                    .get("same_taxonomy")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if same_tax {
                    acc.uses_adjacent_same_taxonomy = true;
                } else {
                    acc.uses_adjacent_global = true;
                }
            }
            "core/entry-related" => {
                acc.uses_related_entries = true;
                let limit = node
                    .props
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(4)
                    .clamp(2, 8) as usize;
                acc.related_limit = acc.related_limit.max(limit);
            }
            "core/menu" => {
                let mode = node
                    .props
                    .get("source_mode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("slot");
                if mode == "menu" {
                    if let Some(key) = node
                        .props
                        .get("menu_key")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                    {
                        acc.requested_menu_keys.insert(key.to_owned());
                    }
                } else if let Some(slot) = node
                    .props
                    .get("slot_key")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    acc.requested_menu_slots.insert(slot.to_owned());
                }
            }
            _ => {}
        }
        scan_builder_nodes_template_requirements(&node.children, acc);
    }
}

pub(in crate::storage) fn inspect_document_template_requirements(
    doc: &BuilderDocument,
    acc: &mut SurfaceScanAcc,
) {
    scan_builder_nodes_template_requirements(&doc.blocks, acc);
    let lp = opendash_core::builder::inspect_loop_projection(doc);
    acc.merge_loop_projection(&lp);
}
