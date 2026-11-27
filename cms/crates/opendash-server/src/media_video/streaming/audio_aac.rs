use super::*;

pub(crate) fn parse_audio_stsd(stsd: &[u8], ctx: &mut InspectCtx) -> ApiResult<String> {
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

pub(crate) fn read_mp4_descr_len(data: &[u8], pos: &mut usize) -> ApiResult<usize> {
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
    Err(ApiError::bad_request(
        "Độ dài descriptor esds không hợp lệ.",
    ))
}

pub(crate) fn validate_esds_aac_lc(esds: &[u8]) -> ApiResult<()> {
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

pub(crate) fn validate_sample_tables(
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
        return Err(ApiError::bad_request(
            "Bảng kích thước sample stsz vượt payload.",
        ));
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
