use std::io::Cursor;

use super::*;

fn fingerprint(bytes: &[u8]) -> String {
    quick_fingerprint(Cursor::new(bytes), bytes.len() as u64).unwrap()
}

#[test]
fn the_fingerprint_covers_size_head_and_tail_but_not_the_middle() {
    let span = QUICK_SPAN as usize;
    let mut big = vec![0u8; span * 3];
    let base = fingerprint(&big);

    big[span + span / 2] = 1;
    assert_eq!(fingerprint(&big), base);
    big[0] = 1;
    assert_ne!(fingerprint(&big), base);
    big[0] = 0;
    *big.last_mut().unwrap() = 1;
    assert_ne!(fingerprint(&big), base);
    assert_ne!(fingerprint(&big[..span * 2]), fingerprint(&big[..span * 2 + 1]));
}

#[test]
fn small_files_hash_all_their_bytes_and_fingerprints_are_tagged() {
    assert_ne!(fingerprint(b"abc"), fingerprint(b"abd"));
    assert_eq!(fingerprint(b"abc"), fingerprint(b"abc"));
    assert!(fingerprint(b"").starts_with("b3:"));
    let between = vec![7u8; QUICK_SPAN as usize + 10];
    assert_eq!(fingerprint(&between), fingerprint(&between.clone()));
}

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(Cursor::new(b"123456789")).unwrap(), 0xcbf4_3926);
    assert_eq!(crc32(Cursor::new(Vec::new())).unwrap(), 0);
}
