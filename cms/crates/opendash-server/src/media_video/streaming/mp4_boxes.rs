use super::*;

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
            lifecycle_state: "active".to_string(),
            generation: 1,
            version: 1,
            created_at: String::new(),
        }
    }
}

impl InspectCtx {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            box_count: 0,
            total_samples: 0,
            sample_ranges: Vec::new(),
        }
    }

    pub(crate) fn check_timeout(&self) -> ApiResult<()> {
        if self.started.elapsed() > INSPECT_TIMEOUT {
            return Err(ApiError::bad_request(
                "Video MP4 vượt quá thời gian kiểm tra metadata.",
            ));
        }
        Ok(())
    }

    pub(crate) fn count_box(&mut self) -> ApiResult<()> {
        self.box_count += 1;
        if self.box_count > MAX_BOXES {
            return Err(ApiError::bad_request(
                "Video MP4 vượt quá số lượng box cho phép.",
            ));
        }
        if self.box_count.is_multiple_of(128) {
            self.check_timeout()?;
        }
        Ok(())
    }
}

pub(crate) fn read_top_box_header(
    file: &mut File,
    pos: u64,
    file_size: u64,
) -> ApiResult<BoxHeader> {
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
        return Err(ApiError::bad_request(
            "Kích thước box MP4 vượt ngoài phạm vi file.",
        ));
    }
    Ok(BoxHeader {
        kind,
        payload_start: pos + header_size,
        box_end,
    })
}

pub(crate) fn iter_boxes<'a>(
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
                    return Err(ApiError::bad_request(
                        "Box ftyp MP4 có kích thước không hợp lệ.",
                    ));
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
                    return Err(ApiError::bad_request(
                        "MP4 chỉ được phép có đúng một box moov.",
                    ));
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

pub(crate) fn validate_ftyp(payload: &[u8]) -> ApiResult<()> {
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

pub(crate) fn parse_moov(
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

    let duration_ms = duration_ms.ok_or_else(|| ApiError::bad_request("MP4 thiếu box mvhd."))?;

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
            return Err(ApiError::bad_request("MP4 có sample byte range chồng lấn."));
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

pub(crate) fn parse_mvhd(payload: &[u8]) -> ApiResult<u64> {
    if payload.len() < 4 {
        return Err(ApiError::bad_request("Box mvhd bị cắt cụt."));
    }
    let version = payload[0];
    let (timescale, duration) = match version {
        0 => {
            if payload.len() < 24 {
                return Err(ApiError::bad_request("Box mvhd v0 bị cắt cụt."));
            }
            let ts =
                u32::from_be_bytes([payload[12], payload[13], payload[14], payload[15]]) as u64;
            let dur =
                u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]) as u64;
            (ts, dur)
        }
        1 => {
            if payload.len() < 36 {
                return Err(ApiError::bad_request("Box mvhd v1 bị cắt cụt."));
            }
            let ts =
                u32::from_be_bytes([payload[20], payload[21], payload[22], payload[23]]) as u64;
            let dur = u64::from_be_bytes([
                payload[24],
                payload[25],
                payload[26],
                payload[27],
                payload[28],
                payload[29],
                payload[30],
                payload[31],
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
    let ms_u64 =
        u64::try_from(ms).map_err(|_| ApiError::bad_request("Duration MP4 vượt giới hạn."))?;
    if ms_u64 == 0 || ms_u64 > MAX_DURATION_MS {
        return Err(ApiError::bad_request(
            "Thời lượng video vượt quá giới hạn 1..600000 ms.",
        ));
    }
    Ok(ms_u64)
}

pub(crate) fn parse_trak(
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

pub(crate) fn parse_tkhd(payload: &[u8]) -> ApiResult<(u32, u32)> {
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
