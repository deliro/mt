#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mt::domain::ids::BleAddress;
use mt::domain::profile::ConnectionProfile;
use mt::persist::profiles::{load_from, save_to};

#[test]
fn round_trip_profiles_to_toml() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let path = dir.path().join("profiles.toml");
    let input = vec![
        ConnectionProfile::Tcp { name: "home".into(), host: "192.168.1.1".into(), port: 4403 },
        ConnectionProfile::Ble { name: "pack".into(), address: BleAddress::new("AA:BB:CC:DD:EE:FF") },
    ];
    save_to(&path, &input, None).expect("save");
    let loaded = load_from(&path).expect("load");
    assert_eq!(loaded.profiles.len(), 2);
    assert!(matches!(loaded.profiles.first(), Some(ConnectionProfile::Tcp { .. })));
    assert!(matches!(loaded.profiles.get(1), Some(ConnectionProfile::Ble { .. })));
    assert!(loaded.last_active.is_none());
}

#[test]
fn round_trip_preserves_last_active() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let path = dir.path().join("profiles.toml");
    let input = vec![ConnectionProfile::Tcp {
        name: "home".into(),
        host: "192.168.1.1".into(),
        port: 4403,
    }];
    save_to(&path, &input, Some("tcp:192.168.1.1:4403")).expect("save");
    let loaded = load_from(&path).expect("load");
    assert_eq!(loaded.last_active.as_deref(), Some("tcp:192.168.1.1:4403"));
}

#[test]
fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let path = dir.path().join("nope.toml");
    let loaded = load_from(&path).expect("load");
    assert!(loaded.profiles.is_empty());
    assert!(loaded.last_active.is_none());
}
