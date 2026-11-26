//! Small register-resident table lookups expressed as portable vector extraction.
use core::{intrinsics::simd::simd_extract_dyn, simd::Simd};

/// Index a sixteen-byte table with the low nibble of each lane. The mask is
/// intentional: callers may supply any byte, and no lane can index out of bounds.
/// Unlike `Simd::swizzle_dyn`, generic extraction is optimized using the caller's
/// target features rather than the features selected when `core` was built.
#[inline]
pub fn nibble(table: [u8; 16], indices: Simd<u8, 32>) -> Simd<u8, 32> {
    let table = Simd::<u8, 16>::from_array(table);
    let indices = indices & Simd::splat(15);
    Simd::from_array(core::array::from_fn(|lane| {
        // SAFETY: masking every lane with 15 proves its value is in 0..16.
        // This intrinsic extracts a vector element, not a memory address.
        unsafe { simd_extract_dyn(table, u32::from(indices[lane])) }
    }))
}
