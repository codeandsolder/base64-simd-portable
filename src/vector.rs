//! Explicit portable-vector kernel, used for comparison with auto-vectorization.
use core::simd::{
    Select, Simd, ToBytes,
    cmp::{SimdPartialEq, SimdPartialOrd},
    simd_swizzle,
};

type Bytes = Simd<u8, 32>;

#[inline]
fn values<const URL: bool>(input: Bytes) -> Option<Bytes> {
    let upper = input - Bytes::splat(b'A');
    let lower = input - Bytes::splat(b'a');
    let digit = input - Bytes::splat(b'0');
    let is_upper = upper.simd_lt(Bytes::splat(26));
    let is_lower = lower.simd_lt(Bytes::splat(26));
    let is_digit = digit.simd_lt(Bytes::splat(10));
    let is_62 = input.simd_eq(Bytes::splat(if URL { b'-' } else { b'+' }));
    let is_63 = input.simd_eq(Bytes::splat(if URL { b'_' } else { b'/' }));
    if !(is_upper | is_lower | is_digit | is_62 | is_63).all() {
        return None;
    }
    Some(is_upper.select(
        upper,
        is_lower.select(
            lower + Bytes::splat(26),
            is_digit.select(
                digit + Bytes::splat(52),
                is_62.select(Bytes::splat(62), Bytes::splat(63)),
            ),
        ),
    ))
}

// Adjacent sextet pairs form 12-bit values; adjacent pairs form 24-bit words.
// Explicit constant shuffles avoid the widening/gather cascade from scalar code.
#[inline]
fn pack(values: Bytes) -> [u8; 24] {
    let pair = Simd::<u16, 16>::from_le_bytes(values);
    let pair = ((pair & Simd::splat(63)) << Simd::splat(6)) | (pair >> Simd::splat(8));
    let word = Simd::<u32, 8>::from_le_bytes(pair.to_le_bytes());
    let word = ((word & Simd::splat(4095)) << Simd::splat(12)) | (word >> Simd::splat(16));
    let bytes = word.to_be_bytes();
    simd_swizzle!(
        bytes,
        [
            1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15, 17, 18, 19, 21, 22, 23, 25, 26, 27, 29, 30, 31
        ]
    )
    .to_array()
}

#[inline]
pub fn decode_block<const URL: bool>(input: &[u8; 32]) -> Option<[u8; 24]> {
    Some(pack(values::<URL>(Bytes::from_array(*input))?))
}
