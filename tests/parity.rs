use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use base64_simd_portable::{Error, decode, encode, encoded_len};
fn boundaries<const URL: bool, const PAD: bool>(reference: &impl Engine) {
    let mut data = [0u8; 2080];
    let mut state = 0x12345678u32;
    for x in &mut data {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        *x = state as u8;
    }
    let mut output = [0u8; 2800];
    let mut recovered = [0u8; 2081];
    for offset in 0..32 {
        for len in 0..=2048 {
            let input = &data[offset..offset + len];
            let expected = reference.encode(input);
            let n = encoded_len(len, PAD).unwrap();
            assert_eq!(expected.len(), n);
            output.fill(0xa5);
            recovered.fill(0xa5);
            assert_eq!(encode::<URL, PAD>(input, &mut output[offset..]).unwrap(), n);
            assert_eq!(&output[offset..offset + n], expected.as_bytes());
            assert_eq!(output[offset + n], 0xa5);
            assert_eq!(
                decode::<URL, PAD>(&output[offset..offset + n], &mut recovered[offset..]).unwrap(),
                len
            );
            assert_eq!(&recovered[offset..offset + len], input);
            assert_eq!(recovered[offset + len], 0xa5);
        }
    }
}
#[test]
fn all_boundaries() {
    boundaries::<false, true>(&STANDARD);
    boundaries::<false, false>(&STANDARD_NO_PAD);
    boundaries::<true, true>(&URL_SAFE);
    boundaries::<true, false>(&URL_SAFE_NO_PAD);
}
#[test]
fn all_triplets() {
    let mut all = Vec::with_capacity(3 * (1 << 24));
    for x in 0u32..(1 << 24) {
        all.extend_from_slice(&x.to_le_bytes()[..3]);
    }
    let mut encoded = vec![0; all.len() / 3 * 4];
    let mut decoded = vec![0; all.len()];
    for url in [false, true] {
        let (n, expected) = if url {
            (
                encode::<true, true>(&all, &mut encoded).unwrap(),
                URL_SAFE.encode(&all),
            )
        } else {
            (
                encode::<false, true>(&all, &mut encoded).unwrap(),
                STANDARD.encode(&all),
            )
        };
        assert_eq!(n, encoded.len());
        assert!(encoded == expected.as_bytes());
        let n = if url {
            decode::<true, true>(&encoded, &mut decoded)
        } else {
            decode::<false, true>(&encoded, &mut decoded)
        }
        .unwrap();
        assert_eq!(n, all.len());
        assert!(decoded == all);
    }
}
fn invalid<const URL: bool, const PAD: bool>(reference: &impl Engine) {
    let mut s = [b'A'; 68];
    let mut out = [0; 64];
    for i in 0..s.len() {
        for b in 0..=255u8 {
            s[i] = b;
            let a = reference.decode(s);
            let b = decode::<URL, PAD>(&s, &mut out);
            assert_eq!(
                a.is_ok(),
                b.is_ok(),
                "byte={b:?}, index={i}, URL={URL}, PAD={PAD}"
            );
            if let (Ok(a), Ok(n)) = (a, b) {
                assert_eq!(a, &out[..n]);
            }
        }
        s[i] = b'A';
    }
    for s in [
        "", "A", "AA", "AAA", "AAAA", "A=", "AA=", "AAA=", "AA==", "A===", "====", "AA=A", "AAAA=",
        "AAAA==", "AB==", "AAB=", "AB", "AAB",
    ] {
        let a = reference.decode(s);
        let b = decode::<URL, PAD>(s.as_bytes(), &mut out);
        assert_eq!(a.is_ok(), b.is_ok(), "{s} URL={URL} PAD={PAD}");
        if let (Ok(a), Ok(n)) = (a, b) {
            assert_eq!(a, &out[..n]);
        }
    }
}
#[test]
fn invalid_inputs() {
    invalid::<false, true>(&STANDARD);
    invalid::<false, false>(&STANDARD_NO_PAD);
    invalid::<true, true>(&URL_SAFE);
    invalid::<true, false>(&URL_SAFE_NO_PAD);
    assert_eq!(encode::<false, true>(b"a", &mut [0; 3]), Err(Error::Output));
    assert_eq!(decode::<false, true>(b"YQ==", &mut []), Err(Error::Output));
    assert_eq!(encoded_len(usize::MAX, true), None);
}
