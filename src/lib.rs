#![no_std]
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Length,
    Output,
    Character,
    Padding,
    TrailingBits,
}

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub const fn encoded_len(input: usize, padded: bool) -> Option<usize> {
    let Some(full) = (input / 3).checked_mul(4) else {
        return None;
    };
    let rem = input % 3;
    full.checked_add(if rem == 0 {
        0
    } else if padded {
        4
    } else {
        rem + 1
    })
}

#[inline(always)]
fn alphabet<const URL: bool>() -> &'static [u8; 64] {
    if URL { URL_SAFE } else { STANDARD }
}

// The values entering this function are in 0..64; signed comparisons let LLVM
// use native byte comparisons on architectures without unsigned byte compares.
#[inline(always)]
fn ascii<const URL: bool>(x: u8) -> u8 {
    let base = if x as i8 > 51 {
        252
    } else if x as i8 > 25 {
        71
    } else {
        65
    };
    let special = if URL {
        if x & 1 == 0 { 239 } else { 32 }
    } else {
        (x & 1).wrapping_mul(3).wrapping_add(237)
    };
    x.wrapping_add(if x as i8 > 61 { special } else { base })
}

// Fixed shuffle gathers [b,a,c,b] for each input triplet. LLVM recognizes
// the byte rearrangement as a vector shuffle instead of individual gathers.
#[inline(always)]
fn shuffle(a: &[u8; 32]) -> [u8; 32] {
    [
        a[1], a[0], a[2], a[1], a[4], a[3], a[5], a[4], a[7], a[6], a[8], a[7], a[10], a[9], a[11],
        a[10], a[13], a[12], a[14], a[13], a[16], a[15], a[17], a[16], a[19], a[18], a[20], a[19],
        a[22], a[21], a[23], a[22],
    ]
}

// Non-power-of-two factors deliberately preserve the high/low product while
// preventing LLVM from widening 16-bit lanes into variable 32-bit shifts.
// 65*masked and 1025*masked have the same high halves as 64 and 1024;
// adding 0x1000 to 16 has no effect on the low half for multiples of 16.
#[inline(always)]
fn expand(a: &[u8; 32]) -> [u8; 32] {
    let s = shuffle(a);
    let w: [u16; 16] = core::array::from_fn(|i| u16::from_le_bytes([s[2 * i], s[2 * i + 1]]));
    const H: [u16; 16] = [
        0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00,
        0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0,
    ];
    const M: [u16; 16] = [
        65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025,
    ];
    const L: [u16; 16] = [
        0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0,
        0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f,
    ];
    const N: [u16; 16] = [
        4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256,
    ];
    let v: [u16; 16] = core::array::from_fn(|i| {
        ((((w[i] & H[i]) as u32 * M[i] as u32) >> 16) as u16) | ((w[i] & L[i]).wrapping_mul(N[i]))
    });
    core::array::from_fn(|i| v[i / 2].to_le_bytes()[i % 2])
}

#[inline(never)]
fn encode_bulk<const URL: bool>(input: &[u8], output: &mut [u8]) {
    assert_eq!(input.len() % 3, 0);
    assert_eq!(output.len(), input.len() / 3 * 4);
    let mut src = input;
    let mut dst = output;
    while src.len() >= 32 {
        let a = src.first_chunk::<32>().unwrap();
        let v = expand(a);
        let b = dst.first_chunk_mut::<32>().unwrap();
        for i in 0..32 {
            b[i] = ascii::<URL>(v[i]);
        }
        src = &src[24..];
        dst = &mut dst[32..];
    }
    let alphabet = alphabet::<URL>();
    for (a, b) in src
        .as_chunks::<3>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<4>().0)
    {
        let [x, y, z] = *a;
        *b = [
            alphabet[(x >> 2) as usize],
            alphabet[((x << 4 | y >> 4) & 63) as usize],
            alphabet[((y << 2 | z >> 6) & 63) as usize],
            alphabet[(z & 63) as usize],
        ];
    }
}

pub fn encode<const URL: bool, const PAD: bool>(
    input: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    let n = encoded_len(input.len(), PAD).ok_or(Error::Length)?;
    let dst = output.get_mut(..n).ok_or(Error::Output)?;
    let len = input.len() / 3 * 3;
    let (head, tail) = dst.split_at_mut(len / 3 * 4);
    encode_bulk::<URL>(&input[..len], head);
    let alphabet = alphabet::<URL>();
    match &input[len..] {
        [] => {}
        [a] => {
            tail[0] = alphabet[(a >> 2) as usize];
            tail[1] = alphabet[((a << 4) & 63) as usize];
            if PAD {
                tail[2..].fill(b'=');
            }
        }
        [a, b] => {
            tail[0] = alphabet[(a >> 2) as usize];
            tail[1] = alphabet[((a << 4 | b >> 4) & 63) as usize];
            tail[2] = alphabet[((b << 2) & 63) as usize];
            if PAD {
                tail[3] = b'=';
            }
        }
        _ => unreachable!(),
    }
    Ok(n)
}

// Invalid bytes map to a sentinel with the high bit set, allowing a complete
// SIMD-friendly block validity reduction without a branch per byte.
#[inline(always)]
fn value<const URL: bool>(x: u8) -> u8 {
    let alpha = (x | 32).wrapping_sub(b'a');
    let digit = x.wrapping_sub(b'0');
    let s62 = if URL { b'-' } else { b'+' };
    let s63 = if URL { b'_' } else { b'/' };
    if alpha < 26 {
        alpha + if x & 32 != 0 { 26 } else { 0 }
    } else if digit < 10 {
        digit + 52
    } else if x == s62 {
        62
    } else if x == s63 {
        63
    } else {
        128
    }
}

#[inline(always)]
fn decode_block<const URL: bool>(a: &[u8; 32]) -> Option<[u8; 24]> {
    let v: [u8; 32] = core::array::from_fn(|i| value::<URL>(a[i]));
    if v.iter().fold(0, |acc, x| acc | x) & 128 != 0 {
        return None;
    }
    let word: [u32; 8] = core::array::from_fn(|i| {
        let w = u32::from_le_bytes([v[4 * i], v[4 * i + 1], v[4 * i + 2], v[4 * i + 3]]);
        ((w & 0x0000003f) << 18)
            | ((w & 0x00003f00) << 4)
            | ((w & 0x003f0000) >> 10)
            | ((w & 0x3f000000) >> 24)
    });
    let [a, b, c, d, e, f, g, h] = word.map(u32::to_be_bytes);
    Some([
        a[1], a[2], a[3], b[1], b[2], b[3], c[1], c[2], c[3], d[1], d[2], d[3], e[1], e[2], e[3],
        f[1], f[2], f[3], g[1], g[2], g[3], h[1], h[2], h[3],
    ])
}

#[inline(never)]
fn decode_bulk<const URL: bool>(mut src: &[u8], mut dst: &mut [u8]) -> Result<(), Error> {
    while let Some(a) = src.first_chunk::<32>() {
        *dst.first_chunk_mut::<24>()
            .expect("validated output length") = decode_block::<URL>(a).ok_or(Error::Character)?;
        src = &src[32..];
        dst = &mut dst[24..];
    }
    for (a, b) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<3>().0)
    {
        let [w, x, y, z] = a.map(value::<URL>);
        if (w | x | y | z) & 128 != 0 {
            return Err(Error::Character);
        }
        *b = [w << 2 | x >> 4, x << 4 | y >> 2, y << 6 | z];
    }
    Ok(())
}

/// Decode canonical RFC 4648 data. PAD selects required padding (when needed)
/// versus strictly unpadded input. Nonzero unused bits are rejected.
pub fn decode<const URL: bool, const PAD: bool>(
    input: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    let unpadded = if PAD {
        if input.len() % 4 != 0 {
            return Err(Error::Padding);
        }
        input.len() - usize::from(input.last() == Some(&b'=')) - usize::from(input.ends_with(b"=="))
    } else {
        input.len()
    };
    let rem = unpadded % 4;
    if rem == 1 {
        return Err(Error::Length);
    }
    let n = unpadded / 4 * 3 + rem.saturating_sub(1);
    let dst = output.get_mut(..n).ok_or(Error::Output)?;
    let len = unpadded / 4 * 4;
    let (head, tail) = dst.split_at_mut(len / 4 * 3);
    decode_bulk::<URL>(&input[..len], head)?;
    match &input[len..unpadded] {
        [] => {}
        [a, b] => {
            let (a, b) = (value::<URL>(*a), value::<URL>(*b));
            if (a | b) & 128 != 0 {
                return Err(Error::Character);
            }
            if b & 15 != 0 {
                return Err(Error::TrailingBits);
            }
            tail[0] = a << 2 | b >> 4;
        }
        [a, b, c] => {
            let (a, b, c) = (value::<URL>(*a), value::<URL>(*b), value::<URL>(*c));
            if (a | b | c) & 128 != 0 {
                return Err(Error::Character);
            }
            if c & 3 != 0 {
                return Err(Error::TrailingBits);
            }
            tail[0] = a << 2 | b >> 4;
            tail[1] = b << 4 | c >> 2;
        }
        _ => unreachable!(),
    }
    Ok(n)
}
