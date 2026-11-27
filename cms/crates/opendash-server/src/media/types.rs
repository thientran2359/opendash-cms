//! Content-addressed image file, variant, and thumbnail types & gate definitions.

use super::*;

pub(super) use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub(super) use image::{
    DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits, imageops,
};
pub(super) use opendash_core::media::{
    LARGE_MAX_WIDTH, MEDIUM_MAX_WIDTH, MediaVariantFormat, MediaVariantSpec,
    scaled_dimensions_for_width,
};
pub(super) use sha2::{Digest, Sha256};
pub(super) use tokio::fs;

pub(super) use crate::{
    error::{ApiError, ApiResult},
    storage::media::{MediaAsset, valid_media_id},
};

pub const MAX_MEDIA_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_DIMENSION: u32 = 6000;
pub(super) const MAX_PIXELS: u64 = 20_000_000;
pub(crate) const MAX_ALPHA_LOSSLESS_PIXELS: u64 = 1_000_000;

/// Global decode/ingest semaphores shared across image upload, replace, and regenerate.
pub(crate) static DECODE_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
pub(crate) static VIDEO_INGEST_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(Debug)]
pub struct DecodedMedia {
    pub original: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub id: String,
    pub extension: &'static str,
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub variants: Vec<GeneratedVariant>,
}

#[derive(Debug, Default)]
pub(super) struct MediaReadState {
    pub(super) active_readers: HashMap<String, usize>,
    pub(super) deleting: HashSet<String>,
    pub(super) replacing: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct MediaReadGate {
    pub(super) state: Mutex<MediaReadState>,
    pub(super) notify: tokio::sync::Notify,
}

#[derive(Debug)]
pub struct MediaReadLease {
    pub(super) gate: Arc<MediaReadGate>,
    pub(super) media_id: String,
}

#[derive(Debug)]
pub struct MediaDeleteGateGuard {
    pub(super) gate: Arc<MediaReadGate>,
    pub(super) media_id: String,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct MediaReplaceGateGuard {
    pub(super) gate: Arc<MediaReadGate>,
    pub(super) media_id: String,
}

#[derive(Debug, Clone)]
pub struct MediaStore {
    pub(super) root: PathBuf,
    pub(super) read_gate: Arc<MediaReadGate>,
}
