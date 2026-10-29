//! Rough timing against the `semver` crate. Run with
//! `cargo run --release --example bench`.
use std::hint::black_box;
use std::time::Instant;

const N: usize = 2_000_000;

fn per_op(f: impl FnOnce() -> u64) -> (f64, u64) {
    let t = Instant::now();
    let acc = f();
    (t.elapsed().as_nanos() as f64 / N as f64, acc)
}

fn main() {
    let plain = ["1.41.3", "10.20.30", "0.9.12", "2.3.4", "1.13.8"];
    let tagged = ["2.0.0-rc.1", "1.41.3.9314-a0bfb8370"];
    let mut sink = 0u64;
    let mut run = |label: &str, f: &mut dyn FnMut() -> u64| {
        let (ns, acc) = per_op(f);
        sink += acc;
        println!("{label:<44}{ns:>7.1} ns/op");
    };

    run("softver  parse  x.y.z", &mut || {
        (0..N)
            .map(|i| {
                softver::Version::parse(black_box(plain[i % 5]))
                    .unwrap()
                    .major()
            })
            .sum()
    });
    run("semver   parse  x.y.z", &mut || {
        (0..N)
            .map(|i| {
                semver::Version::parse(black_box(plain[i % 5]))
                    .unwrap()
                    .major
            })
            .sum()
    });
    run("softver  parse  rc / git-hash suffix", &mut || {
        (0..N)
            .map(|i| {
                softver::Version::parse(black_box(tagged[i % 2]))
                    .unwrap()
                    .major()
            })
            .sum()
    });
    run("semver   parse  2.0.0-rc.1", &mut || {
        (0..N)
            .map(|_| semver::Version::parse(black_box(tagged[0])).unwrap().major)
            .sum()
    });
    let (x, y) = (
        softver::Version::parse("1.41.3").unwrap(),
        softver::Version::parse("1.13.8").unwrap(),
    );
    run("softver  compare", &mut || {
        (0..N).map(|_| (black_box(&x) > black_box(&y)) as u64).sum()
    });
    black_box(sink);
}
