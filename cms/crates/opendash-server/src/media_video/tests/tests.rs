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
