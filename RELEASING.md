# Releasing

This document is the step-by-step for cutting and shipping a new binary
release. Assumes the working tree is clean and on `master`.

## 0. Prereqs (one-time)

Install these before your first release:

- **macOS:** Xcode command-line tools (`xcode-select --install`),
  `cargo install cargo-bundle` (only needed if you want a `.app` bundle
  instead of a bare binary).
- **Linux:** `pkg-config`, `libssl-dev`, `libdbus-1-dev`, `libudev-dev`,
  `libgtk-3-dev` (and the corresponding `-devel` on RPM distros).
- **Windows:** Rust toolchain with `x86_64-pc-windows-msvc` target + Visual
  Studio Build Tools.
- **All platforms:** `cargo install cross` if you want to cross-compile
  Linux binaries from macOS / Windows.

## 1. Cut the version

1. Bump `version = "X.Y.Z"` in `Cargo.toml`.
2. Update `CHANGELOG.md` (create if missing) with a dated section
   summarising what changed since the last tag. Mention:
   - user-visible features
   - bug fixes
   - protocol / firmware compatibility notes
   - any breaking changes to the on-disk profile / history formats
3. Commit:

   ```sh
   git commit -m "Release vX.Y.Z"
   ```

4. Tag:

   ```sh
   git tag -a vX.Y.Z -m "vX.Y.Z"
   ```

5. Push tag and branch:

   ```sh
   git push origin master --tags
   ```

## 2. Pre-flight checks

Run locally on every target platform before building the release binary.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Manual smoke test against a real radio (at minimum):

- Connect over BLE, stay connected for ≥ 15 min, verify the link watchdog
  does not false-trip.
- Send a DM and a broadcast message. Confirm `○ queued → ◐ sent → ✓ acked`
  on the DM and `○ queued → ◐ sent` on the broadcast.
- Run a traceroute to a node 2+ hops away and confirm the reply renders.
- Open the Channels tab, click *Copy share URL*, paste the URL into
  another client, confirm the channels match.
- Reboot the device via the Admin section (destructive-confirm modal
  should fire). Reconnect automatically after the device comes back up.
- Close and reopen the laptop lid once connected over BLE. Watchdog should
  emit `Disconnected` within ~7 min if the handle is stale — then
  reconnect from the UI and verify it works.

## 3. Build platform binaries

All release builds are produced from the tag commit.

### macOS (Apple Silicon + Intel)

```sh
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Fat binary
mkdir -p dist/macos
lipo -create \
  target/aarch64-apple-darwin/release/mt \
  target/x86_64-apple-darwin/release/mt \
  -output dist/macos/mt

# Tarball
tar -czf dist/mt-vX.Y.Z-macos-universal.tar.gz -C dist/macos mt
```

Optional `.app` bundle (requires `cargo-bundle` or manual `Info.plist`):

```sh
# One-off: fill out [package.metadata.bundle] in Cargo.toml first
cargo bundle --release --target aarch64-apple-darwin
```

For distribution outside your own Mac, the bundle must be **codesigned**
and **notarized** — otherwise Gatekeeper blocks it. This needs an Apple
Developer ID ($99/yr). The minimum flow:

```sh
codesign --deep --force --options runtime \
  --sign "Developer ID Application: <Your Name> (<TEAMID>)" \
  dist/macos/mt.app
xcrun notarytool submit dist/macos/mt.app.zip \
  --apple-id <your-apple-id> --team-id <TEAMID> \
  --password <app-specific-password> --wait
xcrun stapler staple dist/macos/mt.app
```

If you don't have a Developer ID, ship the bare unsigned binary — users
will need to right-click → *Open* on first launch, or `xattr -d
com.apple.quarantine mt` from the terminal.

### Linux (x86_64)

```sh
cargo build --release --target x86_64-unknown-linux-gnu
mkdir -p dist/linux-x64
cp target/x86_64-unknown-linux-gnu/release/mt dist/linux-x64/
tar -czf dist/mt-vX.Y.Z-linux-x64.tar.gz -C dist/linux-x64 mt
```

For broader glibc compatibility, build inside a container pinned to an
older distro (Debian 11 / Ubuntu 22.04) using `cross`:

```sh
cross build --release --target x86_64-unknown-linux-gnu
```

### Windows (x86_64)

From Windows:

```sh
cargo build --release --target x86_64-pc-windows-msvc
mkdir dist\windows-x64
copy target\x86_64-pc-windows-msvc\release\mt.exe dist\windows-x64\
```

Zip:

```sh
powershell Compress-Archive -Path dist\windows-x64\mt.exe `
  -DestinationPath dist\mt-vX.Y.Z-windows-x64.zip
```

Code-signing with a Windows Authenticode certificate is recommended but
optional; unsigned binaries just show a SmartScreen warning on first run.

## 4. Publish

1. Upload the three archives to your release hosting of choice (GitHub
   Releases, S3 bucket, etc.). Attach to the `vX.Y.Z` tag.
2. Copy the CHANGELOG entry into the release description.
3. Announce (optional): Meshtastic Discord `#off-topic`, your own channels.

## 5. Post-release

1. Bump `version` in `Cargo.toml` to `X.Y.(Z+1)-dev` (or next minor) on
   `master` so post-release commits are clearly flagged.
2. Open a new empty `## vX.Y.(Z+1) — unreleased` section in `CHANGELOG.md`.
3. Commit + push.

## Data migration notes

If you change the on-disk shape of `profiles.toml` or the SQLite schema,
bump the minor version and document the migration in the CHANGELOG. The
SQLite migrator in `src/persist/history.rs` handles additive column
changes automatically via `PRAGMA table_info` + `ALTER TABLE`; anything
destructive needs explicit code.

## Rollback

If a release ships a regression:

1. Tag the previous good commit as `vX.Y.(Z-1).hotfix` (or add a
   `.post1` suffix) — don't delete the bad tag, that breaks links.
2. Push a new release with the fix.
3. Update the CHANGELOG with a "Known issue" note on the bad version.
