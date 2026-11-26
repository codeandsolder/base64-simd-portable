# base64-simd-portable

Experimental Base64 codec using ordinary Rust and LLVM auto-vectorization. No runtime dependencies, no architecture intrinsics, `no_std`, no unsafe code. Standard and URL-safe alphabets, padded and unpadded buffer APIs. This is an experimental baseline, not a production replacement.

The design follows [rust-skills2 SIMD guidance](https://github.com/codeandsolder/rust-skills2/blob/main/rules/opt-simd-portable.md): explicit overflow semantics, safe fixed-size buffers and tails, scalar oracle, cross-target correctness and measured generated code. Optimized CPU target flags are useful for local experiments but not required to execute on baseline systems.

See `docs/experiments.md` for the measured compiler transformations. Run `cargo test --locked --release` for exhaustive parity and `cargo --config 'build.rustflags=["-C","target-cpu=znver2"]' run --locked --release --example bench` for local measurements on Zen 2. Use the appropriate deployment CPU for other machines; never distribute a CPU-specific binary as a universal one.
