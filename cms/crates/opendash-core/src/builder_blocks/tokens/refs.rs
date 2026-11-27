use super::*;
/// Scans `<!-- od:... -->` comments leniently to extract referenced media and
/// pattern IDs, including `Media` fields from active, declared `od:plugin_block`
/// schemas (`ProviderInactive`/`Withdrawn` entries emit zero media).

pub fn extract_block_refs_lenient_with_schemas(
    raw_body: &str,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> ExtractedBlockRefs {
    let mut refs = ExtractedBlockRefs::default();
    if !contains_blocks(raw_body) {
        return refs;
    }
    let mut cursor = 0usize;
    while let Some(rel) = raw_body[cursor..].find(OPEN_PREFIX) {
        let marker_pos = cursor + rel;
        let Some(end_rel) = raw_body[marker_pos..].find(COMMENT_END) else {
            break;
        };
        let comment_end = marker_pos + end_rel + COMMENT_END.len();
        let raw_inner = &raw_body[marker_pos + OPEN_PREFIX.len()..marker_pos + end_rel];
        let body_inner = raw_inner
            .trim()
            .strip_suffix('/')
            .map(str::trim)
            .unwrap_or_else(|| raw_inner.trim());

        if let Ok((tag, json_str)) = split_tag_and_json(body_inner) {
            match tag {
                "section" => {
                    if let Ok(meta) = serde_json::from_str::<ContainerMeta>(json_str)
                        && let Some(bg_media) = meta
                            .style
                            .as_ref()
                            .and_then(|s| s.background_media_id.as_deref())
                            .filter(|id| !id.is_empty())
                    {
                        refs.image_ids.insert(bg_media.to_owned());
                    }
                }
                "image" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<ImageProps>(json_str)
                        && !props.media_id.is_empty()
                        && !props
                            .binding
                            .as_ref()
                            .is_some_and(|b| b.target == BindingTarget::MediaId)
                    {
                        refs.image_ids.insert(props.media_id);
                    }
                }
                "video" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<VideoProps>(json_str) {
                        if !props.media_id.is_empty() {
                            refs.video_ids.insert(props.media_id);
                        }
                        if let Some(poster) = props.poster_image_id
                            && !poster.is_empty()
                        {
                            refs.poster_image_ids.insert(poster);
                        }
                    }
                }
                "pattern_ref" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<PatternRefProps>(json_str) {
                        refs.pattern_ids.insert(props.pattern_id);
                    }
                }
                "plugin_block" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<PluginBlockProps>(json_str)
                        && let Some(entry) = schemas.get(&props.block_id)
                        && entry.status == ProviderStatus::Active
                        && entry.declaration_state == DeclarationState::Declared
                    {
                        let collected = entry.descriptor.schema.collect_media_ids(&props.values);
                        refs.image_ids.extend(collected.image_ids);
                        refs.video_ids.extend(collected.video_ids);
                        refs.file_ids.extend(collected.file_ids);
                        refs.any_media_ids.extend(collected.any_ids);
                        refs.pattern_ids
                            .extend(entry.descriptor.schema.collect_pattern_ids(&props.values));
                    }
                }
                _ => {}
            }
        }
        cursor = comment_end;
    }
    refs
}
