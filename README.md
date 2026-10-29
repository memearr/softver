# softver

Fast, forgiving parsing and comparison of **real-world software version
strings** — the ones SemVer parsers reject or, worse, mis-order.

```rust
use softver::Version;

// Plex reports "1.41.3.9314-a0bfb8370": four numbers plus a git hash.
let plex: Version = "1.41.3.9314-a0bfb8370".parse().unwrap();
assert!(plex >= "1.13.8.5395".parse().unwrap());          // numeric, per component
assert!(Version::parse("1.9").unwrap() < Version::parse("1.13").unwrap());
assert!(Version::parse("2.0rc1").unwrap() < Version::parse("2.0").unwrap());

// Pull a version out of `--version` output.
let v = Version::find("ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023").unwrap();
assert_eq!(v.release(), &[6, 1, 1]);
```

## Why not `semver`?

`semver` is the right tool for crates and npm packages. For *other people's
software* it falls over:

| Input | `semver` | `softver` |
|---|---|---|
| `1.13.8.5395` (4 parts) | error | ok |
| `1.13` / `1` | error | ok (`== 1.13.0` / `1.0.0`) |
| `v1.2.3`, ` 1.2.3\n` | error | ok |
| `2024.01.05` (leading zeros) | error | ok |
| `1.41.3-a0bfb8370` (3-part + git hash) | parses, but the hash is a **pre-release**, so it sorts *below* `1.41.3` | hash is build info: `== 1.41.3` |
| `1.41.3.9314-a0bfb8370` (Plex) | error | ok, `== 1.41.3.9314` |
| `ffmpeg version 6.1.1-3ubuntu5 …` | error | `Version::find` → `6.1.1` |

## Rules

* Any number of numeric components, compared numerically (`1.9 < 1.13`).
  Missing trailing components are zero (`1.13 == 1.13.0`). Components must fit
  a `u64` (else `ParseError::Overflow`).
* A leading `v`/`V` and surrounding ASCII whitespace are ignored.
* **Pre-release tags** sort before the release, in this order, then by number:
  `dev`/`snapshot`/`nightly` < `alpha`/`a` < `beta`/`b` < `pre`/`preview` <
  `rc`/`c`. Written `1.0rc1`, `1.0-rc.1`, `1.0_RC1`, … — the whole suffix must
  be a tag plus numbers, so a hash like `a0bfb8370` or `b1234567` is **not**
  read as "alpha 0" / "beta 1234567".
* **Anything else** after the numbers (`-a0bfb8370`, `+build.5`, `-3ubuntu5`)
  is build metadata: available from `build()`, ignored by `==`, `Ord` and
  `Hash` (which are consistent with each other).
* `Version::parse` is strict about the rest of the string
  (`ParseError::Trailing`); `Version::find` scans free text and stops at
  whitespace, `,`, `;`, `(`, `)`.

### Deliberately not handled

* Post-release tags (`1.0.post1`, `1.0-p1`, `1.0-sp2`) are treated as build
  metadata, i.e. equal to `1.0`.
* Epochs (`1:2.3`), Python `.devN` ordering quirks, and distro-specific
  revision rules (dpkg/rpm) — use a package-manager-specific crate for those.
* Non-ASCII whitespace is not trimmed.

## Performance

No dependencies, no `unsafe`. Plain `x.y.z` versions parse without allocating
(up to four components live inline); tags and build suffixes allocate once.
`cargo run --release --example bench` on the author's Windows box
(`semver` for reference):

```
softver  parse  x.y.z                          ~30 ns
semver   parse  x.y.z                          ~16 ns
softver  parse  rc / git-hash suffix          ~125 ns   (allocates)
semver   parse  2.0.0-rc.1                     ~30 ns
softver  compare                                ~4 ns
```

So it is **slower than `semver`** at parsing (about 2× on plain versions,
more with suffixes) — it does more — but still tens of millions of parses per
second, and comparison is a few nanoseconds. The suffix path is dominated by
Windows' slow system allocator; expect it to be closer on Linux (not measured).

## License

MIT
