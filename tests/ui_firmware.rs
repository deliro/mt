#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mt::ui::firmware::{is_below_parity, parse_version};

#[test]
fn parses_typical_meshtastic_versions() {
    assert_eq!(parse_version("2.5.4.abcdef"), Some((2, 5)));
    assert_eq!(parse_version("2.4.0-beta"), Some((2, 4)));
    assert_eq!(parse_version("1.3.7"), Some((1, 3)));
    assert_eq!(parse_version("3.0"), Some((3, 0)));
    assert_eq!(parse_version("v2.5.4"), Some((2, 5)));
}

#[test]
fn parse_version_rejects_garbage() {
    assert_eq!(parse_version(""), None);
    assert_eq!(parse_version("unknown"), None);
    assert_eq!(parse_version("   "), None);
}

#[test]
fn below_parity_before_2_5() {
    assert!(is_below_parity("1.3.7"));
    assert!(is_below_parity("2.4.0"));
    assert!(is_below_parity("2.4.9"));
    assert!(!is_below_parity("2.5.0"));
    assert!(!is_below_parity("2.5.4.abcdef"));
    assert!(!is_below_parity("3.0.0"));
}

#[test]
fn unknown_version_is_not_below_parity() {
    // If we can't parse the string we don't nag the user — the banner is
    // advisory, not mandatory. Better to hide it than surface a misleading
    // warning on experimental builds.
    assert!(!is_below_parity("unreleased"));
    assert!(!is_below_parity(""));
}
