//! Bounded, non-fragmented MP4 (AVC + optional AAC-LC) metadata inspector.
//! Enforces strict box/depth/track/sample/memory/time limits without external
//! parsers or transcoders.

use super::*;

pub(super) use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

pub(super) use crate::{
    error::{ApiError, ApiResult},
    storage::media::MediaAsset,
};

pub const MAX_VIDEO_BYTES: usize = 100 * 1024 * 1024;

pub const MAX_MOOV_BYTES: u64 = 2 * 1024 * 1024;

pub const MAX_BOXES: usize = 4096;

pub const MAX_DEPTH: usize = 8;

pub const MAX_TRACKS: usize = 2;

pub const MAX_SAMPLES_TOTAL: u64 = 100_000;

pub const MAX_VIDEO_WIDTH: u32 = 3840;

pub const MAX_VIDEO_HEIGHT: u32 = 2160;

pub const MAX_DURATION_MS: u64 = 600_000;

pub(super) const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedVideo {
    pub id: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub duration_ms: i64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
}

pub(super) struct InspectCtx {
    pub(super) started: Instant,
    pub(super) box_count: usize,
    pub(super) total_samples: u64,
    pub(super) sample_ranges: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BoxHeader {
    pub(super) kind: [u8; 4],
    pub(super) payload_start: u64,
    pub(super) box_end: u64,
}

pub(super) struct SliceBox<'a> {
    pub(super) kind: [u8; 4],
    pub(super) payload: &'a [u8],
}

pub(super) enum TrackResult {
    Video {
        width: u32,
        height: u32,
        codec: String,
    },
    Audio {
        codec: String,
    },
}

pub(super) struct BitReader<'a> {
    pub(super) data: &'a [u8],
    pub(super) bit_pos: usize,
}
