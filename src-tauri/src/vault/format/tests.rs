use super::*;

/// Small KDF params so tests don't burn 64 MiB × several cases.
fn test_kdf() -> KdfParams {
    KdfParams {
        m_cost_kib: 8 * 1024,
        t_cost: 1,
        p_cost: 1,
    }
}

#[test]
fn roundtrip() {
    let dek: [u8; 32] = random_bytes();
    let header = wrap_new("correct horse", &dek, test_kdf()).unwrap();
    let payload = br#"{"version":1}"#;
    let file = seal(&header, &dek, payload).unwrap();

    let (parsed, nonce, ct) = parse(&file).unwrap();
    let dek2 = unwrap_dek(&parsed, "correct horse").unwrap();
    let pt = open_payload(&parsed, &dek2, &nonce, &ct).unwrap();
    assert_eq!(pt.as_slice(), payload);
}

#[test]
fn wrong_password_rejected() {
    let dek: [u8; 32] = random_bytes();
    let header = wrap_new("right", &dek, test_kdf()).unwrap();
    let err = unwrap_dek(&header, "wrong").unwrap_err();
    assert!(matches!(err, AppError::InvalidPassword));
}

#[test]
fn password_change_rewraps_dek_only() {
    let dek: [u8; 32] = random_bytes();
    let header = wrap_new("old", &dek, test_kdf()).unwrap();
    let payload = b"payload data";
    let file = seal(&header, &dek, payload).unwrap();
    let (parsed, nonce, ct) = parse(&file).unwrap();

    // Re-wrap the same DEK under a new password; payload bytes untouched.
    let new_header = wrap_new("new", &dek, test_kdf()).unwrap();
    let dek2 = unwrap_dek(&new_header, "new").unwrap();
    assert_eq!(dek2.as_slice(), dek.as_slice());

    // Old payload still opens with the recovered DEK and the old header.
    let pt = open_payload(&parsed, &dek2, &nonce, &ct).unwrap();
    assert_eq!(pt.as_slice(), payload);
}

#[test]
fn corrupted_file_rejected() {
    let dek: [u8; 32] = random_bytes();
    let header = wrap_new("pw", &dek, test_kdf()).unwrap();
    let mut file = seal(&header, &dek, b"data").unwrap();

    // Flip a payload byte → authentication failure.
    let last = file.len() - 1;
    file[last] ^= 0xff;
    let (parsed, nonce, ct) = parse(&file).unwrap();
    let dek2 = unwrap_dek(&parsed, "pw").unwrap();
    assert!(matches!(
        open_payload(&parsed, &dek2, &nonce, &ct),
        Err(AppError::Corrupted(_))
    ));

    // Tampered header (KDF params) → DEK unwrap fails (AAD mismatch → derives differently anyway).
    let mut file2 = seal(&header, &dek, b"data").unwrap();
    file2[5] ^= 0x01;
    let (parsed2, _, _) = parse(&file2).unwrap();
    assert!(unwrap_dek(&parsed2, "pw").is_err());

    // Truncated / not a vault.
    assert!(parse(b"SRVS").is_err());
    assert!(parse(
        b"not a vault file at all............................................................"
    )
    .is_err());
}
