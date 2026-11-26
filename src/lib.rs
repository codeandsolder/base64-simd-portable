//! Base64 buffer codecs with portable, compiler-vectorized kernels.
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "portable-simd", feature(portable_simd))]
#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(
    not(all(feature = "std", any(target_arch = "x86", target_arch = "x86_64"))),
    forbid(unsafe_code)
)]
#[cfg(feature = "portable-simd")]
mod vector;
#[cfg(feature = "portable-simd")]
use vector::decode_block;

mod dispatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Invalid input length or encoded length overflow.
    Length,
    /// The destination is too short.
    Output,
    /// An input byte is not part of the selected alphabet.
    Character,
    /// Incorrect padding or padded input length.
    Padding,
    /// Nonzero unused bits in the final sextet.
    TrailingBits,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Length => "invalid Base64 length",
            Self::Output => "output buffer too short",
            Self::Character => "invalid Base64 character",
            Self::Padding => "invalid Base64 padding",
            Self::TrailingBits => "nonzero trailing Base64 bits",
        })
    }
}
impl core::error::Error for Error {}

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Return the encoded byte count, or `None` if it overflows `usize`.
#[must_use = "Use the length to size the output buffer or handle overflow."]
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

const fn alphabet<const URL: bool>() -> &'static [u8; 64] {
    if URL { URL_SAFE } else { STANDARD }
}

// The values entering this function are in 0..64; signed comparisons let LLVM
// use native byte comparisons on architectures without unsigned byte compares.
#[inline]
const fn ascii<const URL: bool>(x: u8) -> u8 {
    let base = if x.cast_signed() > 51 {
        252
    } else if x.cast_signed() > 25 {
        71
    } else {
        65
    };
    let special = if URL {
        if x & 1 == 0 { 239 } else { 32 }
    } else {
        (x & 1).wrapping_mul(3).wrapping_add(237)
    };
    x.wrapping_add(if x.cast_signed() > 61 { special } else { base })
}

// Fixed shuffle gathers [b,a,c,b] for each input triplet. LLVM recognizes
// the byte rearrangement as a vector shuffle instead of individual gathers.
#[inline]
const fn shuffle(a: &[u8; 32]) -> [u8; 32] {
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
#[inline]
fn expand(a: &[u8; 32]) -> [u8; 32] {
    const HIGH_MASK: [u16; 16] = [
        0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00,
        0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0,
    ];
    const HIGH_MULTIPLIER: [u16; 16] = [
        65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025,
    ];
    const LOW_MASK: [u16; 16] = [
        0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0,
        0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f,
    ];
    const LOW_MULTIPLIER: [u16; 16] = [
        4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256,
    ];
    let shuffled = shuffle(a);
    let words: [u16; 16] =
        core::array::from_fn(|i| u16::from_le_bytes([shuffled[2 * i], shuffled[2 * i + 1]]));
    let v: [u16; 16] = core::array::from_fn(|i| {
        let product = u32::from(words[i] & HIGH_MASK[i]) * u32::from(HIGH_MULTIPLIER[i]);
        let bytes = product.to_le_bytes();
        u16::from_le_bytes([bytes[2], bytes[3]])
            | (words[i] & LOW_MASK[i]).wrapping_mul(LOW_MULTIPLIER[i])
    });
    core::array::from_fn(|i| v[i / 2].to_le_bytes()[i % 2])
}

#[inline(always)]
#[expect(
    clippy::inline_always,
    reason = "The dispatch wrapper must compile this whole kernel for its enabled target features."
)]
fn encode_vectorized<const URL: bool>(input: &[u8], output: &mut [u8]) -> Result<(), Error> {
    debug_assert_eq!(input.len() % 3, 0);
    debug_assert_eq!(output.len(), input.len() / 3 * 4);
    let mut src = input;
    let mut dst = output;
    while let Some(a) = src.first_chunk::<32>() {
        let v = expand(a);
        let b = dst.first_chunk_mut::<32>().ok_or(Error::Output)?;
        for i in 0..32 {
            b[i] = ascii::<URL>(v[i]);
        }
        src = &src[24..];
        dst = &mut dst[32..];
    }
    encode_scalar::<URL>(src, dst);
    Ok(())
}

fn encode_scalar<const URL: bool>(src: &[u8], dst: &mut [u8]) {
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

/// Encode using the selected alphabet (`URL`) and padding policy (`PAD`).
///
/// # Errors
/// Returns [`Error::Length`] on length overflow or [`Error::Output`] if the destination is too short.
pub fn encode<const URL: bool, const PAD: bool>(
    input: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    let n = encoded_len(input.len(), PAD).ok_or(Error::Length)?;
    let dst = output.get_mut(..n).ok_or(Error::Output)?;
    let len = input.len() / 3 * 3;
    let (head, tail) = dst.split_at_mut(len / 3 * 4);
    dispatch::encode::<URL>(&input[..len], head)?;
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
        _ => return Err(Error::Length),
    }
    Ok(n)
}

// Invalid bytes map to a sentinel with the high bit set, allowing a complete
// SIMD-friendly block validity reduction without a branch per byte.
#[inline]
const fn value<const URL: bool>(x: u8) -> u8 {
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

#[inline]
#[cfg(not(feature = "portable-simd"))]
fn decode_block<const URL: bool>(input: &[u8; 32]) -> Option<[u8; 24]> {
    let v: [u8; 32] = core::array::from_fn(|i| value::<URL>(input[i]));
    if v.iter().fold(0, |acc, x| acc | x) & 128 != 0 {
        return None;
    }
    let word: [u32; 8] = core::array::from_fn(|i| {
        let w = u32::from_le_bytes([v[4 * i], v[4 * i + 1], v[4 * i + 2], v[4 * i + 3]]);
        ((w & 0x0000_003f) << 18)
            | ((w & 0x0000_3f00) << 4)
            | ((w & 0x003f_0000) >> 10)
            | ((w & 0x3f00_0000) >> 24)
    });
    let bytes = word.map(u32::to_be_bytes);
    Some([
        bytes[0][1],
        bytes[0][2],
        bytes[0][3],
        bytes[1][1],
        bytes[1][2],
        bytes[1][3],
        bytes[2][1],
        bytes[2][2],
        bytes[2][3],
        bytes[3][1],
        bytes[3][2],
        bytes[3][3],
        bytes[4][1],
        bytes[4][2],
        bytes[4][3],
        bytes[5][1],
        bytes[5][2],
        bytes[5][3],
        bytes[6][1],
        bytes[6][2],
        bytes[6][3],
        bytes[7][1],
        bytes[7][2],
        bytes[7][3],
    ])
}

#[inline(always)]
#[expect(
    clippy::inline_always,
    reason = "The dispatch wrapper must compile this whole kernel for its enabled target features."
)]
fn decode_vectorized<const URL: bool>(mut src: &[u8], mut dst: &mut [u8]) -> Result<(), Error> {
    while let Some(a) = src.first_chunk::<32>() {
        *dst.first_chunk_mut::<24>().ok_or(Error::Output)? =
            decode_block::<URL>(a).ok_or(Error::Character)?;
        src = &src[32..];
        dst = &mut dst[24..];
    }
    decode_scalar::<URL>(src, dst)
}

fn decode_scalar<const URL: bool>(src: &[u8], dst: &mut [u8]) -> Result<(), Error> {
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
/// Output may be partially modified on malformed input.
///
/// # Errors
/// Returns an error for invalid length, padding, character, trailing bits, or insufficient output space.
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
    dispatch::decode::<URL>(&input[..len], head)?;
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
        _ => return Err(Error::Length),
    }
    Ok(n)
}
