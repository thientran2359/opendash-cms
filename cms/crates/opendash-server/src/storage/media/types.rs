use serde::Serialize;

pub use opendash_core::media::{
    MediaVariantFormat, MediaVariantSpec, MediaVariantView, QueuedMediaVariant,
    build_responsive_srcsets, format_media_etag,
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MediaAsset {
    pub id: String,
    pub content_sha256: String,
    pub kind: String,
    pub filename: String,
    pub mime_type: String,
    pub extension: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumb_width: Option<i64>,
    pub thumb_height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub lifecycle_state: String,
    pub generation: i64,
    pub version: i64,
    pub variants_ready_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl MediaAsset {
    pub fn metadata_etag(&self) -> String {
        format_media_etag(self.generation, self.version)
    }

    #[cfg(test)]
    pub fn test_image_stub(id: String, filename: String) -> Self {
        Self {
            content_sha256: id.clone(),
            id,
            kind: "image".into(),
            filename,
            mime_type: "image/png".into(),
            extension: "png".into(),
            byte_size: 123,
            width: 100,
            height: 100,
            thumb_width: Some(50),
            thumb_height: Some(50),
            duration_ms: None,
            video_codec: None,
            audio_codec: None,
            lifecycle_state: "active".into(),
            generation: 1,
            version: 1,
            variants_ready_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MediaVariantRow {
    pub media_id: String,
    pub version: i64,
    pub spec: String,
    pub format: String,
    pub width: i64,
    pub height: i64,
    pub byte_size: i64,
    #[allow(dead_code)]
    pub relative_path: String,
    #[allow(dead_code)]
    pub sha256: String,
    #[allow(dead_code)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaAssetDetails {
    #[serde(flatten)]
    pub asset: MediaAsset,
    pub variants: Vec<MediaVariantView>,
    pub webp_srcset: Option<String>,
    pub fallback_srcset: Option<String>,
    pub etag: String,
}
