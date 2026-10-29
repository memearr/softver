//! Fast, forgiving parsing and comparison of real-world software versions.
//!
//! Real version strings are not all SemVer: Plex reports
//! `1.41.3.9314-a0bfb8370`, Windows uses four parts, calendar-versioned tools
//! use `2024.01.05`, and `--version` output buries the number in prose. SemVer
//! parsers reject most of these, or worse, *mis-order* them (the trailing
//! `-a0bfb8370` git hash is a SemVer "pre-release", which would sort
//! `1.41.3.9314-a0bfb8370` *before* `1.41.3.9314`).
//!
//! `softver` parses what is actually out there and orders it the way people
//! expect:
//!
//! * Any number of numeric components: `1`, `1.2`, `1.13.8.5395`.
//! * Compared numerically per component (`1.9 < 1.13`), with missing trailing
//!   components equal to zero (`1.13 == 1.13.0`).
//! * A leading `v`/`V` is ignored.
//! * Recognised pre-release tags (`dev`, `snapshot`, `nightly`, `alpha`/`a`,
//!   `beta`/`b`, `pre`, `preview`, `rc`) sort *before* the release, in that
//!   order, then by their number: `1.0rc2 < 1.0` and `1.0-beta.2 < 1.0rc1`.
//! * Anything else after the numbers (`-a0bfb8370`, `+build.5`, `-3ubuntu5`) is
//!   **build metadata**: kept for display, ignored for ordering and equality.
//! * [`Version::find`] pulls a version out of free text.
//!
//! No dependencies, no `unsafe`. Plain `x.y.z` versions parse without allocating.
//!
//! ```
//! use softver::Version;
//!
//! let plex: Version = "1.41.3.9314-a0bfb8370".parse().unwrap();
//! assert!(plex >= "1.13.8.5395".parse().unwrap());
//! assert!(Version::parse("1.9").unwrap() < Version::parse("1.13").unwrap());
//! assert!(Version::parse("2.0rc1").unwrap() < Version::parse("2.0").unwrap());
//!
//! let v = Version::find("ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023").unwrap();
//! assert_eq!(v.release(), &[6, 1, 1]);
//! ```

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// Why a string could not be parsed as a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The input was empty (or only whitespace / a lone `v`).
    Empty,
    /// The input did not start with a digit (after an optional `v`).
    NoNumber,
    /// A numeric component does not fit in a `u64`.
    Overflow,
    /// Unexpected characters followed the version (only [`Version::parse`];
    /// [`Version::find`] stops at them instead).
    Trailing,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseError::Empty => "empty version string",
            ParseError::NoNumber => "version must start with a number",
            ParseError::Overflow => "version component too large",
            ParseError::Trailing => "unexpected characters after version",
        })
    }
}

impl std::error::Error for ParseError {}

/// Pre-release tags, in ascending order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Stage {
    Dev,
    Alpha,
    Beta,
    Pre,
    Rc,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::Dev => "dev",
            Stage::Alpha => "alpha",
            Stage::Beta => "beta",
            Stage::Pre => "pre",
            Stage::Rc => "rc",
        }
    }
}

#[derive(Debug, Clone)]
struct Pre {
    stage: Stage,
    /// Numbers after the tag (`rc.2.1` -> `[2, 1]`), trailing zeros ignored
    /// when comparing.
    num: Nums,
}

/// A short list of numbers that lives inline (no allocation) for the common
/// case of four or fewer, spilling to the heap beyond that.
#[derive(Debug, Clone)]
enum Nums {
    Inline([u64; 4], u8),
    Heap(Vec<u64>),
}

impl Nums {
    fn new() -> Self {
        Nums::Inline([0; 4], 0)
    }

    fn push(&mut self, n: u64) {
        match self {
            Nums::Inline(buf, len) if usize::from(*len) < buf.len() => {
                buf[usize::from(*len)] = n;
                *len += 1;
            }
            Nums::Inline(buf, _) => {
                let mut v = Vec::with_capacity(8);
                v.extend_from_slice(buf);
                v.push(n);
                *self = Nums::Heap(v);
            }
            Nums::Heap(v) => v.push(n),
        }
    }

    fn as_slice(&self) -> &[u64] {
        match self {
            Nums::Inline(buf, len) => &buf[..usize::from(*len)],
            Nums::Heap(v) => v,
        }
    }
}

/// A parsed software version. See the [crate docs](crate).
#[derive(Debug, Clone)]
pub struct Version {
    release: Nums,
    pre: Option<Box<Pre>>,
    build: Option<Box<str>>,
}

impl Version {
    /// Parse a whole string as a version (surrounding ASCII whitespace allowed).
    pub fn parse(input: &str) -> Result<Version, ParseError> {
        let input = input.trim_ascii();
        let (v, rest) = parse_prefix(input)?;
        if rest.is_empty() {
            Ok(v)
        } else {
            Err(ParseError::Trailing)
        }
    }

    /// Find the first version in free text, e.g. `--version` output:
    /// `"ffmpeg version 6.1.1-3ubuntu5 Copyright"` -> `6.1.1`.
    ///
    /// A match needs at least one `.` between numbers (so `"build 7"` isn't
    /// a version), must not be glued to a preceding letter/digit/dot, and ends
    /// at whitespace, `,`, `;`, `(` or `)`. Returns `None` if there is none.
    pub fn find(text: &str) -> Option<Version> {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                // A leading `v` is allowed if it is itself at a boundary.
                let start_ok = match i.checked_sub(1).map(|p| b[p]) {
                    None => true,
                    Some(c) if (c == b'v' || c == b'V') => {
                        i == 1 || !b[i - 2].is_ascii_alphanumeric()
                    }
                    Some(c) => !(c.is_ascii_alphanumeric() || c == b'.'),
                };
                if start_ok {
                    let end = b[i..]
                        .iter()
                        .position(|&c| {
                            c.is_ascii_whitespace() || matches!(c, b',' | b';' | b'(' | b')')
                        })
                        .map_or(text.len(), |n| i + n);
                    // Prefer the whole token; if its tail is junk, fall back to
                    // the longest prefix that parses (the numbers alone).
                    if let Ok((v, _)) = parse_prefix(&text[i..end]) {
                        if v.release().len() >= 2 {
                            return Some(v);
                        }
                    }
                }
                // Skip the rest of this digit run so "12.x" isn't retried at "2".
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        None
    }

    /// The numeric components, e.g. `[1, 13, 8, 5395]`. As written: trailing
    /// zeros are kept (they just don't affect comparison).
    pub fn release(&self) -> &[u64] {
        self.release.as_slice()
    }

    /// First component.
    pub fn major(&self) -> u64 {
        self.release.as_slice()[0]
    }

    /// Second component, `0` if absent.
    pub fn minor(&self) -> u64 {
        self.release.as_slice().get(1).copied().unwrap_or(0)
    }

    /// Third component, `0` if absent.
    pub fn patch(&self) -> u64 {
        self.release.as_slice().get(2).copied().unwrap_or(0)
    }

    /// Whether this carries a recognised pre-release tag (`rc1`, `-beta.2`...).
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }

    /// Build metadata / unrecognised suffix (`a0bfb8370`, `build.5`), if any.
    pub fn build(&self) -> Option<&str> {
        self.build.as_deref()
    }

    /// `self >= min`, where `min` is a version string. Errors only if `min`
    /// doesn't parse.
    pub fn at_least(&self, min: &str) -> Result<bool, ParseError> {
        Ok(*self >= Version::parse(min)?)
    }
}

/// Parse a version at the start of `s`, returning it and the unparsed rest.
fn parse_prefix(s: &str) -> Result<(Version, &str), ParseError> {
    let b = s.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'v' | b'V')) && b.get(1).is_some_and(u8::is_ascii_digit) {
        i = 1;
    }
    if i >= b.len() {
        return Err(ParseError::Empty);
    }
    if !b[i].is_ascii_digit() {
        return Err(ParseError::NoNumber);
    }

    let mut release = Nums::new();
    loop {
        let mut n: u64 = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(b[i] - b'0')))
                .ok_or(ParseError::Overflow)?;
            i += 1;
        }
        release.push(n);
        // Another component only if `.` is followed by a digit; otherwise the
        // dot belongs to whatever follows (e.g. a sentence-ending period).
        if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
            i += 1;
        } else {
            break;
        }
    }

    let mut v = Version {
        release,
        pre: None,
        build: None,
    };
    let rest = &s[i..];
    if rest.is_empty() {
        return Ok((v, rest)); // the common case: just numbers
    }
    let token_end = rest
        .bytes()
        .position(|c| c.is_ascii_whitespace())
        .unwrap_or(rest.len());
    let (head, after) = rest.split_at(token_end);

    // `head` is `[sep]tail[+build]`. The tail is a pre-release tag
    // (`rc1`, `-beta.2`), else unrecognised build info (`-a0bfb8370`).
    let (tail, plus) = match head.split_once('+') {
        Some((t, p)) => (t, Some(p)),
        None => (head, None),
    };
    let tail = strip_sep(tail);
    let mut build: Option<Box<str>> = None;
    if !tail.is_empty() {
        if let Some(p) = parse_pre(tail) {
            v.pre = Some(p);
        } else if let Some((first, more)) = split_first_sep(tail).filter(|(f, _)| !f.is_empty()) {
            // "rc1-a0bfb8370": a tag followed by build info.
            match parse_pre(first) {
                Some(p) if is_tail_char_run(more) => {
                    v.pre = Some(p);
                    add_build(&mut build, more);
                }
                _ if is_tail_char_run(tail) => add_build(&mut build, tail),
                _ => return Ok((v, rest)),
            }
        } else if is_tail_char_run(tail) {
            add_build(&mut build, tail);
        } else {
            return Ok((v, rest)); // junk: the version ends at the numbers
        }
    }
    if let Some(p) = plus.filter(|p| !p.is_empty()) {
        // Later `+`s (from a previous Display of combined build info) are fine.
        if !p.split('+').all(is_tail_char_run) {
            return Ok((v, rest));
        }
        add_build(&mut build, p);
    }
    v.build = build;
    Ok((v, after))
}

/// Drop one leading separator (`-`, `_`, `~`, `.`).
fn strip_sep(s: &str) -> &str {
    match s.as_bytes().first() {
        Some(b'-' | b'_' | b'~' | b'.') => &s[1..],
        _ => s,
    }
}

/// Split at the first `-` or `_`.
fn split_first_sep(s: &str) -> Option<(&str, &str)> {
    let i = s.bytes().position(|c| c == b'-' || c == b'_')?;
    Some((&s[..i], &s[i + 1..]))
}

/// Append a build-info part, `+`-separated from any earlier one.
fn add_build(build: &mut Option<Box<str>>, part: &str) {
    *build = Some(match build.take() {
        None => part.into(),
        Some(old) => format!("{old}+{part}").into(),
    });
}

/// Characters that may appear in a build/hash suffix.
fn is_tail_char_run(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b'~'))
}

/// Recognise `tag[sep]digits[.digits]*` (or a bare tag) as a pre-release.
/// The *whole* text must match, so a git hash like `a0bfb8370` -- which starts
/// like `a0` -- is not mistaken for "alpha 0".
fn parse_pre(s: &str) -> Option<Box<Pre>> {
    let b = s.as_bytes();
    let word_end = b
        .iter()
        .position(|c| !c.is_ascii_alphabetic())
        .unwrap_or(b.len());
    let word = &s[..word_end];
    // Lower-case into a stack buffer: no allocation, and no tag is longer.
    let mut lower = [0u8; 8];
    let lower = lower.get_mut(..word.len())?;
    lower.copy_from_slice(word.as_bytes());
    lower.make_ascii_lowercase();
    let stage = match &*lower {
        b"dev" | b"snapshot" | b"nightly" => Stage::Dev,
        b"alpha" | b"a" => Stage::Alpha,
        b"beta" | b"b" => Stage::Beta,
        b"pre" | b"preview" => Stage::Pre,
        b"rc" | b"c" => Stage::Rc,
        _ => return None,
    };
    // `a`/`b`/`c` alone are also how git hashes start (`b1234567`). Only
    // believe them as tags when the number is short, like `b2` or `a10`.
    if word.len() == 1 && s.len() - word_end > 3 {
        return None;
    }
    let mut num = Nums::new();
    let mut rest = &s[word_end..];
    while !rest.is_empty() {
        if matches!(rest.as_bytes().first(), Some(b'.' | b'-' | b'_')) {
            rest = &rest[1..];
        }
        let end = rest
            .bytes()
            .position(|c| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 {
            return None; // letters/junk after the number: not a pure tag
        }
        num.push(rest[..end].parse::<u64>().ok()?);
        rest = &rest[end..];
    }
    Some(Box::new(Pre { stage, num }))
}

fn trim_zeros(v: &[u64]) -> &[u64] {
    let n = v.iter().rposition(|&x| x != 0).map_or(0, |p| p + 1);
    &v[..n]
}

fn cmp_nums(a: &[u64], b: &[u64]) -> Ordering {
    // Equal after dropping trailing zeros == equal with implicit zero padding.
    trim_zeros(a).cmp(trim_zeros(b))
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_nums(self.release.as_slice(), other.release.as_slice()).then_with(|| {
            match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => a
                    .stage
                    .cmp(&b.stage)
                    .then_with(|| cmp_nums(a.num.as_slice(), b.num.as_slice())),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        trim_zeros(self.release.as_slice()).hash(state);
        if let Some(p) = &self.pre {
            p.stage.hash(state);
            trim_zeros(p.num.as_slice()).hash(state);
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, n) in self.release.as_slice().iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{n}")?;
        }
        if let Some(p) = &self.pre {
            f.write_str("-")?;
            f.write_str(p.stage.name())?;
            for n in p.num.as_slice() {
                write!(f, ".{n}")?;
            }
        }
        if let Some(b) = &self.build {
            write!(f, "+{b}")?;
        }
        Ok(())
    }
}

impl FromStr for Version {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, ParseError> {
        Version::parse(s)
    }
}

/// Compare two version strings.
pub fn compare(a: &str, b: &str) -> Result<Ordering, ParseError> {
    Ok(Version::parse(a)?.cmp(&Version::parse(b)?))
}
