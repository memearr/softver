//! Backs the README's "Why not semver?" table with the real `semver` crate.
use softver::Version;

#[test]
fn semver_rejects_what_softver_accepts() {
    for s in [
        "1.13.8.5395",
        "1.13",
        "1",
        "v1.2.3",
        " 1.2.3\n",
        "2024.01.05",
    ] {
        assert!(
            semver::Version::parse(s).is_err(),
            "semver unexpectedly accepted {s:?}"
        );
        assert!(Version::parse(s).is_ok(), "softver rejected {s:?}");
    }
}

#[test]
fn semver_misorders_a_git_hash_suffix_softver_does_not() {
    let (a, b) = ("1.41.3-a0bfb8370", "1.41.3");
    // semver: the hash is a pre-release, so the hashed build is *older*.
    assert!(semver::Version::parse(a).unwrap() < semver::Version::parse(b).unwrap());
    assert_eq!(Version::parse(a).unwrap(), Version::parse(b).unwrap());
}
