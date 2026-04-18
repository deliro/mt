# Releasing

Releases are driven by [release-please](https://github.com/googleapis/release-please).
The happy path is fully automated: merge feature/fix commits to `master`, and a
standing "Release" PR accumulates them; when you're ready to ship, merge that PR
and GitHub Actions does the rest.

## Commit message format (required)

Release-please reads [Conventional Commits](https://www.conventionalcommits.org/).
Every commit that should show up in the changelog must start with one of:

| Prefix      | Meaning                                   | Version bump       |
|-------------|-------------------------------------------|--------------------|
| `feat:`     | User-visible new feature                  | minor (0.x.0)      |
| `fix:`      | User-visible bug fix                      | patch (0.0.x)      |
| `perf:`     | Performance improvement                   | patch              |
| `refactor:` | Internal change, no behaviour difference  | none (shown)       |
| `docs:`     | Documentation only                        | none (shown)       |
| `build:`    | Build / dependency change                 | none (shown)       |
| `ci:`       | CI configuration change                   | none (shown)       |
| `chore:`    | Housekeeping                              | none (hidden)      |
| `test:`     | Tests only                                | none (hidden)      |

Breaking changes (major bump) go either as `feat!:` / `fix!:` or with a
`BREAKING CHANGE:` footer in the body. Pre-1.0 the project bumps minor
instead of major for breakings (`bump-minor-pre-major`).

## Automated flow

1. Merge conventional-commit PRs to `master`.
2. `release-please.yml` keeps a rolling "chore: release X.Y.Z" PR open with:
   - `Cargo.toml` version bump
   - `CHANGELOG.md` entry generated from the commits since the last release
3. When you merge that PR, release-please tags `vX.Y.Z` and creates a GitHub
   Release.
4. The tag push triggers `release.yml` which:
   - builds per-target binaries (aarch64/x86_64 macOS, x86_64 Linux, x86_64
     Windows)
   - `lipo`s the two macOS builds into a universal binary
   - attaches all archives to the release via `gh release upload --clobber`
5. Release goes live. Copy the CHANGELOG section into the release description
   if you want more context than the raw commit list.

## Refreshing Cargo.lock on the release PR

Release-please bumps `Cargo.toml` but **not** `Cargo.lock`. That triggers a CI
failure on the PR until the lock is refreshed. One of:

- Check out the PR branch locally, `cargo update --workspace --offline`, and
  push the lock file commit.
- Or: merge the PR; CI on `master` will fail; push a follow-up `chore: refresh
  Cargo.lock` commit. Cleaner is the first option.

## Manual builds (local, outside CI)

The automated workflow covers every platform. When you need to build a release
binary locally — e.g. for a hotfix before CI catches up, or for codesigning —
run the steps below. They mirror `.github/workflows/release.yml`.

### macOS universal

```sh
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p dist
lipo -create \
  target/aarch64-apple-darwin/release/mt \
  target/x86_64-apple-darwin/release/mt \
  -output dist/mt
tar -czf dist/mt-macos-universal.tar.gz -C dist mt
```

### Linux x86_64

Inside a Debian 11 / Ubuntu 22.04 container for broad glibc compat:

```sh
cargo build --release --target x86_64-unknown-linux-gnu
tar -czf dist/mt-x86_64-unknown-linux-gnu.tar.gz \
  -C target/x86_64-unknown-linux-gnu/release mt
```

### Windows x86_64

From Windows with MSVC build tools:

```sh
cargo build --release --target x86_64-pc-windows-msvc
powershell Compress-Archive \
  -Path target\x86_64-pc-windows-msvc\release\mt.exe \
  -DestinationPath dist\mt-x86_64-pc-windows-msvc.zip
```

## Pre-flight smoke test (against a real radio)

CI tests cover pure-Rust logic. Before merging the release PR, run through
this manually against hardware:

- Connect over BLE, stay connected ≥ 15 min — confirm the link watchdog does
  not false-trip.
- Send a DM and a broadcast. Confirm `○ queued → ◐ sent → ✓ acked` on the DM
  and `○ queued → ◐ sent` on the broadcast.
- Run a traceroute to a node 2+ hops away and confirm the reply renders.
- Channels tab: *Copy share URL*, paste into another client, confirm
  channels match. *Import from URL…* with a known-good URL, review, save.
- Admin section: *Reboot*; destructive-confirm fires. Device reboots,
  watchdog + probe detect the gap, session reconnects.
- Close + reopen the laptop lid on a BLE session. Expect a `Disconnected`
  within ~7 min; reconnect from the UI and confirm it works.

## Codesigning (macOS)

Unsigned macOS binaries get blocked by Gatekeeper on first launch. Two paths:

1. **Developer ID** ($99/yr Apple account):

   ```sh
   codesign --deep --force --options runtime \
     --sign "Developer ID Application: <Name> (<TEAMID>)" \
     dist/mt
   ditto -c -k --keepParent dist/mt dist/mt.zip
   xcrun notarytool submit dist/mt.zip \
     --apple-id <email> --team-id <TEAMID> \
     --password <app-specific-pw> --wait
   xcrun stapler staple dist/mt
   ```

2. **Unsigned ship**: tell users to right-click → *Open* on first launch, or
   `xattr -d com.apple.quarantine ./mt` from a terminal.

Windows Authenticode signing is recommended but optional — unsigned binaries
just get a SmartScreen warning the first time.

## Rollback

A bad release:

1. Don't delete the tag — that breaks links everywhere.
2. Mark the GitHub Release as pre-release with a "Known issue" note.
3. Land the fix on `master` with a `fix:` commit; release-please opens a
   patch PR; merge; done.

## Data migrations

If you change `profiles.toml` or the SQLite schema, bump minor and document
the migration in the release notes. The SQLite migrator in
`src/persist/history.rs` handles additive column changes via `PRAGMA
table_info` + `ALTER TABLE`; anything destructive needs explicit code and a
release note.
