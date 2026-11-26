//! Explicit portable-vector kernel, used for comparison with auto-vectorization.
use core::simd::{
    Select, Simd, ToBytes,
    cmp::{SimdPartialEq, SimdPartialOrd},
    num::SimdUint,
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
fn pack(values: Bytes) -> Bytes {
    let pair = Simd::<u16, 16>::from_le_bytes(values);
    let pair = ((pair & Simd::splat(63)) << Simd::splat(6)) | (pair >> Simd::splat(8));
    let word = Simd::<u32, 8>::from_le_bytes(pair.to_le_bytes());
    let word = ((word & Simd::splat(4095)) << Simd::splat(12)) | (word >> Simd::splat(16));
    let bytes = word.to_le_bytes();
    simd_swizzle!(
        bytes,
        [
            2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 3, 7, 11, 15, 18, 17, 16, 22, 21, 20, 26, 25,
            24, 30, 29, 28, 19, 23, 27, 31
        ]
    )
}

#[inline]
pub fn decode_store<const URL: bool>(input: &[u8; 32], output: &mut [u8; 28]) -> Option<()> {
    let packed = pack(values::<URL>(Bytes::from_array(*input))?);
    let low = simd_swizzle!(
        packed,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    let high = simd_swizzle!(
        packed,
        [
            16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
        ]
    );
    low.copy_to_slice(&mut output[..16]);
    high.copy_to_slice(&mut output[12..]);
    Some(())
}

// A 32-byte input window contains eight triplets. After the initial block,
// the window starts four bytes before the next triplet so each 128-bit half
// contains exactly four triplets, eliminating the cross-half permutation.
#[inline]
fn expand_words(shuffled: Bytes) -> Bytes {
    const HIGH_MASK: [u16; 16] = [
        0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00,
        0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0,
    ];
    const LOW_MASK: [u16; 16] = [
        0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0,
        0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f,
    ];
    // 65/1025 preserve the high product of 64/1024 with these masks;
    // 0x1010 preserves the low product of 16 for a multiple of 16. This
    // avoids LLVM converting mixed shifts to widened variable shifts.
    const HIGH_FACTOR: [u32; 16] = [
        65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025,
    ];
    const LOW_FACTOR: [u16; 16] = [
        4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256,
    ];
    let words = Simd::<u16, 16>::from_le_bytes(shuffled);
    let high = (((words & Simd::from_array(HIGH_MASK)).cast::<u32>()
        * Simd::from_array(HIGH_FACTOR))
        >> Simd::splat(16))
    .cast::<u16>();
    (high | ((words & Simd::from_array(LOW_MASK)) * Simd::from_array(LOW_FACTOR))).to_le_bytes()
}

#[inline]
fn encode_ascii<const URL: bool>(values: Bytes) -> Bytes {
    const STANDARD_OFFSETS: [u8; 16] = [
        65, 71, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 237, 240, 0, 0,
    ];
    const URL_OFFSETS: [u8; 16] = [
        65, 71, 252, 252, 252, 252, 252, 252, 252, 252, 252, 252, 239, 32, 0, 0,
    ];
    // 0 -> A..Z, 1 -> a..z, 2..11 -> digits, 12/13 -> special symbols.
    let index = values.saturating_sub(Bytes::splat(51))
        + values
            .cast::<i8>()
            .simd_gt(Simd::splat(25))
            .select(Bytes::splat(1), Bytes::splat(0));
    values + super::lookup::nibble(if URL { URL_OFFSETS } else { STANDARD_OFFSETS }, index)
}

#[inline(always)]
#[expect(
    clippy::inline_always,
    reason = "Compile the whole kernel with the dispatch wrapper's enabled target features."
)]
pub fn encode_vectorized<const URL: bool>(input: &[u8], output: &mut [u8]) {
    let (Some(first), Some(head)) = (input.first_chunk::<32>(), output.first_chunk_mut::<32>())
    else {
        super::encode_scalar::<URL>(input, output);
        return;
    };
    let first = simd_swizzle!(
        Bytes::from_array(*first),
        [
            1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10, 13, 12, 14, 13, 16, 15, 17, 16, 19,
            18, 20, 19, 22, 21, 23, 22
        ]
    );
    *head = encode_ascii::<URL>(expand_words(first)).to_array();
    let mut src = &input[20..];
    let mut dst = &mut output[32..];
    while let (Some(input), Some(output)) = (src.first_chunk::<32>(), dst.first_chunk_mut::<32>()) {
        let shuffled = simd_swizzle!(
            Bytes::from_array(*input),
            [
                5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 17, 16, 18, 17, 20, 19, 21,
                20, 23, 22, 24, 23, 26, 25, 27, 26
            ]
        );
        *output = encode_ascii::<URL>(expand_words(shuffled)).to_array();
        src = &src[24..];
        dst = &mut dst[32..];
    }
    super::encode_scalar::<URL>(&src[4..], dst);
}
