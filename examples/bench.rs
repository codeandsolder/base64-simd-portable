use base64::{
    Engine,
    engine::{Simd, general_purpose::PAD},
};
use base64_simd::AsOut;
use std::{hint::black_box, time::Instant};
fn sample(mut f: impl FnMut(), n: usize) -> f64 {
    let t = Instant::now();
    for _ in 0..n {
        f();
    }
    t.elapsed().as_secs_f64() / n as f64
}
fn compare(label: &str, bytes: usize, n: usize, mut a: impl FnMut(), mut b: impl FnMut()) {
    let (mut ar, mut br, mut ratios) = (Vec::new(), Vec::new(), Vec::new());
    for r in 0..9 {
        let (a, b) = if r % 2 == 0 {
            (sample(&mut a, n), sample(&mut b, n))
        } else {
            let b = sample(&mut b, n);
            let a = sample(&mut a, n);
            (a, b)
        };
        ar.push(a);
        br.push(b);
        ratios.push(a / b);
    }
    ar.sort_by(f64::total_cmp);
    br.sort_by(f64::total_cmp);
    ratios.sort_by(f64::total_cmp);
    println!(
        "{label}\t{bytes}\t{:.3}\t{:.3}\t{:.3}",
        bytes as f64 / ar[4] / 1e9,
        bytes as f64 / br[4] / 1e9,
        ratios[4]
    );
}
fn main() {
    let engine = Simd::standard(PAD);
    println!("operation\tinput_bytes\treference_GB/s\tautovec_GB/s\tpaired_ratio");
    for size in [120, 4096, 65536, 1048576, 8388608] {
        let mut data = vec![0; size];
        let mut state = 1234567u32;
        for x in &mut data {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *x = state as u8;
        }
        let enc = base64_simd::STANDARD.encode_to_string(&data);
        let mut a = vec![0; enc.len()];
        let mut b = a.clone();
        let mut aa = vec![0; data.len()];
        let mut bb = aa.clone();
        let n = (16 * 1024 * 1024 / size).clamp(4, 100000);
        assert_eq!(
            base64_simd_portable::encode::<false, true>(&data, &mut a).unwrap(),
            enc.len()
        );
        assert_eq!(a, enc.as_bytes());
        assert_eq!(
            base64_simd_portable::decode::<false, true>(&a, &mut aa).unwrap(),
            size
        );
        assert_eq!(aa, data);
        compare(
            "encode vs base64-simd",
            size,
            n,
            || {
                black_box(
                    base64_simd::STANDARD
                        .encode(black_box(&data), black_box(b.as_mut_slice()).as_out()),
                );
            },
            || {
                black_box(
                    base64_simd_portable::encode::<false, true>(black_box(&data), black_box(&mut a))
                        .unwrap(),
                );
            },
        );
        compare(
            "encode vs base64",
            size,
            n,
            || {
                black_box(
                    engine
                        .encode_slice(black_box(&data), black_box(&mut b))
                        .unwrap(),
                );
            },
            || {
                black_box(
                    base64_simd_portable::encode::<false, true>(black_box(&data), black_box(&mut a))
                        .unwrap(),
                );
            },
        );
        compare(
            "decode vs base64-simd",
            enc.len(),
            n,
            || {
                black_box(
                    base64_simd::STANDARD
                        .decode(
                            black_box(enc.as_bytes()),
                            black_box(bb.as_mut_slice()).as_out(),
                        )
                        .unwrap(),
                );
            },
            || {
                black_box(
                    base64_simd_portable::decode::<false, true>(
                        black_box(enc.as_bytes()),
                        black_box(&mut aa),
                    )
                    .unwrap(),
                );
            },
        );
        compare(
            "decode vs base64",
            enc.len(),
            n,
            || {
                black_box(
                    engine
                        .decode_slice(black_box(enc.as_bytes()), black_box(&mut bb))
                        .unwrap(),
                );
            },
            || {
                black_box(
                    base64_simd_portable::decode::<false, true>(
                        black_box(enc.as_bytes()),
                        black_box(&mut aa),
                    )
                    .unwrap(),
                );
            },
        );
    }
}
