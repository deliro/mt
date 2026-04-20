# Changelog

All notable user-visible changes are listed here. Dates are UTC.

## [0.2.1](https://github.com/deliro/mt/compare/v0.2.0...v0.2.1) (2026-04-20)


### Bug Fixes

* **topology:** restore zoom-blur fallback + harden tile pipeline ([a3a023e](https://github.com/deliro/mt/commit/a3a023ea86d91c2b11b8476c4498ec65242c18d6))


### CI

* **release-please:** dispatch release.yml after a release tag is cut ([b782622](https://github.com/deliro/mt/commit/b7826221675346eee41161d2c5feadfabf9d8b31))
* **release:** allow publish job to run on workflow_dispatch ([92d7539](https://github.com/deliro/mt/commit/92d753943ad005a9cf3697aa12d0bb4535e356e2))
* **release:** fix homebrew tap empty-repo push, replace winget action ([ac01434](https://github.com/deliro/mt/commit/ac01434c527b4fed4862c28948efc761ab5ed9cb))
* **release:** linux AppImage + windows portable .exe + homebrew + winget ([def595c](https://github.com/deliro/mt/commit/def595c70e10fbcb42a1c98168b317ec786f04bc))
* **release:** pass --repo to gh release upload ([2fc0e08](https://github.com/deliro/mt/commit/2fc0e0898a668025cade835d0cd159e7c9f70ea5))
* **release:** rename AppImage by glob, not by hard-coded mt-* prefix ([bf5315c](https://github.com/deliro/mt/commit/bf5315c1c24553c2cd6ae363d63869f2c2b6f1ee))

## [0.2.0](https://github.com/deliro/mt/compare/v0.1.0...v0.2.0) (2026-04-19)


### Features

* **alerts:** client-side alert rules + OS notifications ([fe7bacb](https://github.com/deliro/mt/commit/fe7bacba35c336a9a17fbf1fdc27be8fe31fe42f))
* **chat:** searchable message list ([af7fe29](https://github.com/deliro/mt/commit/af7fe294b8dc0421e5a16fb30f6ff457b8865ac2))
* External Notification + Canned Message + Range Test (Phase B) ([b1a3f94](https://github.com/deliro/mt/commit/b1a3f9482eed8b00c4ff168bed2a207bca1f7120))
* **nodes:** click-to-sort headers persisted in SQLite ([4f7972a](https://github.com/deliro/mt/commit/4f7972aea77722793b0fd2ded38ef64af250ff93))
* **nodes:** per-node reliability scorecard on the detail popup ([0f7a443](https://github.com/deliro/mt/commit/0f7a443708ffb9bcc649d0eb22a4e78b837a0921))
* **persist:** consolidate profiles + last_active into sqlite ([876188f](https://github.com/deliro/mt/commit/876188fb618f0a8dc3c554b498ead43551edac5d))
* SecurityConfig + remote admin ([b24d36d](https://github.com/deliro/mt/commit/b24d36d04599e39ef222ca586d1878b38bb1711c))
* **settings:** config backup / restore as JSON ([99afa78](https://github.com/deliro/mt/commit/99afa780f34dff9983c04ab523510ca9978c6ba8))
* **topology:** back map tiles with SQLite, add clear+size in Settings ([c88e1a2](https://github.com/deliro/mt/commit/c88e1a20a27d7becf8b947cf5f415a5e4312f0a9))
* **topology:** collapse overlapping nodes into a clickable cluster ([85e20d7](https://github.com/deliro/mt/commit/85e20d7e54e790d742e9197f4ff91396c256eb88))
* **topology:** hop-count badge + size gradient on every node ([e1208ec](https://github.com/deliro/mt/commit/e1208ecbd459eaa83ee9038d9e404a2441aa472a))
* **topology:** mesh graph with signal and geographic views ([dea1b66](https://github.com/deliro/mt/commit/dea1b6698ace4362922f0e12f07f5fc8e786c276))
* **topology:** real OSM map tiles in the geographic view ([d2105a2](https://github.com/deliro/mt/commit/d2105a23eef2cfa5842b21aca17059d4f52c5122))
* **topology:** show cached-ancestor tiles while detail is loading ([1785bf4](https://github.com/deliro/mt/commit/1785bf4bb99448dc904ccc89b4101393d17276c4))
* **ui:** auto-reconnect on startup and after drops ([56e2d41](https://github.com/deliro/mt/commit/56e2d41f82771e51a861485fc68125e9c607823d))
* **ui:** firmware log viewer ([8436404](https://github.com/deliro/mt/commit/84364045498aaadaf9199b434917cb38672137a8))
* **ui:** firmware version banner below 2.5 ([11d320d](https://github.com/deliro/mt/commit/11d320da267ac2280b282b9cd31b02a8dfdc7448))
* **ui:** keyboard shortcuts (Cmd/Ctrl+K, Esc) ([1c47c56](https://github.com/deliro/mt/commit/1c47c560bdfa4b887a22627c760e48d1107f2930))
* **ui:** MQTT bridge indicator in the status bar ([d5de924](https://github.com/deliro/mt/commit/d5de924e5c886e0a9c98aab30e3ca925552a5e6c))
* **ui:** packet inspector tab ([e1799c1](https://github.com/deliro/mt/commit/e1799c18c95c7643894bbf66469fdba30f6fa7b5))


### Bug Fixes

* **config-export:** include fixed GPS + security policy ([b700eba](https://github.com/deliro/mt/commit/b700eba1f92449fca6afdc95045d15e605965ba5))
* **connect:** drop duplicate 'Connecting…' label from connect screen ([166c64c](https://github.com/deliro/mt/commit/166c64cc25d423ecea6d768aadd6288a769c5622))
* **nodes:** split count into online / idle / archived buckets ([7fc5589](https://github.com/deliro/mt/commit/7fc55892292004c4983333d2aeeecfa6e78ae0ec))
* **reconnect:** actually persist last-active profile key ([3008d42](https://github.com/deliro/mt/commit/3008d42dfa001f6c1a96a5d9d38f993601b977e1))
* **reconnect:** hide banner on first manual connect; add diagnostics ([9823de3](https://github.com/deliro/mt/commit/9823de3a3527ce69aaca6100bbb4ec794c75eab2))
* **scan:** auto-persist profile on Connect; clean up orphaned last_active ([251a465](https://github.com/deliro/mt/commit/251a46555f2c50503125f91a05d9b0868c8dfe5e))
* **topology:** clamp zoom to OSM's max so tile fetches don't 400 ([b6b9f08](https://github.com/deliro/mt/commit/b6b9f08eebdcbfeadaa39ac9588a12903d63ea68))
* **topology:** drop tile requests that sit outside the world ([863c715](https://github.com/deliro/mt/commit/863c715cff386bcf5ccf879c8fcab01af9806914))
* **topology:** fan out overlapping nodes in the geographic view ([a9d81f1](https://github.com/deliro/mt/commit/a9d81f1f00c0d16d0f6854663b5673bf91e519cd))
* **topology:** honest per-hop rings + zoom/pan in signal view ([5e5a2fd](https://github.com/deliro/mt/commit/5e5a2fd1d448e4dc5d36fbdd52daa2db47cf7c40))
* unknown senders, local timestamps, runaway chat scroll ([9e8cfe2](https://github.com/deliro/mt/commit/9e8cfe24a3aa1c50d77b9e9f0d352e0ea096c582))


### Performance

* **topology:** parallelize tile fetches (up to 8 concurrent) ([ade9b9a](https://github.com/deliro/mt/commit/ade9b9a5d1ce67c5cfa8aeca5e8d92ed06547412))
* **topology:** prioritise tile fetches closest to the current zoom ([5df3755](https://github.com/deliro/mt/commit/5df375500e4692d31beeccf5f2190a4a92cf2d63))
* **ui:** event-driven repaint, smaller tile cache, capped chat buffer ([36e525b](https://github.com/deliro/mt/commit/36e525bedc675cf84201fff885d3bd47021ee814))


### CI

* **deps:** bump actions/checkout from 4 to 6 ([#3](https://github.com/deliro/mt/issues/3)) ([e43ade5](https://github.com/deliro/mt/commit/e43ade55e7baa8290766492ee76fcef6d8258a45))
* **deps:** bump actions/download-artifact from 4 to 8 ([#1](https://github.com/deliro/mt/issues/1)) ([5e7ecff](https://github.com/deliro/mt/commit/5e7ecff6b9944b56f018e8815087b8c28176c54b))
* **deps:** bump actions/upload-artifact from 4 to 7 ([#2](https://github.com/deliro/mt/issues/2)) ([d0e35c1](https://github.com/deliro/mt/commit/d0e35c15961437d022837a39e7b2c34ee6094db4))
* GitHub Actions + release-please ([e8003d5](https://github.com/deliro/mt/commit/e8003d55463c7dd3315dd3c6eb1e24fdbd5221ea))

## v0.1.0 — unreleased

First tagged build. Feature complete for day-to-day Meshtastic use
against firmware 2.5+.

### Features

- BLE / USB-serial / TCP transports.
- Chat with per-channel broadcast, direct messages, delivery state
  (`queued / sent / acked / failed`) and ack-race / broadcast-Sent fixes.
- Node roster with live + cached split, favorites, ignore list, SNR /
  RSSI / battery, traceroute with per-hop SNR on forward and reverse
  paths.
- Full channel editing: 8 slots, roles, name, PSK (AES256 / AES128 /
  1-byte preset / clear), MQTT uplink-downlink, position precision,
  share URL import / export (`meshtastic.org/e/#` scheme).
- Admin actions with destructive-confirm modals: reboot, shutdown,
  reboot-to-OTA, factory reset (device / config-only), NodeDB reset.
- Module configs: MQTT, Telemetry, Neighbor Info, Store-and-Forward.
- Config sections: Owner, LoRa, Device, Position (with Fixed Position),
  Power, Network, Display, Bluetooth — every field has a tooltip.
- Local persistence: per-device SQLite of nodes and messages, survives
  restarts, clearable from the Settings tab.
- BLE suspend/resume recovery: receive-side watchdog + heartbeat
  metadata probe so a stale BLE handle after laptop wake is detected
  and surfaced as a disconnect within ~7 min.
- Bundled DejaVu Sans font for broad Unicode coverage in user-chosen
  node names.
