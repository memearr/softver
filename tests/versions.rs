use softver::{compare, ParseError, Version};
use std::cmp::Ordering::{Equal, Greater, Less};
use std::collections::HashSet;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

#[test]
fn compares_numerically_not_lexically() {
    assert_eq!(compare("1.9", "1.13"), Ok(Less));
    assert_eq!(compare("1.10", "1.9"), Ok(Greater));
    assert_eq!(compare("1.13.8.5394", "1.13.8.5395"), Ok(Less));
    assert_eq!(compare("1.13.8.5395", "1.13.8.5395"), Ok(Equal));
    assert_eq!(compare("2", "10"), Ok(Less));
    assert_eq!(compare("1.0.0.1", "1.0.0"), Ok(Greater));
}

#[test]
fn missing_trailing_components_are_zero() {
    assert_eq!(compare("1.13", "1.13.0"), Ok(Equal));
    assert_eq!(compare("1.13", "1.13.0.0"), Ok(Equal));
    assert_eq!(compare("1", "1.0.0"), Ok(Equal));
    assert_eq!(compare("1.13", "1.13.0.1"), Ok(Less));
    // ...and equal versions hash equal.
    let set: HashSet<Version> = ["1.13", "1.13.0", "v1.13.0.0"]
        .iter()
        .map(|s| v(s))
        .collect();
    assert_eq!(set.len(), 1);
}

#[test]
fn any_number_of_components_and_leading_zeros() {
    assert_eq!(v("1.2.3.4.5.6").release(), &[1, 2, 3, 4, 5, 6]);
    assert_eq!(v("2024.01.05").release(), &[2024, 1, 5]);
    assert_eq!(compare("2024.01.05", "2024.1.5"), Ok(Equal));
    assert_eq!(compare("2024.10.01", "2024.9.30"), Ok(Greater));
    assert_eq!(v("7").major(), 7);
    assert_eq!((v("7").minor(), v("7").patch()), (0, 0));
}

#[test]
fn leading_v_and_whitespace_are_ignored() {
    assert_eq!(v("v1.2.3"), v("1.2.3"));
    assert_eq!(v("V1.2.3"), v("1.2.3"));
    assert_eq!(v("  1.2.3\n"), v("1.2.3"));
}

#[test]
fn build_suffixes_and_hashes_do_not_affect_ordering() {
    // The case SemVer gets wrong: a git-hash suffix is not a pre-release.
    let plex = v("1.41.3.9314-a0bfb8370");
    assert_eq!(plex, v("1.41.3.9314"));
    assert!(plex >= v("1.13.8.5395"));
    assert_eq!(plex.build(), Some("a0bfb8370"));
    assert!(!plex.is_prerelease());
    assert_eq!(v("6.1.1-3ubuntu5"), v("6.1.1"));
    assert_eq!(v("1.0.0+build.5"), v("1.0.0"));
    assert_eq!(v("1.0.0+build.5").build(), Some("build.5"));
    assert_eq!(v("1.0-1"), v("1.0")); // Debian-style revision
                                      // Hashes that merely *start* like a tag are still build info.
    for h in ["a0bfb8370", "b1234567", "c0ffee1", "dev0abcdef"] {
        let x = v(&format!("1.0.0-{h}"));
        assert!(!x.is_prerelease(), "{h}");
        assert_eq!(x, v("1.0.0"), "{h}");
    }
}

#[test]
fn prereleases_sort_before_the_release_in_stage_order() {
    let order = [
        "1.0-dev",
        "1.0-alpha",
        "1.0-alpha.2",
        "1.0-beta.1",
        "1.0-beta.2",
        "1.0-pre",
        "1.0rc1",
        "1.0rc2",
        "1.0",
    ];
    for w in order.windows(2) {
        assert_eq!(compare(w[0], w[1]), Ok(Less), "{} < {}", w[0], w[1]);
    }
    assert_eq!(compare("1.0a1", "1.0-alpha.1"), Ok(Equal));
    assert_eq!(compare("1.0b2", "1.0-BETA-2"), Ok(Equal));
    assert_eq!(compare("1.0rc1", "0.9"), Ok(Greater)); // release part dominates
    assert!(v("2.0.0-rc.1").is_prerelease());
    assert_eq!(compare("1.0rc1+abc", "1.0rc1+xyz"), Ok(Equal));
    assert_eq!(compare("1.0rc1-a0bfb8370", "1.0rc1"), Ok(Equal));
}

#[test]
fn rejects_what_is_not_a_version() {
    assert_eq!(Version::parse(""), Err(ParseError::Empty));
    assert_eq!(Version::parse("   "), Err(ParseError::Empty));
    assert_eq!(Version::parse("v"), Err(ParseError::NoNumber));
    assert_eq!(Version::parse("garbage"), Err(ParseError::NoNumber));
    assert_eq!(Version::parse(".1.2"), Err(ParseError::NoNumber));
    assert_eq!(Version::parse("1.2.3 and more"), Err(ParseError::Trailing));
    assert_eq!(Version::parse("1.2.3/x"), Err(ParseError::Trailing));
    assert_eq!(
        Version::parse("1.99999999999999999999999"),
        Err(ParseError::Overflow)
    );
}

#[test]
fn a_trailing_dot_is_not_a_component() {
    assert_eq!(v("1.2."), v("1.2"));
    assert_eq!(v("1.2.").release(), &[1, 2]);
}

#[test]
fn finds_versions_in_free_text() {
    let f = |t: &str| Version::find(t).map(|v| v.release().to_vec());
    assert_eq!(
        f("ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023"),
        Some(vec![6, 1, 1])
    );
    assert_eq!(
        f("Plex Media Server v1.41.3.9314-a0bfb8370"),
        Some(vec![1, 41, 3, 9314])
    );
    assert_eq!(f("gifsicle 1.95\nCopyright"), Some(vec![1, 95]));
    assert_eq!(f("Python 3.12.1."), Some(vec![3, 12, 1]));
    assert_eq!(f("(1.2.3)"), Some(vec![1, 2, 3]));
    assert_eq!(f("version=2.0,"), Some(vec![2, 0]));
    // Not versions: lone numbers, glued to letters/dots, no number at all.
    assert_eq!(f("build 7 of 12"), None);
    assert_eq!(f("abc1.2.3"), None);
    assert_eq!(f("no digits here"), None);
    assert_eq!(f(""), None);
    // The first real version wins, skipping dates-as-lone-numbers earlier on.
    assert_eq!(f("rev 12 released 3.4.5 (2.0.0)"), Some(vec![3, 4, 5]));
    assert_eq!(
        Version::find("tool 2.0rc1 beta").map(|v| v.is_prerelease()),
        Some(true)
    );
}

#[test]
fn display_round_trips_the_meaningful_parts() {
    assert_eq!(v("v1.2.3").to_string(), "1.2.3");
    assert_eq!(
        v("1.41.3.9314-a0bfb8370").to_string(),
        "1.41.3.9314+a0bfb8370"
    );
    assert_eq!(v("2.0rc1").to_string(), "2.0-rc.1");
    assert_eq!(v("1.0-BETA-2").to_string(), "1.0-beta.2");
    for s in ["1.2.3", "1.0-rc1", "1.0+abc", "1.0.0.0.1", "1.0-rc1+abc"] {
        assert_eq!(v(&v(s).to_string()), v(s), "{s}");
        assert_eq!(v(&v(s).to_string()).to_string(), v(s).to_string(), "{s}");
    }
}

#[test]
fn at_least_helper() {
    assert_eq!(v("1.41.3.9314-a0bfb8370").at_least("1.13.8.5395"), Ok(true));
    assert_eq!(v("1.13.8.5394").at_least("1.13.8.5395"), Ok(false));
    assert_eq!(v("1.0").at_least("nope"), Err(ParseError::NoNumber));
}

#[test]
fn ordering_is_total_and_consistent_on_a_mixed_list() {
    let mut list: Vec<Version> = [
        "1.0",
        "1.0rc1",
        "0.9.9",
        "1.0.1",
        "1.0-beta",
        "10.0",
        "2",
        "1.0.0.0.1",
        "1.0-alpha.1",
    ]
    .iter()
    .map(|s| v(s))
    .collect();
    list.sort();
    let got: Vec<String> = list.iter().map(|x| x.to_string()).collect();
    assert_eq!(
        got,
        [
            "0.9.9",
            "1.0-alpha.1",
            "1.0-beta",
            "1.0-rc.1",
            "1.0",
            "1.0.0.0.1",
            "1.0.1",
            "2",
            "10.0"
        ]
    );
}

/// Parsing must never panic, whatever it is fed (including multibyte text
/// where a byte-index slice could land mid-character), and anything that
/// parses must round-trip through Display.
#[test]
fn never_panics_on_arbitrary_input() {
    let alphabet: Vec<&str> = vec![
        "0",
        "1",
        "9",
        "12",
        "99999999999999999999",
        ".",
        "..",
        "-",
        "+",
        "_",
        "~",
        " ",
        "\t",
        "v",
        "V",
        "a",
        "b",
        "c",
        "rc",
        "beta",
        "dev",
        "é",
        "日本",
        "🦀",
        "(",
        ")",
        ",",
        "x",
        "A0bf",
    ];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..50_000 {
        let len = (next() % 12) as usize;
        let s: String = (0..len)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        let _ = Version::find(&s);
        if let Ok(parsed) = Version::parse(&s) {
            let again = Version::parse(&parsed.to_string())
                .unwrap_or_else(|e| panic!("{s:?} -> {parsed} failed to re-parse: {e}"));
            assert_eq!(parsed, again, "{s:?}");
        }
    }
}
