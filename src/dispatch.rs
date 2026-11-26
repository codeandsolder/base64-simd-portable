//! Dispatch is the only architecture-specific boundary; the kernels contain
//! neither native intrinsics nor unchecked memory operations.
use super::{Error, decode_scalar, decode_vectorized, encode_scalar, encode_vectorized};

// Targets with baseline vector support can use the same vectorized kernels
// without runtime feature detection. Other `no_std` targets use scalar code.
const COMPILED_VECTOR_SUPPORT: bool = cfg!(any(
    target_feature = "avx2",
    target_feature = "neon",
    target_feature = "simd128",
));

#[cfg(all(feature = "std", any(target_arch = "x86", target_arch = "x86_64")))]
#[target_feature(enable = "avx2")]
fn encode_avx2<const URL: bool>(input: &[u8], output: &mut [u8]) {
    encode_vectorized::<URL>(input, output);
}

#[cfg(all(feature = "std", any(target_arch = "x86", target_arch = "x86_64")))]
#[target_feature(enable = "avx2")]
fn decode_avx2<const URL: bool>(input: &[u8], output: &mut [u8]) -> Result<(), Error> {
    decode_vectorized::<URL>(input, output)
}

pub fn encode<const URL: bool>(input: &[u8], output: &mut [u8]) {
    if COMPILED_VECTOR_SUPPORT {
        return encode_vectorized::<URL>(input, output);
    }
    #[cfg(all(feature = "std", any(target_arch = "x86", target_arch = "x86_64")))]
    if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection establishes AVX2 support before entering
        // the specialized kernel. Its ordinary slice arguments remain safe.
        return unsafe { encode_avx2::<URL>(input, output) };
    }
    encode_scalar::<URL>(input, output);
}

pub fn decode<const URL: bool>(input: &[u8], output: &mut [u8]) -> Result<(), Error> {
    if COMPILED_VECTOR_SUPPORT {
        return decode_vectorized::<URL>(input, output);
    }
    #[cfg(all(feature = "std", any(target_arch = "x86", target_arch = "x86_64")))]
    if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection establishes AVX2 support before entering
        // the specialized kernel. Its ordinary slice arguments remain safe.
        return unsafe { decode_avx2::<URL>(input, output) };
    }
    decode_scalar::<URL>(input, output)
}
