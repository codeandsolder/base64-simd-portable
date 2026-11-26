# Compiler-guided experiments

Initial implementation and measurements (nightly-2026-11-22, LLVM 23.1.6, Ryzen 9 4900HS): at 64 KiB encoding 12.33 GB/s vs base64 0.23.1 14.69 and base64-simd 0.8.0 19.84; at 8 MiB roughly equal. Decoder 3.37 GB/s vs 23.51 for base64-simd. Numbers are historical, not acceptance thresholds. Full paired timings and original source/disassembly archived on Acer under `/home/jan/scratch/js-redox-dependency-audit-20261126/base64-structured-evidence.tar.gz`.

Use fixed byte shuffle and mathematically equivalent non-power-of-two multipliers to preserve the high/low halves of packed 16-bit products; LLVM emits vpshufb/vpmulhuw/vpmullw. Using exact power-of-two factors instead caused widening and variable shifts. `-mul-constant-optimization=false` did not change the disassembly. Related upstream issue: https://github.com/llvm/llvm-project/issues/140418. Same source built for wasm32 with +simd128 emits SIMD operations, not yet measured on WASM. See `src/lib.rs` for the arithmetic proof.

Remaining work: decoder inverse packing and validation, compare scalar auto-vectorization with portable SIMD, baseline fallback and CPU dispatch, codegen and runtime tests across target features. Follow rust-skills2 opt-simd-portable, opt-target-cpu, opt-bounds-check, perf-array-windows, num-overflow-explicit and perf-benchmark-regression. Do not introduce architecture intrinsics; preserve strict source policy.

## Portable decoder and runtime dispatch (2026-11-26)

Fleet toolchain nightly-2026-11-19 (LLVM 23.1.6), Asus Ryzen 9 4900HS. No native intrinsics. Replaced inefficient lane extraction with safe portable SIMD `ToBytes` to reinterpret adjacent sextets as 16-bit pairs, then 32-bit words, followed by a constant shuffle. Explicit casts and swizzles initially reached 4.42 GB/s; safe byte reinterpretation reached 8.57 GB/s versus 3.33 GB/s for the auto-vectorized decoder. A nibble classifier with dynamic `swizzle_dyn` was rejected (0.375 GB/s: compiler scalarization). Encoder remains ordinary Rust; `#[target_feature]` wrappers dispatch after feature detection, retaining a baseline scalar path. Compiler target-feature inlining is deliberate and locally lint-reviewed. Benchmark uses nine alternating paired rounds and initialized buffers; input bytes are raw for encode and encoded for decode.

| Input (raw) | Runtime encode | Runtime decode | Base64-simd encode | Base64-simd decode |
| --- | ---: | ---: | ---: | ---: |
| 4 KiB | 11.39 GB/s | 8.24 GB/s | 18.43 GB/s | 21.70 GB/s |
| 64 KiB | 12.05 GB/s | 8.52 GB/s | 19.51 GB/s | 22.38 GB/s |
| 1 MiB | 11.24 GB/s | 8.15 GB/s | 18.07 GB/s | 21.21 GB/s |
| 8 MiB | 6.09 GB/s | 6.86 GB/s | 6.36 GB/s | 8.68 GB/s |

Raw evidence: `docs/benchmarks/2026-11-26-runtime.tsv` and `2026-11-26-native.tsv`. The two references are development-only. Release parity passed both default runtime dispatch and forced baseline `--no-default-features`, exhaustive triplets across both alphabets, alignment offsets, padding modes, invalid byte injection and output guards. Local pinned shared strict gate passed. AArch64/WASM execution and performance remain to be measured; compile checks alone are not runtime validation. Next: reduce decode validation cost without scalar dynamic lookup, examine LLVM's multiply-add combining, tune short-input paths and compare real source-map workloads. Do not adopt in js-redox yet.

## Register lookup and overlapping loads/stores

The default encoder uses a portable dynamic extraction helper over a 16-byte table with every index masked to four bits. LLVM lowers this to packed byte shuffles; the prebuilt core's swizzle_dyn did not. This requires the unstable generic core_intrinsics API but does not introduce native intrinsics. Overlapping 32-byte input windows avoid cross-lane permutation. Two overlapping 16-byte decode stores fit in a checked 28-byte destination and advance by 24 bytes; the final short block uses the scalar path. Malformed input may modify the validated output prefix.

Runtime AVX2 dispatch with generic build, nine alternating paired rounds on Asus: encoder ratios against base64-simd at 120B/4KiB/64KiB/1MiB/8MiB were 0.752/1.067/1.078/1.054/1.032. 64KiB measured 16.180 vs 14.708 GB/s under load. Full release differential parity passed. Overlap-only decoder kernel measured 11.283 GB/s vs 22.460 at 64KiB. Experimental nibble tables and batched validity reached 13.103 GB/s; not yet integrated. Full run in docs/benchmarks/2026-11-26-vector-encoder.tsv. Scratch prototypes and assembly: Asus /home/jan/scratch/base64-simd-portable/20261126-throughput/.
