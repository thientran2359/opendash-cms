//! Bounded, non-fragmented MP4 (AVC + optional AAC-LC) metadata inspector.
//! Enforces strict box/depth/track/sample/memory/time limits without external
//! parsers or transcoders.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

use crate::{
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
const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);

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

impl ValidatedVideo {
    pub fn metadata(&self, filename: String) -> MediaAsset {
        MediaAsset {
            id: self.id.clone(),
            kind: "video".to_string(),
            filename,
            mime_type: "video/mp4".to_string(),
            extension: "mp4".to_string(),
            byte_size: self.byte_size,
            width: self.width,
            height: self.height,
            thumb_width: None,
            thumb_height: None,
            duration_ms: Some(self.duration_ms),
            video_codec: Some(self.video_codec.clone()),
            audio_codec: self.audio_codec.clone(),
            created_at: String::new(),
        }
    }
}

struct InspectCtx {
    started: Instant,
    box_count: usize,
    total_samples: u64,
    sample_ranges: Vec<(u64, u64)>,
}

impl InspectCtx {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            box_count: 0,
            total_samples: 0,
            sample_ranges: Vec::new(),
        }
    }

    fn check_timeout(&self) -> ApiResult<()> {
        if self.started.elapsed() > INSPECT_TIMEOUT {
            return Err(ApiError::bad_request(
                "Video MP4 vượt quá thời gian kiểm tra metadata.",
            ));
        }
        Ok(())
    }

    fn count_box(&mut self) -> ApiResult<()> {
        self.box_count += 1;
        if self.box_count > MAX_BOXES {
            return Err(ApiError::bad_request("Video MP4 vượt quá số lượng box cho phép."));
        }
        if self.box_count.is_multiple_of(128) {
            self.check_timeout()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct BoxHeader {
    kind: [u8; 4],
    payload_start: u64,
    box_end: u64,
}

fn read_top_box_header(file: &mut File, pos: u64, file_size: u64) -> ApiResult<BoxHeader> {
    if pos + 8 > file_size {
        return Err(ApiError::bad_request("Box MP4 bị cắt cụt."));
    }
    file.seek(SeekFrom::Start(pos))
        .map_err(|_| ApiError::bad_request("Không đọc được header MP4."))?;
    let mut hdr = [0u8; 8];
    file.read_exact(&mut hdr)
        .map_err(|_| ApiError::bad_request("Không đọc được header MP4."))?;
    let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
    let kind = [hdr[4], hdr[5], hdr[6], hdr[7]];
    let (header_size, box_end) = match size32 {
        0 => (8u64, file_size),
        1 => {
            if pos + 16 > file_size {
                return Err(ApiError::bad_request("Box MP4 64-bit bị cắt cụt."));
            }
            let mut ext = [0u8; 8];
            file.read_exact(&mut ext)
                .map_err(|_| ApiError::bad_request("Không đọc được kích thước 64-bit MP4."))?;
            let size64 = u64::from_be_bytes(ext);
            let end = pos
                .checked_add(size64)
                .ok_or_else(|| ApiError::bad_request("Kích thước box MP4 bị tràn số."))?;
            (16u64, end)
        }
        n => {
            let end = pos
                .checked_add(n)
                .ok_or_else(|| ApiError::bad_request("Kích thước box MP4 bị tràn số."))?;
            (8u64, end)
        }
    };
    if box_end < pos + header_size || box_end > file_size {
        return Err(ApiError::bad_request("Kích thước box MP4 vượt ngoài phạm vi file."));
    }
    Ok(BoxHeader {
        kind,
        payload_start: pos + header_size,
        box_end,
    })
}

struct SliceBox<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

fn iter_boxes<'a>(
    mut data: &'a [u8],
    ctx: &mut InspectCtx,
    depth: usize,
) -> ApiResult<Vec<SliceBox<'a>>> {
    if depth > MAX_DEPTH {
        return Err(ApiError::bad_request("Cấu trúc box MP4 lồng nhau quá sâu."));
    }
    let mut out = Vec::new();
    while !data.is_empty() {
        ctx.count_box()?;
        if data.len() < 8 {
            return Err(ApiError::bad_request("Sub-box MP4 bị cắt cụt."));
        }
        let size32 = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let kind = [data[4], data[5], data[6], data[7]];
        let (hdr_len, total_len) = match size32 {
            0 => (8usize, data.len()),
            1 => {
                if data.len() < 16 {
                    return Err(ApiError::bad_request("Sub-box MP4 64-bit bị cắt cụt."));
                }
                let size64 = u64::from_be_bytes([
                    data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
                ]);
                let total = usize::try_from(size64)
                    .map_err(|_| ApiError::bad_request("Sub-box MP4 quá lớn."))?;
                (16usize, total)
            }
            n => (8usize, n),
        };
        if total_len < hdr_len || total_len > data.len() {
            return Err(ApiError::bad_request("Sub-box MP4 vượt phạm vi container."));
        }
        out.push(SliceBox {
            kind,
            payload: &data[hdr_len..total_len],
        });
        data = &data[total_len..];
    }
    Ok(out)
}

pub fn inspect_mp4(path: &Path, file_size: u64, sha256: String) -> ApiResult<ValidatedVideo> {
    if file_size == 0 || file_size > MAX_VIDEO_BYTES as u64 {
        return Err(ApiError::bad_request(
            "Video trống hoặc vượt quá giới hạn 100 MiB.",
        ));
    }
    let mut file = File::open(path)
        .map_err(|_| ApiError::internal("Không mở được file video tạm để kiểm tra."))?;
    let actual_len = file
        .metadata()
        .map_err(|_| ApiError::internal("Không đọc được kích thước file video tạm."))?
        .len();
    if actual_len != file_size {
        return Err(ApiError::bad_request("Kích thước file video không khớp."));
    }

    let mut ctx = InspectCtx::new();
    let mut pos = 0u64;
    let mut seen_ftyp = false;
    let mut moov_bytes: Option<Vec<u8>> = None;
    let mut mdat_ranges: Vec<(u64, u64)> = Vec::new();

    while pos < file_size {
        ctx.count_box()?;
        let hdr = read_top_box_header(&mut file, pos, file_size)?;
        let payload_len = hdr.box_end - hdr.payload_start;
        match &hdr.kind {
            b"ftyp" => {
                if pos != 0 || seen_ftyp {
                    return Err(ApiError::bad_request("Box ftyp MP4 không hợp lệ."));
                }
                if !(8..=4096).contains(&payload_len) {
                    return Err(ApiError::bad_request("Box ftyp MP4 có kích thước không hợp lệ."));
                }
                let mut buf = vec![0u8; payload_len as usize];
                file.seek(SeekFrom::Start(hdr.payload_start))
                    .and_then(|_| file.read_exact(&mut buf))
                    .map_err(|_| ApiError::bad_request("Không đọc được box ftyp MP4."))?;
                validate_ftyp(&buf)?;
                seen_ftyp = true;
            }
            b"moov" => {
                if !seen_ftyp {
                    return Err(ApiError::bad_request("MP4 thiếu ftyp ở đầu file."));
                }
                if moov_bytes.is_some() {
                    return Err(ApiError::bad_request("MP4 chỉ được phép có đúng một box moov."));
                }
                if payload_len == 0 || payload_len > MAX_MOOV_BYTES {
                    return Err(ApiError::bad_request(
                        "Metadata moov của MP4 trống hoặc vượt quá 2 MiB.",
                    ));
                }
                let mut buf = vec![0u8; payload_len as usize];
                file.seek(SeekFrom::Start(hdr.payload_start))
                    .and_then(|_| file.read_exact(&mut buf))
                    .map_err(|_| ApiError::bad_request("Không đọc được box moov MP4."))?;
                moov_bytes = Some(buf);
            }
            b"mdat" => {
                if !seen_ftyp {
                    return Err(ApiError::bad_request("MP4 thiếu ftyp ở đầu file."));
                }
                if payload_len > 0 {
                    mdat_ranges.push((hdr.payload_start, hdr.box_end));
                }
            }
            b"free" | b"skip" | b"wide" => {}
            b"moof" | b"mfra" | b"sidx" => {
                return Err(ApiError::bad_request(
                    "Không hỗ trợ MP4 phân mảnh (fragmented MP4).",
                ));
            }
            _ => {
                return Err(ApiError::bad_request(
                    "MP4 chứa top-level box không được phép.",
                ));
            }
        }
        pos = hdr.box_end;
    }

    if !seen_ftyp || mdat_ranges.is_empty() {
        return Err(ApiError::bad_request("MP4 thiếu ftyp hoặc mdat hợp lệ."));
    }
    let moov = moov_bytes.ok_or_else(|| ApiError::bad_request("MP4 thiếu box moov."))?;
    parse_moov(&moov, &mdat_ranges, &mut ctx, file_size, sha256)
}

fn validate_ftyp(payload: &[u8]) -> ApiResult<()> {
    if payload.len() < 8 || !payload.len().is_multiple_of(4) {
        return Err(ApiError::bad_request("Box ftyp MP4 sai định dạng."));
    }
    let major = &payload[0..4];
    let mut compatible = major == b"isom" || major == b"mp42";
    for chunk in payload[8..].chunks_exact(4) {
        if chunk == b"isom" || chunk == b"mp42" {
            compatible = true;
            break;
        }
    }
    if !compatible {
        return Err(ApiError::unsupported_media_type(
            "MP4 phải tương thích isom hoặc mp42.",
        ));
    }
    Ok(())
}

fn parse_moov(
    moov: &[u8],
    mdat_ranges: &[(u64, u64)],
    ctx: &mut InspectCtx,
    file_size: u64,
    sha256: String,
) -> ApiResult<ValidatedVideo> {
    let boxes = iter_boxes(moov, ctx, 1)?;
    let mut duration_ms: Option<u64> = None;
    let mut traks: Vec<&[u8]> = Vec::new();

    for b in boxes {
        match &b.kind {
            b"mvhd" => {
                if duration_ms.is_some() {
                    return Err(ApiError::bad_request("MP4 có nhiều hơn một mvhd."));
                }
                duration_ms = Some(parse_mvhd(b.payload)?);
            }
            b"trak" => {
                if traks.len() >= MAX_TRACKS {
                    return Err(ApiError::bad_request("MP4 vượt quá tối đa 2 track."));
                }
                traks.push(b.payload);
            }
            b"mvex" | b"moof" | b"pssh" | b"emsg" => {
                return Err(ApiError::bad_request(
                    "Không hỗ trợ MP4 phân mảnh hoặc mã hóa.",
                ));
            }
            b"udta" | b"free" | b"skip" | b"iods" | b"meta" => {}
            _ => {}
        }
    }

    let duration_ms =
        duration_ms.ok_or_else(|| ApiError::bad_request("MP4 thiếu box mvhd."))?;

    let mut video_info: Option<(u32, u32, String)> = None;
    let mut audio_codec: Option<String> = None;

    for trak in traks {
        match parse_trak(trak, mdat_ranges, ctx)? {
            TrackResult::Video {
                width,
                height,
                codec,
            } => {
                if video_info.is_some() {
                    return Err(ApiError::bad_request(
                        "MP4 chỉ được phép có đúng một video track.",
                    ));
                }
                video_info = Some((width, height, codec));
            }
            TrackResult::Audio { codec } => {
                if audio_codec.is_some() {
                    return Err(ApiError::bad_request(
                        "MP4 chỉ được phép có tối đa một audio track.",
                    ));
                }
                audio_codec = Some(codec);
            }
        }
    }

    let (width, height, video_codec) =
        video_info.ok_or_else(|| ApiError::bad_request("MP4 phải có một video track AVC."))?;

    // Check that no sample byte ranges overlap across any track.
    ctx.sample_ranges.sort_unstable_by_key(|&(start, _)| start);
    for pair in ctx.sample_ranges.windows(2) {
        ctx.check_timeout()?;
        if pair[0].1 > pair[1].0 {
            return Err(ApiError::bad_request(
                "MP4 có sample byte range chồng lấn.",
            ));
        }
    }

    Ok(ValidatedVideo {
        id: sha256,
        byte_size: file_size as i64,
        width: i64::from(width),
        height: i64::from(height),
        duration_ms: duration_ms as i64,
        video_codec,
        audio_codec,
    })
}

fn parse_mvhd(payload: &[u8]) -> ApiResult<u64> {
    if payload.len() < 4 {
        return Err(ApiError::bad_request("Box mvhd bị cắt cụt."));
    }
    let version = payload[0];
    let (timescale, duration) = match version {
        0 => {
            if payload.len() < 24 {
                return Err(ApiError::bad_request("Box mvhd v0 bị cắt cụt."));
            }
            let ts = u32::from_be_bytes([payload[12], payload[13], payload[14], payload[15]]) as u64;
            let dur =
                u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]) as u64;
            (ts, dur)
        }
        1 => {
            if payload.len() < 36 {
                return Err(ApiError::bad_request("Box mvhd v1 bị cắt cụt."));
            }
            let ts = u32::from_be_bytes([payload[20], payload[21], payload[22], payload[23]]) as u64;
            let dur = u64::from_be_bytes([
                payload[24], payload[25], payload[26], payload[27], payload[28], payload[29],
                payload[30], payload[31],
            ]);
            (ts, dur)
        }
        _ => return Err(ApiError::bad_request("Phiên bản mvhd không hợp lệ.")),
    };
    if timescale == 0 || duration == 0 {
        return Err(ApiError::bad_request("Timescale hoặc duration MP4 bằng 0."));
    }
    let ms = u128::from(duration)
        .checked_mul(1000)
        .map(|v| v / u128::from(timescale))
        .ok_or_else(|| ApiError::bad_request("Duration MP4 bị tràn số."))?;
    let ms_u64 = u64::try_from(ms)
        .map_err(|_| ApiError::bad_request("Duration MP4 vượt giới hạn."))?;
    if ms_u64 == 0 || ms_u64 > MAX_DURATION_MS {
        return Err(ApiError::bad_request(
            "Thời lượng video vượt quá giới hạn 1..600000 ms.",
        ));
    }
    Ok(ms_u64)
}

enum TrackResult {
    Video {
        width: u32,
        height: u32,
        codec: String,
    },
    Audio {
        codec: String,
    },
}

fn parse_trak(
    trak: &[u8],
    mdat_ranges: &[(u64, u64)],
    ctx: &mut InspectCtx,
) -> ApiResult<TrackResult> {
    let boxes = iter_boxes(trak, ctx, 2)?;
    let mut tkhd_dims: Option<(u32, u32)> = None;
    let mut mdia_payload: Option<&[u8]> = None;

    for b in boxes {
        match &b.kind {
            b"tkhd" => tkhd_dims = Some(parse_tkhd(b.payload)?),
            b"mdia" => mdia_payload = Some(b.payload),
            b"tref" | b"edts" | b"udta" | b"free" | b"skip" => {}
            _ => {}
        }
    }

    let (tkhd_w, tkhd_h) =
        tkhd_dims.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu tkhd."))?;
    let mdia = mdia_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu mdia."))?;
    let mdia_boxes = iter_boxes(mdia, ctx, 3)?;

    let mut handler: Option<[u8; 4]> = None;
    let mut minf_payload: Option<&[u8]> = None;
    let mut has_mdhd = false;

    for b in mdia_boxes {
        match &b.kind {
            b"mdhd" => {
                validate_mdhd(b.payload)?;
                has_mdhd = true;
            }
            b"hdlr" => {
                if b.payload.len() < 12 {
                    return Err(ApiError::bad_request("Box hdlr bị cắt cụt."));
                }
                handler = Some([b.payload[8], b.payload[9], b.payload[10], b.payload[11]]);
            }
            b"minf" => minf_payload = Some(b.payload),
            _ => {}
        }
    }

    if !has_mdhd {
        return Err(ApiError::bad_request("Track MP4 thiếu mdhd."));
    }
    let handler = handler.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu hdlr."))?;
    if &handler != b"vide" && &handler != b"soun" {
        return Err(ApiError::unsupported_media_type(
            "MP4 chứa loại track không được hỗ trợ.",
        ));
    }

    let minf = minf_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu minf."))?;
    let minf_boxes = iter_boxes(minf, ctx, 4)?;
    let mut stbl_payload: Option<&[u8]> = None;
    let mut has_dinf = false;

    for b in minf_boxes {
        match &b.kind {
            b"dinf" => {
                validate_dinf(b.payload, ctx)?;
                has_dinf = true;
            }
            b"stbl" => stbl_payload = Some(b.payload),
            _ => {}
        }
    }
    if !has_dinf {
        return Err(ApiError::bad_request("Track MP4 thiếu dinf."));
    }

    let stbl = stbl_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stbl."))?;
    let stbl_boxes = iter_boxes(stbl, ctx, 5)?;

    let mut stsd_payload: Option<&[u8]> = None;
    let mut stts_payload: Option<&[u8]> = None;
    let mut stsc_payload: Option<&[u8]> = None;
    let mut stsz_payload: Option<&[u8]> = None;
    let mut stco_payload: Option<(bool, &[u8])> = None;

    for b in stbl_boxes {
        match &b.kind {
            b"stsd" => stsd_payload = Some(b.payload),
            b"stts" => stts_payload = Some(b.payload),
            b"stsc" => stsc_payload = Some(b.payload),
            b"stsz" => stsz_payload = Some(b.payload),
            b"stco" => stco_payload = Some((false, b.payload)),
            b"co64" => stco_payload = Some((true, b.payload)),
            b"sinf" | b"schi" | b"schm" => {
                return Err(ApiError::bad_request("Không hỗ trợ track MP4 mã hóa."));
            }
            _ => {}
        }
    }

    let stsd = stsd_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stsd."))?;
    let stts = stts_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stts."))?;
    let stsc = stsc_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stsc."))?;
    let stsz = stsz_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stsz."))?;
    let (is_co64, stco) =
        stco_payload.ok_or_else(|| ApiError::bad_request("Track MP4 thiếu stco/co64."))?;

    let sample_count = validate_sample_tables(stts, stsc, stsz, stco, is_co64, mdat_ranges, ctx)?;
    if sample_count == 0 {
        return Err(ApiError::bad_request("Track MP4 không có sample nào."));
    }

    if &handler == b"vide" {
        let (w, h, codec) = parse_video_stsd(stsd, ctx)?;
        if w != tkhd_w || h != tkhd_h {
            return Err(ApiError::bad_request(
                "Kích thước track MP4 không khớp với thông số video.",
            ));
        }
        Ok(TrackResult::Video {
            width: w,
            height: h,
            codec,
        })
    } else {
        let codec = parse_audio_stsd(stsd, ctx)?;
        Ok(TrackResult::Audio { codec })
    }
}

fn parse_tkhd(payload: &[u8]) -> ApiResult<(u32, u32)> {
    if payload.len() < 4 {
        return Err(ApiError::bad_request("Box tkhd bị cắt cụt."));
    }
    let (w_off, h_off) = match payload[0] {
        0 => (76usize, 80usize),
        1 => (88usize, 92usize),
        _ => return Err(ApiError::bad_request("Phiên bản tkhd không hợp lệ.")),
    };
    if payload.len() < h_off + 4 {
        return Err(ApiError::bad_request("Box tkhd bị cắt cụt."));
    }
    let w = u32::from_be_bytes([
        payload[w_off],
        payload[w_off + 1],
        payload[w_off + 2],
        payload[w_off + 3],
    ]) >> 16;
    let h = u32::from_be_bytes([
        payload[h_off],
        payload[h_off + 1],
        payload[h_off + 2],
        payload[h_off + 3],
    ]) >> 16;
    Ok((w, h))
}

fn validate_mdhd(payload: &[u8]) -> ApiResult<()> {
    if payload.len() < 4 {
        return Err(ApiError::bad_request("Box mdhd bị cắt cụt."));
    }
    let (ts, dur) = match payload[0] {
        0 => {
            if payload.len() < 20 {
                return Err(ApiError::bad_request("Box mdhd v0 bị cắt cụt."));
            }
            (
                u32::from_be_bytes([payload[12], payload[13], payload[14], payload[15]]) as u64,
                u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]) as u64,
            )
        }
        1 => {
            if payload.len() < 32 {
                return Err(ApiError::bad_request("Box mdhd v1 bị cắt cụt."));
            }
            (
                u32::from_be_bytes([payload[20], payload[21], payload[22], payload[23]]) as u64,
                u64::from_be_bytes([
                    payload[24], payload[25], payload[26], payload[27], payload[28], payload[29],
                    payload[30], payload[31],
                ]),
            )
        }
        _ => return Err(ApiError::bad_request("Phiên bản mdhd không hợp lệ.")),
    };
    if ts == 0 || dur == 0 {
        return Err(ApiError::bad_request("mdhd timescale hoặc duration bằng 0."));
    }
    Ok(())
}

fn validate_dinf(payload: &[u8], ctx: &mut InspectCtx) -> ApiResult<()> {
    let boxes = iter_boxes(payload, ctx, 5)?;
    for b in boxes {
        if &b.kind == b"dref" {
            if b.payload.len() < 8 {
                return Err(ApiError::bad_request("Box dref bị cắt cụt."));
            }
            let count = u32::from_be_bytes([b.payload[4], b.payload[5], b.payload[6], b.payload[7]])
                as usize;
            let entries_slice = &b.payload[8..];
            if count == 0 || count.saturating_mul(8) > entries_slice.len() {
                return Err(ApiError::bad_request("Số lượng dref không hợp lệ."));
            }
            let refs = iter_boxes(entries_slice, ctx, 6)?;
            if refs.len() != count {
                return Err(ApiError::bad_request("Số entry dref không khớp."));
            }
            for r in refs {
                if r.payload.len() < 4 || (r.payload[3] & 0x01) == 0 {
                    return Err(ApiError::bad_request(
                        "Không hỗ trợ tham chiếu dữ liệu ngoài (external data reference).",
                    ));
                }
            }
            return Ok(());
        }
    }
    Err(ApiError::bad_request("Box dinf thiếu dref."))
}

fn parse_video_stsd(stsd: &[u8], ctx: &mut InspectCtx) -> ApiResult<(u32, u32, String)> {
    if stsd.len() < 8 {
        return Err(ApiError::bad_request("Box stsd bị cắt cụt."));
    }
    let count = u32::from_be_bytes([stsd[4], stsd[5], stsd[6], stsd[7]]);
    if count != 1 {
        return Err(ApiError::bad_request("stsd video phải có đúng 1 entry."));
    }
    let entries = iter_boxes(&stsd[8..], ctx, 6)?;
    let entry = entries
        .first()
        .ok_or_else(|| ApiError::bad_request("stsd video thiếu entry."))?;
    if &entry.kind != b"avc1" {
        return Err(ApiError::unsupported_media_type(
            "Video MP4 chỉ hỗ trợ codec H.264/AVC (avc1).",
        ));
    }
    if entry.payload.len() < 78 {
        return Err(ApiError::bad_request("Entry avc1 bị cắt cụt."));
    }
    let stsd_w = u32::from(u16::from_be_bytes([entry.payload[24], entry.payload[25]]));
    let stsd_h = u32::from(u16::from_be_bytes([entry.payload[26], entry.payload[27]]));
    if stsd_w == 0 || stsd_h == 0 || stsd_w > MAX_VIDEO_WIDTH || stsd_h > MAX_VIDEO_HEIGHT {
        return Err(ApiError::bad_request(
            "Kích thước video vượt giới hạn 1..3840 x 1..2160.",
        ));
    }

    let children = iter_boxes(&entry.payload[78..], ctx, 7)?;
    let mut avcc_payload: Option<&[u8]> = None;
    for child in children {
        match &child.kind {
            b"avcC" => avcc_payload = Some(child.payload),
            b"sinf" | b"schi" | b"schm" => {
                return Err(ApiError::bad_request("Không hỗ trợ video mã hóa."));
            }
            _ => {}
        }
    }
    let avcc = avcc_payload.ok_or_else(|| ApiError::bad_request("Video avc1 thiếu avcC."))?;
    let (sps_w, sps_h, codec) = parse_avcc(avcc)?;
    if sps_w != stsd_w || sps_h != stsd_h {
        return Err(ApiError::bad_request(
            "Kích thước SPS không khớp với metadata avc1.",
        ));
    }
    Ok((sps_w, sps_h, codec))
}

fn parse_avcc(avcc: &[u8]) -> ApiResult<(u32, u32, String)> {
    if avcc.len() < 7 || avcc[0] != 1 {
        return Err(ApiError::bad_request("Box avcC không hợp lệ."));
    }
    let profile = avcc[1];
    let compat = avcc[2];
    let level = avcc[3];
    let num_sps = avcc[5] & 0x1f;
    if num_sps == 0 {
        return Err(ApiError::bad_request("avcC thiếu SPS."));
    }
    let mut idx = 6usize;
    let mut dims: Option<(u32, u32)> = None;
    for sps_i in 0..num_sps {
        if idx + 2 > avcc.len() {
            return Err(ApiError::bad_request("SPS trong avcC bị cắt cụt."));
        }
        let sps_len = u16::from_be_bytes([avcc[idx], avcc[idx + 1]]) as usize;
        idx += 2;
        if sps_len == 0 || idx + sps_len > avcc.len() {
            return Err(ApiError::bad_request("Độ dài SPS trong avcC không hợp lệ."));
        }
        let sps = &avcc[idx..idx + sps_len];
        if (sps[0] & 0x1f) != 7 {
            return Err(ApiError::bad_request("NALU đầu tiên trong avcC không phải SPS."));
        }
        if sps_i == 0 {
            dims = Some(parse_h264_sps_dimensions(sps)?);
        }
        idx += sps_len;
    }
    if idx >= avcc.len() {
        return Err(ApiError::bad_request("avcC thiếu PPS."));
    }
    let num_pps = avcc[idx];
    idx += 1;
    if num_pps == 0 {
        return Err(ApiError::bad_request("avcC thiếu PPS."));
    }
    for _ in 0..num_pps {
        if idx + 2 > avcc.len() {
            return Err(ApiError::bad_request("PPS trong avcC bị cắt cụt."));
        }
        let pps_len = u16::from_be_bytes([avcc[idx], avcc[idx + 1]]) as usize;
        idx += 2;
        if pps_len == 0 || idx + pps_len > avcc.len() {
            return Err(ApiError::bad_request("Độ dài PPS trong avcC không hợp lệ."));
        }
        if (avcc[idx] & 0x1f) != 8 {
            return Err(ApiError::bad_request("PPS trong avcC sai loại NALU."));
        }
        idx += pps_len;
    }
    let (w, h) = dims.ok_or_else(|| ApiError::bad_request("Không giải mã được SPS."))?;
    Ok((w, h, format!("avc1.{profile:02x}{compat:02x}{level:02x}")))
}

struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn remaining_bits(&self) -> usize {
        self.data.len().saturating_mul(8).saturating_sub(self.bit_pos)
    }

    fn read_bit(&mut self) -> ApiResult<u32> {
        let byte_idx = self.bit_pos / 8;
        if byte_idx >= self.data.len() {
            return Err(ApiError::bad_request("SPS H.264 bị cắt cụt."));
        }
        let shift = 7 - (self.bit_pos % 8);
        let bit = u32::from((self.data[byte_idx] >> shift) & 1);
        self.bit_pos += 1;
        Ok(bit)
    }

    fn read_bits(&mut self, n: usize) -> ApiResult<u32> {
        if n > 32 || self.remaining_bits() < n {
            return Err(ApiError::bad_request("SPS H.264 bị cắt cụt."));
        }
        let mut val = 0u32;
        for _ in 0..n {
            val = (val << 1) | self.read_bit()?;
        }
        Ok(val)
    }

    fn read_ue(&mut self) -> ApiResult<u32> {
        let mut zeros = 0usize;
        while zeros < 32 {
            if self.read_bit()? == 1 {
                break;
            }
            zeros += 1;
        }
        if zeros >= 32 {
            return Err(ApiError::bad_request("Exp-Golomb SPS vượt giới hạn."));
        }
        let suffix = if zeros == 0 { 0 } else { self.read_bits(zeros)? };
        Ok(((1u32 << zeros) - 1) + suffix)
    }

    fn read_se(&mut self) -> ApiResult<i32> {
        let code = self.read_ue()?;
        let val = ((code + 1) / 2) as i32;
        Ok(if code % 2 == 0 { -val } else { val })
    }
}

fn rbsp_from_nal(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut i = 1usize; // skip NAL header byte
    while i < nal.len() {
        if i + 2 < nal.len() && nal[i] == 0 && nal[i + 1] == 0 && nal[i + 2] == 3 {
            out.push(0);
            out.push(0);
            i += 3;
        } else {
            out.push(nal[i]);
            i += 1;
        }
    }
    out
}

fn parse_h264_sps_dimensions(sps_nal: &[u8]) -> ApiResult<(u32, u32)> {
    let rbsp = rbsp_from_nal(sps_nal);
    let mut br = BitReader::new(&rbsp);
    let profile_idc = br.read_bits(8)?;
    let _constraints = br.read_bits(8)?;
    let _level_idc = br.read_bits(8)?;
    let _sps_id = br.read_ue()?;

    let mut chroma_format_idc = 1u32;
    let mut separate_colour_plane_flag = 0u32;
    if matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    ) {
        chroma_format_idc = br.read_ue()?;
        if chroma_format_idc > 3 {
            return Err(ApiError::bad_request("chroma_format_idc SPS không hợp lệ."));
        }
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = br.read_bit()?;
        }
        let _bit_depth_luma = br.read_ue()?;
        let _bit_depth_chroma = br.read_ue()?;
        let _qpprime = br.read_bit()?;
        let seq_scaling_matrix_present_flag = br.read_bit()?;
        if seq_scaling_matrix_present_flag == 1 {
            let count = if chroma_format_idc != 3 { 8 } else { 12 };
            for i in 0..count {
                if br.read_bit()? == 1 {
                    let size = if i < 6 { 16 } else { 64 };
                    let mut last_scale = 8i32;
                    let mut next_scale = 8i32;
                    for _ in 0..size {
                        if next_scale != 0 {
                            let delta = br.read_se()?;
                            next_scale = (last_scale + delta + 256) % 256;
                        }
                        last_scale = if next_scale == 0 { last_scale } else { next_scale };
                    }
                }
            }
        }
    }

    let _log2_max_frame_num = br.read_ue()?;
    let pic_order_cnt_type = br.read_ue()?;
    if pic_order_cnt_type == 0 {
        let _log2_max_poc = br.read_ue()?;
    } else if pic_order_cnt_type == 1 {
        let _delta_always_zero = br.read_bit()?;
        let _offset_non_ref = br.read_se()?;
        let _offset_top_bottom = br.read_se()?;
        let cycle_len = br.read_ue()? as usize;
        if cycle_len > 255 || cycle_len > br.remaining_bits() {
            return Err(ApiError::bad_request("POC cycle SPS không hợp lệ."));
        }
        for _ in 0..cycle_len {
            let _ = br.read_se()?;
        }
    } else if pic_order_cnt_type > 2 {
        return Err(ApiError::bad_request("pic_order_cnt_type SPS không hợp lệ."));
    }

    let _max_num_ref_frames = br.read_ue()?;
    let _gaps_allowed = br.read_bit()?;
    let pic_width_in_mbs_minus1 = br.read_ue()?;
    let pic_height_in_map_units_minus1 = br.read_ue()?;
    let frame_mbs_only_flag = br.read_bit()?;
    if frame_mbs_only_flag == 0 {
        let _mb_adaptive = br.read_bit()?;
    }
    let _direct_8x8 = br.read_bit()?;
    let frame_cropping_flag = br.read_bit()?;
    let (crop_l, crop_r, crop_t, crop_b) = if frame_cropping_flag == 1 {
        (br.read_ue()?, br.read_ue()?, br.read_ue()?, br.read_ue()?)
    } else {
        (0, 0, 0, 0)
    };

    let raw_w = (pic_width_in_mbs_minus1 + 1)
        .checked_mul(16)
        .ok_or_else(|| ApiError::bad_request("Chiều rộng SPS bị tràn số."))?;
    let raw_h = (2 - frame_mbs_only_flag)
        .checked_mul(pic_height_in_map_units_minus1 + 1)
        .and_then(|v| v.checked_mul(16))
        .ok_or_else(|| ApiError::bad_request("Chiều cao SPS bị tràn số."))?;

    let chroma_array_type = if separate_colour_plane_flag == 0 {
        chroma_format_idc
    } else {
        0
    };
    let (sub_w, sub_h) = match chroma_format_idc {
        1 => (2u32, 2u32),
        2 => (2u32, 1u32),
        _ => (1u32, 1u32),
    };
    let crop_unit_x = if chroma_array_type == 0 { 1 } else { sub_w };
    let crop_unit_y = if chroma_array_type == 0 {
        2 - frame_mbs_only_flag
    } else {
        sub_h * (2 - frame_mbs_only_flag)
    };

    let crop_x = crop_l
        .saturating_add(crop_r)
        .saturating_mul(crop_unit_x);
    let crop_y = crop_t
        .saturating_add(crop_b)
        .saturating_mul(crop_unit_y);
    let width = raw_w
        .checked_sub(crop_x)
        .ok_or_else(|| ApiError::bad_request("Crop ngang SPS vượt chiều rộng."))?;
    let height = raw_h
        .checked_sub(crop_y)
        .ok_or_else(|| ApiError::bad_request("Crop dọc SPS vượt chiều cao."))?;
    if width == 0 || height == 0 || width > MAX_VIDEO_WIDTH || height > MAX_VIDEO_HEIGHT {
        return Err(ApiError::bad_request(
            "Kích thước video SPS vượt giới hạn 1..3840 x 1..2160.",
        ));
    }
    Ok((width, height))
}

fn parse_audio_stsd(stsd: &[u8], ctx: &mut InspectCtx) -> ApiResult<String> {
    if stsd.len() < 8 {
        return Err(ApiError::bad_request("Box stsd audio bị cắt cụt."));
    }
    let count = u32::from_be_bytes([stsd[4], stsd[5], stsd[6], stsd[7]]);
    if count != 1 {
        return Err(ApiError::bad_request("stsd audio phải có đúng 1 entry."));
    }
    let entries = iter_boxes(&stsd[8..], ctx, 6)?;
    let entry = entries
        .first()
        .ok_or_else(|| ApiError::bad_request("stsd audio thiếu entry."))?;
    if &entry.kind != b"mp4a" {
        return Err(ApiError::unsupported_media_type(
            "Audio MP4 chỉ hỗ trợ AAC-LC (mp4a).",
        ));
    }
    if entry.payload.len() < 28 {
        return Err(ApiError::bad_request("Entry mp4a bị cắt cụt."));
    }
    let children = iter_boxes(&entry.payload[28..], ctx, 7)?;
    for child in children {
        match &child.kind {
            b"esds" => {
                validate_esds_aac_lc(child.payload)?;
                return Ok("mp4a.40.2".to_string());
            }
            b"sinf" | b"schi" | b"schm" => {
                return Err(ApiError::bad_request("Không hỗ trợ audio mã hóa."));
            }
            _ => {}
        }
    }
    Err(ApiError::bad_request("Audio mp4a thiếu box esds."))
}

fn read_mp4_descr_len(data: &[u8], pos: &mut usize) -> ApiResult<usize> {
    let mut len = 0usize;
    for _ in 0..4 {
        if *pos >= data.len() {
            return Err(ApiError::bad_request("Descriptor esds bị cắt cụt."));
        }
        let b = data[*pos];
        *pos += 1;
        len = (len << 7) | usize::from(b & 0x7f);
        if (b & 0x80) == 0 {
            return Ok(len);
        }
    }
    Err(ApiError::bad_request("Độ dài descriptor esds không hợp lệ."))
}

fn validate_esds_aac_lc(esds: &[u8]) -> ApiResult<()> {
    if esds.len() < 8 {
        return Err(ApiError::bad_request("Box esds bị cắt cụt."));
    }
    let mut pos = 4usize; // skip FullBox version/flags
    if esds[pos] != 0x03 {
        return Err(ApiError::bad_request("esds thiếu ES_DescrTag."));
    }
    pos += 1;
    let es_len = read_mp4_descr_len(esds, &mut pos)?;
    if pos + es_len > esds.len() || es_len < 3 {
        return Err(ApiError::bad_request("esds ES_DescrTag sai độ dài."));
    }
    let es_end = pos + es_len;
    pos += 2; // ES_ID
    let flags = esds[pos];
    pos += 1;
    if (flags & 0x80) != 0 {
        pos += 2; // dependsOn_ES_ID
    }
    if (flags & 0x40) != 0 {
        if pos >= es_end {
            return Err(ApiError::bad_request("esds URL bị cắt cụt."));
        }
        let url_len = usize::from(esds[pos]);
        pos += 1 + url_len;
    }
    if (flags & 0x20) != 0 {
        pos += 2; // OCR_ES_Id
    }
    if pos >= es_end || esds[pos] != 0x04 {
        return Err(ApiError::bad_request("esds thiếu DecoderConfigDescrTag."));
    }
    pos += 1;
    let dc_len = read_mp4_descr_len(esds, &mut pos)?;
    if pos + dc_len > es_end || dc_len < 13 {
        return Err(ApiError::bad_request("esds DecoderConfig sai độ dài."));
    }
    let object_type_indication = esds[pos];
    if object_type_indication != 0x40 {
        return Err(ApiError::unsupported_media_type(
            "Audio MP4 chỉ hỗ trợ MPEG-4 AAC-LC.",
        ));
    }
    pos += 13;
    if pos >= es_end || esds[pos] != 0x05 {
        return Err(ApiError::bad_request("esds thiếu AudioSpecificConfig."));
    }
    pos += 1;
    let asc_len = read_mp4_descr_len(esds, &mut pos)?;
    if asc_len < 2 || pos + asc_len > es_end {
        return Err(ApiError::bad_request("AudioSpecificConfig bị cắt cụt."));
    }
    let audio_object_type = esds[pos] >> 3;
    if audio_object_type != 2 {
        return Err(ApiError::unsupported_media_type(
            "Audio MP4 chỉ hỗ trợ profile AAC-LC.",
        ));
    }
    Ok(())
}

fn validate_sample_tables(
    stts: &[u8],
    stsc: &[u8],
    stsz: &[u8],
    stco: &[u8],
    is_co64: bool,
    mdat_ranges: &[(u64, u64)],
    ctx: &mut InspectCtx,
) -> ApiResult<u64> {
    // 1. Validate stts count fits remaining payload BEFORE iteration
    if stts.len() < 8 {
        return Err(ApiError::bad_request("Box stts bị cắt cụt."));
    }
    let stts_count = u32::from_be_bytes([stts[4], stts[5], stts[6], stts[7]]) as u64;
    if stts_count.saturating_mul(8) > (stts.len() - 8) as u64 {
        return Err(ApiError::bad_request("Số entry stts vượt payload."));
    }

    // 2. Validate stsz sample_count <= remaining limits BEFORE iteration
    if stsz.len() < 12 {
        return Err(ApiError::bad_request("Box stsz bị cắt cụt."));
    }
    let uniform_size = u32::from_be_bytes([stsz[4], stsz[5], stsz[6], stsz[7]]);
    let sample_count = u32::from_be_bytes([stsz[8], stsz[9], stsz[10], stsz[11]]) as u64;
    if ctx.total_samples.saturating_add(sample_count) > MAX_SAMPLES_TOTAL {
        return Err(ApiError::bad_request(
            "Tổng số sample MP4 vượt giới hạn 100.000.",
        ));
    }
    if uniform_size == 0 && sample_count.saturating_mul(4) > (stsz.len() - 12) as u64 {
        return Err(ApiError::bad_request("Bảng kích thước sample stsz vượt payload."));
    }

    // 3. Validate stsc count fits remaining payload BEFORE iteration
    if stsc.len() < 8 {
        return Err(ApiError::bad_request("Box stsc bị cắt cụt."));
    }
    let stsc_count = u32::from_be_bytes([stsc[4], stsc[5], stsc[6], stsc[7]]) as u64;
    if stsc_count == 0 || stsc_count.saturating_mul(12) > (stsc.len() - 8) as u64 {
        return Err(ApiError::bad_request("Số entry stsc không hợp lệ."));
    }

    // 4. Validate stco/co64 count fits remaining payload BEFORE iteration
    if stco.len() < 8 {
        return Err(ApiError::bad_request("Box stco/co64 bị cắt cụt."));
    }
    let chunk_count = u32::from_be_bytes([stco[4], stco[5], stco[6], stco[7]]) as u64;
    let entry_width = if is_co64 { 8u64 } else { 4u64 };
    if chunk_count == 0 || chunk_count.saturating_mul(entry_width) > (stco.len() - 8) as u64 {
        return Err(ApiError::bad_request("Số chunk stco/co64 không hợp lệ."));
    }

    let mut stsc_entries = Vec::with_capacity(stsc_count as usize);
    let mut prev_first_chunk = 0u32;
    for chunk in stsc[8..].chunks_exact(12).take(stsc_count as usize) {
        let first_chunk = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let samples_per_chunk = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        if first_chunk <= prev_first_chunk || samples_per_chunk == 0 {
            return Err(ApiError::bad_request("Bảng stsc không hợp lệ."));
        }
        prev_first_chunk = first_chunk;
        stsc_entries.push((first_chunk, samples_per_chunk));
    }
    if stsc_entries[0].0 != 1 {
        return Err(ApiError::bad_request("stsc phải bắt đầu từ chunk 1."));
    }

    // Walk chunks & samples, verifying each sample lies entirely inside an mdat payload.
    let mut sample_idx = 0usize;
    let mut stsc_idx = 0usize;
    for chunk_idx in 0..(chunk_count as usize) {
        if chunk_idx % 256 == 0 {
            ctx.check_timeout()?;
        }
        let chunk_num = (chunk_idx + 1) as u32;
        while stsc_idx + 1 < stsc_entries.len() && stsc_entries[stsc_idx + 1].0 <= chunk_num {
            stsc_idx += 1;
        }
        let samples_in_chunk = stsc_entries[stsc_idx].1;
        let base_off = if is_co64 {
            let p = 8 + chunk_idx * 8;
            u64::from_be_bytes([
                stco[p],
                stco[p + 1],
                stco[p + 2],
                stco[p + 3],
                stco[p + 4],
                stco[p + 5],
                stco[p + 6],
                stco[p + 7],
            ])
        } else {
            let p = 8 + chunk_idx * 4;
            u64::from(u32::from_be_bytes([
                stco[p],
                stco[p + 1],
                stco[p + 2],
                stco[p + 3],
            ]))
        };

        let mut cur_off = base_off;
        for _ in 0..samples_in_chunk {
            if (sample_idx as u64) >= sample_count {
                return Err(ApiError::bad_request(
                    "Số sample theo stsc vượt quá stsz sample_count.",
                ));
            }
            let sz = if uniform_size != 0 {
                u64::from(uniform_size)
            } else {
                let p = 12 + sample_idx * 4;
                u64::from(u32::from_be_bytes([
                    stsz[p],
                    stsz[p + 1],
                    stsz[p + 2],
                    stsz[p + 3],
                ]))
            };
            if sz == 0 {
                return Err(ApiError::bad_request("Sample MP4 có kích thước 0."));
            }
            let end_off = cur_off
                .checked_add(sz)
                .ok_or_else(|| ApiError::bad_request("Sample offset MP4 bị tràn số."))?;
            let inside_mdat = mdat_ranges
                .iter()
                .any(|&(m_start, m_end)| cur_off >= m_start && end_off <= m_end);
            if !inside_mdat {
                return Err(ApiError::bad_request(
                    "Sample MP4 trỏ ra ngoài phạm vi box mdat.",
                ));
            }
            ctx.sample_ranges.push((cur_off, end_off));
            cur_off = end_off;
            sample_idx += 1;
        }
    }

    if (sample_idx as u64) != sample_count {
        return Err(ApiError::bad_request(
            "Tổng số sample stsc/stco không khớp stsz.",
        ));
    }
    ctx.total_samples += sample_count;
    Ok(sample_count)
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use sha2::{Digest, Sha256};

    fn box_wrap(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let len = (8 + payload.len()) as u32;
        let mut out = Vec::with_capacity(len as usize);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn full_box(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut inner = Vec::with_capacity(4 + payload.len());
        inner.push(version);
        inner.extend_from_slice(&flags.to_be_bytes()[1..4]);
        inner.extend_from_slice(payload);
        box_wrap(kind, &inner)
    }

    /// Minimal valid H.264 Baseline SPS for 320x180 (cropped from 320x192, 20x12 macroblocks).
    /// profile_idc=66 (Baseline), level_idc=30, pic_width_in_mbs_minus1=19,
    /// pic_height_in_map_units_minus1=11, frame_mbs_only_flag=1, frame_cropping_flag=1 (bottom=6 -> 12 px).
    fn sps_320x180() -> Vec<u8> {
        // Pre-encoded valid SPS NAL unit (NAL type 7):
        // 0x67 0x42 0xc0 0x1e 0xd9 0x01 0x41 0x7b 0x20
        // Let's bit-pack it deterministically so it is 100% exact!
        let mut bits: Vec<u8> = Vec::new();
        fn push_bits(bits: &mut Vec<u8>, val: u32, n: usize) {
            for i in (0..n).rev() {
                bits.push(((val >> i) & 1) as u8);
            }
        }
        fn push_ue(bits: &mut Vec<u8>, val: u32) {
            let code = val + 1;
            let len = 32 - code.leading_zeros() as usize;
            for _ in 0..(len - 1) {
                bits.push(0);
            }
            for i in (0..len).rev() {
                bits.push(((code >> i) & 1) as u8);
            }
        }
        push_bits(&mut bits, 0x67, 8); // NAL header (SPS)
        push_bits(&mut bits, 66, 8); // profile_idc = 66 (Baseline)
        push_bits(&mut bits, 0xc0, 8); // constraints
        push_bits(&mut bits, 30, 8); // level_idc
        push_ue(&mut bits, 0); // sps_id
        push_ue(&mut bits, 0); // log2_max_frame_num_minus4
        push_ue(&mut bits, 0); // pic_order_cnt_type
        push_ue(&mut bits, 0); // log2_max_pic_order_cnt_lsb_minus4
        push_ue(&mut bits, 1); // max_num_ref_frames
        push_bits(&mut bits, 0, 1); // gaps_in_frame_num_value_allowed_flag
        push_ue(&mut bits, 19); // pic_width_in_mbs_minus1 -> 20 * 16 = 320
        push_ue(&mut bits, 11); // pic_height_in_map_units_minus1 -> 12 * 16 = 192
        push_bits(&mut bits, 1, 1); // frame_mbs_only_flag = 1
        push_bits(&mut bits, 1, 1); // direct_8x8_inference_flag
        push_bits(&mut bits, 1, 1); // frame_cropping_flag = 1
        push_ue(&mut bits, 0); // crop_left
        push_ue(&mut bits, 0); // crop_right
        push_ue(&mut bits, 0); // crop_top
        push_ue(&mut bits, 6); // crop_bottom (6 * 2 = 12 => 192 - 12 = 180)
        push_bits(&mut bits, 0, 1); // vui_parameters_present_flag
        push_bits(&mut bits, 1, 1); // rbsp trailing stop bit
        while !bits.len().is_multiple_of(8) {
            bits.push(0);
        }
        bits.chunks_exact(8)
            .map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | b))
            .collect()
    }

    pub fn build_valid_mp4(with_audio: bool) -> (Vec<u8>, String) {
        let mut ftyp_payload = Vec::new();
        ftyp_payload.extend_from_slice(b"isom");
        ftyp_payload.extend_from_slice(&0x00000200u32.to_be_bytes());
        ftyp_payload.extend_from_slice(b"isommp42");
        let ftyp = box_wrap(b"ftyp", &ftyp_payload);

        let video_sample = vec![0x00, 0x00, 0x00, 0x04, 0x65, 0x88, 0x84, 0x21];
        let audio_sample = vec![0x21, 0x10, 0x04, 0x60];

        let mut mdat_payload = Vec::new();
        mdat_payload.extend_from_slice(&video_sample);
        if with_audio {
            mdat_payload.extend_from_slice(&audio_sample);
        }
        let mdat = box_wrap(b"mdat", &mdat_payload);

        // We'll place ftyp, moov, mdat in order; compute mdat payload start after moov is built!
        let build_moov = |mdat_payload_offset: u32| -> Vec<u8> {
            // mvhd: timescale=1000, duration=1500 (1.5s)
            let mut mvhd_p = vec![0u8; 96];
            mvhd_p[8..12].copy_from_slice(&1000u32.to_be_bytes());
            mvhd_p[12..16].copy_from_slice(&1500u32.to_be_bytes());
            mvhd_p[92..96].copy_from_slice(&2u32.to_be_bytes());
            let mvhd = full_box(b"mvhd", 0, 0, &mvhd_p);

            // Video trak (320x180)
            let mut tkhd_v = vec![0u8; 84];
            tkhd_v[8..12].copy_from_slice(&1u32.to_be_bytes()); // track_id
            tkhd_v[16..20].copy_from_slice(&1500u32.to_be_bytes());
            tkhd_v[72..76].copy_from_slice(&(320u32 << 16).to_be_bytes());
            tkhd_v[76..80].copy_from_slice(&(180u32 << 16).to_be_bytes());
            let tkhd_v = full_box(b"tkhd", 0, 3, &tkhd_v);

            let mut mdhd_v = vec![0u8; 20];
            mdhd_v[8..12].copy_from_slice(&1000u32.to_be_bytes());
            mdhd_v[12..16].copy_from_slice(&1500u32.to_be_bytes());
            let mdhd_v = full_box(b"mdhd", 0, 0, &mdhd_v);

            let mut hdlr_v = vec![0u8; 20];
            hdlr_v[4..8].copy_from_slice(b"vide");
            let hdlr_v = full_box(b"hdlr", 0, 0, &hdlr_v);

            let url_self = full_box(b"url ", 0, 1, &[]);
            let mut dref_p = Vec::new();
            dref_p.extend_from_slice(&1u32.to_be_bytes());
            dref_p.extend_from_slice(&url_self);
            let dinf = box_wrap(b"dinf", &full_box(b"dref", 0, 0, &dref_p));

            // avcC with 320x180 SPS and dummy PPS
            let sps = sps_320x180();
            let pps = vec![0x68u8, 0xce, 0x3c, 0x80];
            let mut avcc_p = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
            avcc_p.extend_from_slice(&(sps.len() as u16).to_be_bytes());
            avcc_p.extend_from_slice(&sps);
            avcc_p.push(1);
            avcc_p.extend_from_slice(&(pps.len() as u16).to_be_bytes());
            avcc_p.extend_from_slice(&pps);
            let avcc = box_wrap(b"avcC", &avcc_p);

            let mut avc1_p = vec![0u8; 78];
            avc1_p[6..8].copy_from_slice(&1u16.to_be_bytes()); // data_ref_index
            avc1_p[24..26].copy_from_slice(&320u16.to_be_bytes());
            avc1_p[26..28].copy_from_slice(&180u16.to_be_bytes());
            avc1_p.extend_from_slice(&avcc);
            let avc1 = box_wrap(b"avc1", &avc1_p);

            let mut stsd_v_p = Vec::new();
            stsd_v_p.extend_from_slice(&1u32.to_be_bytes());
            stsd_v_p.extend_from_slice(&avc1);
            let stsd_v = full_box(b"stsd", 0, 0, &stsd_v_p);

            let mut stts_v_p = Vec::new();
            stts_v_p.extend_from_slice(&1u32.to_be_bytes());
            stts_v_p.extend_from_slice(&1u32.to_be_bytes());
            stts_v_p.extend_from_slice(&1500u32.to_be_bytes());
            let stts_v = full_box(b"stts", 0, 0, &stts_v_p);

            let mut stsc_p = Vec::new();
            stsc_p.extend_from_slice(&1u32.to_be_bytes());
            stsc_p.extend_from_slice(&1u32.to_be_bytes());
            stsc_p.extend_from_slice(&1u32.to_be_bytes());
            stsc_p.extend_from_slice(&1u32.to_be_bytes());
            let stsc_v = full_box(b"stsc", 0, 0, &stsc_p);

            let mut stsz_v_p = Vec::new();
            stsz_v_p.extend_from_slice(&(video_sample.len() as u32).to_be_bytes());
            stsz_v_p.extend_from_slice(&1u32.to_be_bytes());
            let stsz_v = full_box(b"stsz", 0, 0, &stsz_v_p);

            let mut stco_v_p = Vec::new();
            stco_v_p.extend_from_slice(&1u32.to_be_bytes());
            stco_v_p.extend_from_slice(&mdat_payload_offset.to_be_bytes());
            let stco_v = full_box(b"stco", 0, 0, &stco_v_p);

            let mut stbl_v_p = Vec::new();
            stbl_v_p.extend_from_slice(&stsd_v);
            stbl_v_p.extend_from_slice(&stts_v);
            stbl_v_p.extend_from_slice(&stsc_v);
            stbl_v_p.extend_from_slice(&stsz_v);
            stbl_v_p.extend_from_slice(&stco_v);
            let stbl_v = box_wrap(b"stbl", &stbl_v_p);

            let mut minf_v_p = Vec::new();
            minf_v_p.extend_from_slice(&dinf);
            minf_v_p.extend_from_slice(&stbl_v);
            let minf_v = box_wrap(b"minf", &minf_v_p);

            let mut mdia_v_p = Vec::new();
            mdia_v_p.extend_from_slice(&mdhd_v);
            mdia_v_p.extend_from_slice(&hdlr_v);
            mdia_v_p.extend_from_slice(&minf_v);
            let mdia_v = box_wrap(b"mdia", &mdia_v_p);

            let mut trak_v_p = Vec::new();
            trak_v_p.extend_from_slice(&tkhd_v);
            trak_v_p.extend_from_slice(&mdia_v);
            let trak_v = box_wrap(b"trak", &trak_v_p);

            let mut moov_p = Vec::new();
            moov_p.extend_from_slice(&mvhd);
            moov_p.extend_from_slice(&trak_v);

            if with_audio {
                let mut tkhd_a = vec![0u8; 84];
                tkhd_a[8..12].copy_from_slice(&2u32.to_be_bytes());
                tkhd_a[16..20].copy_from_slice(&1500u32.to_be_bytes());
                let tkhd_a = full_box(b"tkhd", 0, 3, &tkhd_a);

                let mut hdlr_a = vec![0u8; 20];
                hdlr_a[4..8].copy_from_slice(b"soun");
                let hdlr_a = full_box(b"hdlr", 0, 0, &hdlr_a);

                // esds for AAC-LC (objectTypeIndication=0x40, audioObjectType=2 => 0x12 0x10)
                let esds_p = vec![
                    0x03, 22, 0x00, 0x02, 0x00, // ES_DescrTag
                    0x04, 17, 0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // DecoderConfig
                    0x05, 2, 0x12, 0x10, // AudioSpecificConfig (AAC-LC)
                ];
                let esds = full_box(b"esds", 0, 0, &esds_p);
                let mut mp4a_p = vec![0u8; 28];
                mp4a_p[6..8].copy_from_slice(&1u16.to_be_bytes());
                mp4a_p.extend_from_slice(&esds);
                let mp4a = box_wrap(b"mp4a", &mp4a_p);

                let mut stsd_a_p = Vec::new();
                stsd_a_p.extend_from_slice(&1u32.to_be_bytes());
                stsd_a_p.extend_from_slice(&mp4a);
                let stsd_a = full_box(b"stsd", 0, 0, &stsd_a_p);

                let mut stsz_a_p = Vec::new();
                stsz_a_p.extend_from_slice(&(audio_sample.len() as u32).to_be_bytes());
                stsz_a_p.extend_from_slice(&1u32.to_be_bytes());
                let stsz_a = full_box(b"stsz", 0, 0, &stsz_a_p);

                let audio_off = mdat_payload_offset + video_sample.len() as u32;
                let mut stco_a_p = Vec::new();
                stco_a_p.extend_from_slice(&1u32.to_be_bytes());
                stco_a_p.extend_from_slice(&audio_off.to_be_bytes());
                let stco_a = full_box(b"stco", 0, 0, &stco_a_p);

                let mut stbl_a_p = Vec::new();
                stbl_a_p.extend_from_slice(&stsd_a);
                stbl_a_p.extend_from_slice(&stts_v);
                stbl_a_p.extend_from_slice(&stsc_v);
                stbl_a_p.extend_from_slice(&stsz_a);
                stbl_a_p.extend_from_slice(&stco_a);
                let stbl_a = box_wrap(b"stbl", &stbl_a_p);

                let mut minf_a_p = Vec::new();
                minf_a_p.extend_from_slice(&dinf);
                minf_a_p.extend_from_slice(&stbl_a);
                let minf_a = box_wrap(b"minf", &minf_a_p);

                let mut mdia_a_p = Vec::new();
                mdia_a_p.extend_from_slice(&mdhd_v);
                mdia_a_p.extend_from_slice(&hdlr_a);
                mdia_a_p.extend_from_slice(&minf_a);
                let mdia_a = box_wrap(b"mdia", &mdia_a_p);

                let mut trak_a_p = Vec::new();
                trak_a_p.extend_from_slice(&tkhd_a);
                trak_a_p.extend_from_slice(&mdia_a);
                moov_p.extend_from_slice(&box_wrap(b"trak", &trak_a_p));
            }

            box_wrap(b"moov", &moov_p)
        };

        let dummy_moov = build_moov(0);
        let mdat_payload_offset = (ftyp.len() + dummy_moov.len() + 8) as u32;
        let moov = build_moov(mdat_payload_offset);
        assert_eq!(moov.len(), dummy_moov.len());

        let mut bytes = Vec::with_capacity(ftyp.len() + moov.len() + mdat.len());
        bytes.extend_from_slice(&ftyp);
        bytes.extend_from_slice(&moov);
        bytes.extend_from_slice(&mdat);
        let sha = format!("{:x}", Sha256::digest(&bytes));
        (bytes, sha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("opendash-{name}-{}.mp4", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn accepts_valid_avc_and_aac_mp4() {
        let (bytes, sha) = test_fixtures::build_valid_mp4(true);
        let p = write_temp("valid-avc-aac", &bytes);
        let meta = inspect_mp4(&p, bytes.len() as u64, sha.clone()).expect("valid mp4");
        let _ = std::fs::remove_file(&p);
        assert_eq!(meta.id, sha);
        assert_eq!((meta.width, meta.height), (320, 180));
        assert_eq!(meta.duration_ms, 1500);
        assert!(meta.video_codec.starts_with("avc1."));
        assert_eq!(meta.audio_codec.as_deref(), Some("mp4a.40.2"));
    }

    #[test]
    fn rejects_lying_dimensions_and_fragmented_moof() {
        let (mut bytes, sha) = test_fixtures::build_valid_mp4(false);
        // Corrupt tkhd width from 320 to 640 while keeping SPS 320x180
        let pos = bytes.windows(4).position(|w| w == b"tkhd").unwrap();
        let w_off = pos + 4 + 4 + 72; // kind(4) + ver/flags(4) + 72
        bytes[w_off..w_off + 4].copy_from_slice(&(640u32 << 16).to_be_bytes());
        let p1 = write_temp("lying-dims", &bytes);
        let res1 = inspect_mp4(&p1, bytes.len() as u64, sha.clone());
        let _ = std::fs::remove_file(&p1);
        assert!(res1.is_err());

        // Append a moof box -> rejected as fragmented MP4
        let (mut frag, sha2) = test_fixtures::build_valid_mp4(false);
        frag.extend_from_slice(&8u32.to_be_bytes());
        frag.extend_from_slice(b"moof");
        let p2 = write_temp("fragmented", &frag);
        let res2 = inspect_mp4(&p2, frag.len() as u64, sha2);
        let _ = std::fs::remove_file(&p2);
        assert!(res2.is_err());
    }
}
