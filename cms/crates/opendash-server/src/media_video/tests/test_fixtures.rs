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
