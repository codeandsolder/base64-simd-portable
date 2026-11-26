# base64-simd-portable

Experimental dependency-free Base64 codec with compiler-vectorized encoding and explicit portable SIMD decoding. Standard and URL-safe alphabets, padded and unpadded buffer APIs, strict canonical decoding. No native SIMD intrinsics. Default features enable runtime AVX2 dispatch on x86/x86-64 and portable SIMD via nightly `core::simd`; the only unsafe operations are the documented CPU-feature call boundaries. `--no-default-features` gives a `no_std`, entirely safe scalar fallback on baseline targets. Target-native vector support is selected at compile time for AVX2, NEON and WebAssembly SIMD. No runtime dependencies.

The design follows [rust-skills2 SIMD guidance](https://github.com/codeandsolder/rust-skills2/blob/main/rules/opt-simd-portable.md): explicit overflow semantics, safe fixed-size buffers and tails, scalar oracle, cross-target correctness and measured generated code. Optimized CPU target flags are useful for local experiments but not required to execute on baseline systems.

```rust
let mut encoded = [0; 8];
let n = base64_simd_portable::encode::<false, true>(b"hello", &mut encoded)?;
let mut decoded = [0; 5];
let m = base64_simd_portable::decode::<false, true>(&encoded[..n], &mut decoded)?;
assert_eq!(&decoded[..m], b"hello");
# Ok::<(), base64_simd_portable::Error>(())
```

`URL` selects the URL-safe alphabet; `PAD` selects required canonical padding (when needed) or strictly unpadded input. The destination can be larger than required; the returned count determines the written prefix. Decoding may modify the prefix before returning an error.

Run `cargo test --locked --release` for exhaustive parity and `cargo test --locked --release --no-default-features` for the baseline scalar path. `cargo bench --locked --bench throughput` compares the default redistributable build against both reference crates; `cargo --config 'build.rustflags=["-C","target-cpu=znver2"]' bench --locked --bench throughput` measures a Zen 2-specific build. Never distribute a CPU-specific binary as a universal one. See [experiments](docs/experiments.md) for measured results and limitations.
