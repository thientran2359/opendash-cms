use super::*;

pub(crate) fn validate_mdhd(payload: &[u8]) -> ApiResult<()> {
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
                    payload[24],
                    payload[25],
                    payload[26],
                    payload[27],
                    payload[28],
                    payload[29],
                    payload[30],
                    payload[31],
                ]),
            )
        }
        _ => return Err(ApiError::bad_request("Phiên bản mdhd không hợp lệ.")),
    };
    if ts == 0 || dur == 0 {
        return Err(ApiError::bad_request(
            "mdhd timescale hoặc duration bằng 0.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_dinf(payload: &[u8], ctx: &mut InspectCtx) -> ApiResult<()> {
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

pub(crate) fn parse_video_stsd(stsd: &[u8], ctx: &mut InspectCtx) -> ApiResult<(u32, u32, String)> {
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

pub(crate) fn parse_avcc(avcc: &[u8]) -> ApiResult<(u32, u32, String)> {
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
            return Err(ApiError::bad_request(
                "NALU đầu tiên trong avcC không phải SPS.",
            ));
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

impl<'a> BitReader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    pub(crate) fn remaining_bits(&self) -> usize {
        self.data
            .len()
            .saturating_mul(8)
            .saturating_sub(self.bit_pos)
    }

    pub(crate) fn read_bit(&mut self) -> ApiResult<u32> {
        let byte_idx = self.bit_pos / 8;
        if byte_idx >= self.data.len() {
            return Err(ApiError::bad_request("SPS H.264 bị cắt cụt."));
        }
        let shift = 7 - (self.bit_pos % 8);
        let bit = u32::from((self.data[byte_idx] >> shift) & 1);
        self.bit_pos += 1;
        Ok(bit)
    }

    pub(crate) fn read_bits(&mut self, n: usize) -> ApiResult<u32> {
        if n > 32 || self.remaining_bits() < n {
            return Err(ApiError::bad_request("SPS H.264 bị cắt cụt."));
        }
        let mut val = 0u32;
        for _ in 0..n {
            val = (val << 1) | self.read_bit()?;
        }
        Ok(val)
    }

    pub(crate) fn read_ue(&mut self) -> ApiResult<u32> {
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
        let suffix = if zeros == 0 {
            0
        } else {
            self.read_bits(zeros)?
        };
        Ok(((1u32 << zeros) - 1) + suffix)
    }

    pub(crate) fn read_se(&mut self) -> ApiResult<i32> {
        let code = self.read_ue()?;
        let val = ((code + 1) / 2) as i32;
        Ok(if code % 2 == 0 { -val } else { val })
    }
}

pub(crate) fn rbsp_from_nal(nal: &[u8]) -> Vec<u8> {
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

pub(crate) fn parse_h264_sps_dimensions(sps_nal: &[u8]) -> ApiResult<(u32, u32)> {
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
                        last_scale = if next_scale == 0 {
                            last_scale
                        } else {
                            next_scale
                        };
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
        return Err(ApiError::bad_request(
            "pic_order_cnt_type SPS không hợp lệ.",
        ));
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

    let crop_x = crop_l.saturating_add(crop_r).saturating_mul(crop_unit_x);
    let crop_y = crop_t.saturating_add(crop_b).saturating_mul(crop_unit_y);
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
