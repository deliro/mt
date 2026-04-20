use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;
use eframe::egui::ColorImage;
use image::ImageError;
use lru::LruCache;
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{Semaphore, mpsc};
use tracing::{debug, error, warn};
use walkers::sources::{Attribution, OpenStreetMap, TileSource};
use walkers::{Texture, TextureWithUv, TileId, Tiles};

/// Decoded GPU textures held in RAM. Each tile is 256×256 RGBA ≈ 256KB, so
/// 512 tiles ≈ 128MB resident. SQLite-cached bytes outside this LRU are still
/// available, just need a re-decode on cache miss.
const MEMORY_CAPACITY: usize = 512;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
/// Max concurrent HTTP fetches. OSM usage policy is fine with interactive
/// clients; 16 is what chromium-family browsers allow per origin for HTTP/1.1.
/// With avg fetch ~120ms this gives ~130 tiles/sec of headroom — enough that
/// the queue drains faster than walkers generates requests during zooms.
const MAX_PARALLEL: usize = 16;
/// Upper bound on queued-but-not-yet-fetched requests. Kept large
/// enough that normal panning / zooming won't hit it; when it does,
/// the least-recently-queued entry is dropped.
const MAX_PENDING: usize = 1024;
/// Guard against `ancestor_pending` growing unbounded if the user pans
/// through huge uncovered areas. A full clear is cheap and just means a
/// few redundant sqlite probes afterward (each is sub-millisecond).
const MAX_ANCESTOR_PENDING: usize = 8192;
/// Tiles whose zoom level is further than this from the current view are
/// dropped from the HTTP queue. Walkers has already stopped rendering
/// them; fetching them now just burns permits that belong to the zoom
/// the user is actually looking at.
const STALE_ZOOM_DIFF: u8 = 2;
/// Retry schedule for tiles whose HTTP fetch failed. Tuned for a doomsday
/// app on flaky 2G/3G: fast initial retry catches a brief blip, caps at
/// 30s so a persistent outage keeps checking at a sane cadence without
/// spam. No one sits through multi-minute back-offs — by the time they'd
/// fire the user has moved on.
fn backoff_for(attempt: u32) -> Duration {
    match attempt {
        0 => Duration::from_secs(3),
        1 => Duration::from_secs(10),
        _ => Duration::from_secs(30),
    }
}
/// Upper bound on `fail_backoff` — clears entirely when exceeded. On a
/// clear we forget about past failures and retry immediately, which is
/// the right move if the user's moved to a different network.
const MAX_FAIL_BACKOFF: usize = 4096;
const UV_UNIT: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
/// OSM serves tiles up to zoom 19. Walkers' internal zoom cap is 26,
/// so requests at zoom 20+ reach us and must be dropped before we try
/// to fetch them.
pub const OSM_MAX_ZOOM: u8 = 19;

/// Message the UI sends to the worker.
#[derive(Clone, Copy)]
pub enum TileRequest {
    /// Normal fetch: try sqlite, fall back to HTTP.
    Target(TileId),
    /// Zoom-blur prefetch: the UI's RAM-only `ancestor_crop` found no
    /// fallback for this tile, so ask the worker to walk the pyramid in
    /// sqlite and deliver the first ancestor it finds cached on disk.
    /// Never triggers HTTP — silent no-op if nothing's cached.
    AncestorProbe(TileId),
}

/// Non-delivery outcomes reported by the worker back to the UI.
/// Deliveries themselves go through the separate `tile_tx` channel.
#[derive(Clone, Copy)]
pub enum TileEvent {
    /// HTTP fetch (or decode) failed. UI should schedule a back-off retry.
    Failed(TileId),
    /// Worker dropped the request from its queue without attempting it
    /// (either because the zoom was stale or the queue overflowed). UI
    /// should clear its in-flight marker so a future `at()` can re-queue.
    Pruned(TileId),
}

/// UI-side bookkeeping for a tile whose previous HTTP attempt failed.
#[derive(Clone, Copy)]
struct FailState {
    retry_at: Instant,
    attempt: u32,
}

/// `walkers::Tiles` implementation that caches raw tile bytes in the
/// application's `SQLite` DB (same file as the rest of `HistoryStore`).
///
/// A background thread owns its own `Connection` and an HTTP client;
/// the UI thread only talks to it over mpsc channels.
pub struct SqliteTiles {
    attribution: Attribution,
    tile_size: u32,
    /// Decoded textures only. Keeping placeholders out of this LRU matters:
    /// otherwise in-flight `None` entries and tombstones steal slots from
    /// real textures (incl. ancestor crops the zoom-blur fallback relies on).
    memory: LruCache<TileId, Texture>,
    /// Tiles we've asked the worker to fetch. Value is the send timestamp,
    /// purely for e2e-latency logging at delivery time. Entries are cleared
    /// on delivery (`tile_rx`), on failure (`event_rx: Failed`), or on
    /// worker-side pruning (`event_rx: Pruned`). The worker is always the
    /// authoritative voice — there's no UI-side timeout here.
    pending: HashMap<TileId, Instant>,
    /// Tiles whose last fetch failed. Holds the schedule for the next
    /// retry; `queue_target` refuses to re-send until `retry_at` passes.
    fail_backoff: HashMap<TileId, FailState>,
    /// Tiles for which we've already kicked off an `AncestorProbe` — dedup
    /// so we don't re-walk the sqlite pyramid every frame for the same
    /// target while the probe result is in flight.
    ancestor_pending: HashSet<TileId>,
    request_tx: mpsc::UnboundedSender<TileRequest>,
    tile_rx: mpsc::UnboundedReceiver<(TileId, ColorImage)>,
    event_rx: mpsc::UnboundedReceiver<TileEvent>,
    ctx: egui::Context,
}

impl SqliteTiles {
    pub fn open(db_path: Option<PathBuf>, ctx: egui::Context) -> Self {
        let source = OpenStreetMap;
        let attribution = source.attribution();
        let tile_size = source.tile_size();

        let (request_tx, request_rx) = mpsc::unbounded_channel::<TileRequest>();
        let (tile_tx, tile_rx) = mpsc::unbounded_channel::<(TileId, ColorImage)>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<TileEvent>();
        let ctx_thread = ctx.clone();

        let spawn = thread::Builder::new()
            .name("map-tiles".into())
            .spawn(move || run_loop(db_path, request_rx, tile_tx, event_tx, ctx_thread));
        if let Err(e) = spawn {
            error!(%e, "map-tiles: failed to spawn worker — tiles will not load");
        }

        let cap = NonZeroUsize::new(MEMORY_CAPACITY).unwrap_or(NonZeroUsize::MIN);
        Self {
            attribution,
            tile_size,
            memory: LruCache::new(cap),
            pending: HashMap::new(),
            fail_backoff: HashMap::new(),
            ancestor_pending: HashSet::new(),
            request_tx,
            tile_rx,
            event_rx,
            ctx,
        }
    }

    /// Drop the in-memory texture cache so the next frame re-requests
    /// tiles — used after the user clears the `SQLite` cache.
    pub fn purge_memory(&mut self) {
        self.memory.clear();
        self.pending.clear();
        self.fail_backoff.clear();
        self.ancestor_pending.clear();
    }
}

impl Tiles for SqliteTiles {
    fn at(&mut self, tile_id: TileId) -> Option<TextureWithUv> {
        while let Ok((id, image)) = self.tile_rx.try_recv() {
            let texture = Texture::from_color_image(image, &self.ctx);
            // Success resets any prior failure state — next hiccup starts
            // the back-off schedule fresh from the shortest delay.
            self.fail_backoff.remove(&id);
            if let Some(started) = self.pending.remove(&id) {
                debug!(
                    tile_id = ?id,
                    e2e_ms = started.elapsed().as_millis() as u64,
                    "tile: delivered to UI"
                );
            } else {
                // Arrived without a UI-side pending entry — either an
                // ancestor pulled in by `AncestorProbe` (target ≠ ancestor)
                // or a replay after `purge_memory`. Still worth noting.
                debug!(tile_id = ?id, "tile: delivered to UI (no start timestamp)");
            }
            let _ = self.memory.put(id, texture);
        }

        // Drain worker-reported non-delivery outcomes.
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                TileEvent::Failed(id) => {
                    self.pending.remove(&id);
                    let prev_attempt = self.fail_backoff.get(&id).map_or(0, |s| s.attempt);
                    let attempt = prev_attempt.saturating_add(1);
                    let backoff = backoff_for(prev_attempt);
                    let retry_at = Instant::now().checked_add(backoff).unwrap_or_else(Instant::now);
                    self.fail_backoff.insert(id, FailState { retry_at, attempt });
                    if self.fail_backoff.len() > MAX_FAIL_BACKOFF {
                        // Blow the whole table — on a fresh network the
                        // old failure state is stale anyway, and retrying
                        // immediately is the right move.
                        self.fail_backoff.clear();
                    }
                    debug!(
                        tile_id = ?id,
                        attempt,
                        backoff_ms = backoff.as_millis() as u64,
                        "tile: fetch failed, backing off"
                    );
                }
                TileEvent::Pruned(id) => {
                    self.pending.remove(&id);
                    debug!(tile_id = ?id, "tile: pruned by worker");
                }
            }
        }

        // Walkers' flood-fill keeps expanding past the world edge without
        // wrapping, so x = 2^zoom (and similar) land here. Cheap to
        // re-check per frame, so we just refuse instead of caching a
        // tombstone.
        if tile_id.zoom > OSM_MAX_ZOOM || !tile_within_world(tile_id) {
            return None;
        }

        let result = if let Some(texture) = self.memory.get(&tile_id) {
            Some(TextureWithUv { texture: texture.clone(), uv: UV_UNIT })
        } else {
            self.queue_target(tile_id, "primary");
            let crop = ancestor_crop(&mut self.memory, tile_id);
            if crop.is_none() {
                self.spawn_ancestor_probe(tile_id);
            }
            crop
        };

        // Keep the parent (z-1) warm in the LRU so a future zoom-out has an
        // ancestor to crop from instantly. Without this, `ancestor_crop`
        // only walks up through levels the user actually visited — zooming
        // out for the first time would fall off the end of the pyramid and
        // leave a black slot until the target itself arrives from sqlite.
        self.warm_parent(tile_id);

        result
    }

    fn attribution(&self) -> Attribution {
        self.attribution.clone()
    }

    fn tile_size(&self) -> u32 {
        self.tile_size
    }
}

impl SqliteTiles {
    fn spawn_ancestor_probe(&mut self, tile_id: TileId) {
        if self.ancestor_pending.len() >= MAX_ANCESTOR_PENDING {
            self.ancestor_pending.clear();
        }
        if self.ancestor_pending.insert(tile_id) {
            debug!(target = ?tile_id, "probe: request sent");
            let _ = self.request_tx.send(TileRequest::AncestorProbe(tile_id));
        }
    }

    fn warm_parent(&mut self, tile_id: TileId) {
        if tile_id.zoom == 0 {
            return;
        }
        let parent =
            TileId { x: tile_id.x >> 1, y: tile_id.y >> 1, zoom: tile_id.zoom.saturating_sub(1) };
        // `contains` is non-promoting — we don't want this probe to poke
        // LRU order of the parent we'd otherwise return as an ancestor
        // crop; promotion should only happen when the parent is actually
        // rendered.
        if self.memory.contains(&parent) {
            return;
        }
        self.queue_target(parent, "warm");
    }

    /// Send a `Target` request through the dedup gate. Refuses to send if
    /// a prior attempt is still in `fail_backoff` (retry window hasn't
    /// elapsed) or if a request is already in flight. The worker's
    /// explicit `Delivered` / `Failed` / `Pruned` signals are what drive
    /// re-entry into this function — no UI-side timeout needed.
    fn queue_target(&mut self, tile_id: TileId, reason: &'static str) {
        let now = Instant::now();
        if let Some(state) = self.fail_backoff.get(&tile_id)
            && state.retry_at > now
        {
            return;
        }
        if self.pending.contains_key(&tile_id) {
            return;
        }
        let attempt = self.fail_backoff.get(&tile_id).map_or(0, |s| s.attempt);
        self.pending.insert(tile_id, now);
        debug!(?tile_id, reason, attempt, "target: request sent");
        let _ = self.request_tx.send(TileRequest::Target(tile_id));
    }
}

// —— background worker ——

fn run_loop(
    db_path: Option<PathBuf>,
    mut request_rx: mpsc::UnboundedReceiver<TileRequest>,
    tile_tx: mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: mpsc::UnboundedSender<TileEvent>,
    ctx: egui::Context,
) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            error!(%e, "map-tiles: tokio runtime build failed");
            return;
        }
    };
    let client =
        match reqwest::Client::builder().user_agent(USER_AGENT).timeout(HTTP_TIMEOUT).build() {
            Ok(c) => c,
            Err(e) => {
                error!(%e, "map-tiles: HTTP client build failed");
                return;
            }
        };
    let conn = db_path.and_then(|path| match Connection::open(&path) {
        Ok(c) => Some(c),
        Err(e) => {
            warn!(%e, ?path, "map-tiles: SQLite open failed — running without cache");
            None
        }
    });

    rt.block_on(async move {
        let (http_tx, mut http_rx) = mpsc::unbounded_channel::<(TileId, Vec<u8>)>();
        let sem = Arc::new(Semaphore::new(MAX_PARALLEL));
        let mut pending: VecDeque<TileId> = VecDeque::new();
        let mut observed_zoom: u8 = 0;

        loop {
            drain_pending(&sem, &mut pending, observed_zoom, &client, &http_tx, &event_tx);
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(req) = req else { break };
                    if !handle_request(
                        req, conn.as_ref(), &tile_tx, &event_tx, &ctx,
                        &mut pending, &mut observed_zoom,
                    ) {
                        break;
                    }
                }
                Some((tile_id, bytes)) = http_rx.recv() => {
                    if !handle_fetched(
                        tile_id, bytes, conn.as_ref(), &tile_tx, &event_tx, &ctx,
                    ) {
                        break;
                    }
                }
                else => break,
            }
        }
    });
}

/// Spawn as many HTTP fetches as the semaphore allows, picking the
/// pending request whose zoom is closest to the user's latest observed
/// zoom. Far-zoom (stale) entries stay in the queue and are served
/// only after the relevant ones drain.
fn drain_pending(
    sem: &Arc<Semaphore>,
    pending: &mut VecDeque<TileId>,
    observed_zoom: u8,
    client: &reqwest::Client,
    http_tx: &mpsc::UnboundedSender<(TileId, Vec<u8>)>,
    event_tx: &mpsc::UnboundedSender<TileEvent>,
) {
    // Drop queue entries that are too far from the current view. During a
    // rapid zoom sequence walkers fires a request for every intermediate
    // level; once the zoom settles the intermediate tiles are no longer
    // rendered and fetching them only steals HTTP permits from the tiles
    // the user is actually looking at. Each dropped tile is reported to
    // the UI as `Pruned` so it can clear its in-flight marker.
    let before = pending.len();
    pending.retain(|t| {
        if t.zoom.abs_diff(observed_zoom) <= STALE_ZOOM_DIFF {
            true
        } else {
            let _ = event_tx.send(TileEvent::Pruned(*t));
            false
        }
    });
    let dropped = before.saturating_sub(pending.len());
    if dropped > 0 {
        debug!(dropped, observed_zoom, remaining = pending.len(), "queue: pruned stale requests");
    }

    while !pending.is_empty() {
        let Ok(permit) = Arc::clone(sem).try_acquire_owned() else { break };
        let Some(idx) = pick_priority(pending, observed_zoom) else {
            drop(permit);
            break;
        };
        let Some(tile_id) = pending.remove(idx) else {
            drop(permit);
            break;
        };
        let client = client.clone();
        let http_tx = http_tx.clone();
        let event_tx = event_tx.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let fetch_start = Instant::now();
            match fetch(&client, tile_id).await {
                Ok(bytes) => {
                    debug!(
                        ?tile_id,
                        fetch_ms = fetch_start.elapsed().as_millis() as u64,
                        bytes = bytes.len(),
                        "target: HTTP fetched"
                    );
                    let _ = http_tx.send((tile_id, bytes));
                }
                Err(e) => {
                    warn!(
                        ?tile_id,
                        fetch_ms = fetch_start.elapsed().as_millis() as u64,
                        %e,
                        "tile fetch failed"
                    );
                    let _ = event_tx.send(TileEvent::Failed(tile_id));
                }
            }
        });
    }
}

/// Returns `false` if the UI end of `tile_tx` closed (time to shut
/// down the worker).
fn handle_request(
    req: TileRequest,
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: &mpsc::UnboundedSender<TileEvent>,
    ctx: &egui::Context,
    pending: &mut VecDeque<TileId>,
    observed_zoom: &mut u8,
) -> bool {
    match req {
        TileRequest::Target(tile_id) => {
            *observed_zoom = tile_id.zoom;
            handle_target(tile_id, conn, tile_tx, event_tx, ctx, pending)
        }
        TileRequest::AncestorProbe(target) => {
            handle_ancestor_probe(target, conn, tile_tx, event_tx, ctx)
        }
    }
}

fn handle_target(
    tile_id: TileId,
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: &mpsc::UnboundedSender<TileEvent>,
    ctx: &egui::Context,
    pending: &mut VecDeque<TileId>,
) -> bool {
    let select_start = Instant::now();
    let hit = conn.and_then(|c| match load_bytes(c, tile_id) {
        Ok(opt) => opt,
        Err(e) => {
            warn!(?tile_id, %e, "tile select failed");
            None
        }
    });
    let load_us = select_start.elapsed().as_micros() as u64;

    if let Some(bytes) = hit {
        debug!(?tile_id, load_us, bytes = bytes.len(), "target: sqlite hit");
        deliver_tile(tile_id, bytes, tile_tx.clone(), event_tx.clone(), ctx.clone());
        return true;
    }
    debug!(?tile_id, load_us, "target: sqlite miss, queuing HTTP");
    pending.push_back(tile_id);
    while pending.len() > MAX_PENDING
        && let Some(evicted) = pending.pop_front()
    {
        // Queue overflow — tell the UI so it clears its in-flight marker
        // and can re-queue on demand instead of silently timing out.
        let _ = event_tx.send(TileEvent::Pruned(evicted));
    }
    true
}

/// Walk the pyramid upward from `target` and deliver the first ancestor
/// found cached in sqlite. Loop is bounded by `target.zoom` (≤19), each
/// iteration does one indexed SELECT; typical cost is sub-millisecond.
/// Never issues HTTP — if nothing's cached, silently gives up and the UI
/// sees a black slot until the target itself arrives.
fn handle_ancestor_probe(
    target: TileId,
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: &mpsc::UnboundedSender<TileEvent>,
    ctx: &egui::Context,
) -> bool {
    let Some(conn) = conn else { return true };
    let probe_start = Instant::now();
    for k in 1..=target.zoom {
        let shift = u32::from(k);
        let ancestor = TileId {
            x: target.x >> shift,
            y: target.y >> shift,
            zoom: target.zoom.saturating_sub(k),
        };
        match load_bytes(conn, ancestor) {
            Ok(Some(bytes)) => {
                debug!(
                    ?target,
                    ?ancestor,
                    levels_walked = k,
                    probe_us = probe_start.elapsed().as_micros() as u64,
                    bytes = bytes.len(),
                    "probe: sqlite hit"
                );
                deliver_tile(ancestor, bytes, tile_tx.clone(), event_tx.clone(), ctx.clone());
                return true;
            }
            Ok(None) => {}
            Err(e) => {
                warn!(?ancestor, %e, "probe: select failed");
                return true;
            }
        }
    }
    debug!(
        ?target,
        levels_walked = target.zoom,
        probe_us = probe_start.elapsed().as_micros() as u64,
        "probe: exhausted (nothing cached above this tile)"
    );
    true
}

fn handle_fetched(
    tile_id: TileId,
    bytes: Vec<u8>,
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: &mpsc::UnboundedSender<TileEvent>,
    ctx: &egui::Context,
) -> bool {
    if let Some(c) = conn {
        let save_start = Instant::now();
        match save_bytes(c, tile_id, &bytes) {
            Ok(()) => debug!(
                ?tile_id,
                save_us = save_start.elapsed().as_micros() as u64,
                bytes = bytes.len(),
                "target: sqlite saved"
            ),
            Err(e) => warn!(?tile_id, %e, "tile insert failed"),
        }
    }
    deliver_tile(tile_id, bytes, tile_tx.clone(), event_tx.clone(), ctx.clone());
    true
}

/// Fire-and-forget decode: spawn on tokio's blocking pool so the worker
/// loop can drain the next message instead of blocking ~1ms per tile on
/// PNG inflate. For a 70-tile burst (typical zoom transition) this turns
/// a ~100ms serial chain into a ~20-30ms parallel pipeline on the cores
/// the blocking pool lands on.
fn deliver_tile(
    tile_id: TileId,
    bytes: Vec<u8>,
    tile_tx: mpsc::UnboundedSender<(TileId, ColorImage)>,
    event_tx: mpsc::UnboundedSender<TileEvent>,
    ctx: egui::Context,
) {
    tokio::task::spawn_blocking(move || {
        let decode_start = Instant::now();
        match decode(&bytes) {
            Ok(image) => {
                debug!(
                    ?tile_id,
                    decode_us = decode_start.elapsed().as_micros() as u64,
                    bytes = bytes.len(),
                    "tile: decoded"
                );
                let _ = tile_tx.send((tile_id, image));
                ctx.request_repaint();
            }
            Err(e) => {
                // Corrupt bytes in sqlite or a bad OSM response. Signal the
                // UI so it applies back-off rather than spinning forever on
                // repeat requests for the same broken tile.
                warn!(?tile_id, %e, "tile decode failed");
                let _ = event_tx.send(TileEvent::Failed(tile_id));
            }
        }
    });
}

/// Pick the pending request whose zoom is closest to the user's
/// current zoom. Ties broken toward the most recently queued entry
/// so fresh requests surface over stale ones at the same level.
fn pick_priority(pending: &VecDeque<TileId>, observed_zoom: u8) -> Option<usize> {
    pending
        .iter()
        .enumerate()
        .min_by(|(ia, ta), (ib, tb)| {
            let da = ta.zoom.abs_diff(observed_zoom);
            let db = tb.zoom.abs_diff(observed_zoom);
            da.cmp(&db).then_with(|| ib.cmp(ia))
        })
        .map(|(i, _)| i)
}

async fn fetch(client: &reqwest::Client, tile_id: TileId) -> Result<Vec<u8>, TileError> {
    let url = OpenStreetMap.tile_url(tile_id);
    let resp = client.get(&url).send().await?.error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}

fn load_bytes(conn: &Connection, tile_id: TileId) -> Result<Option<Vec<u8>>, TileError> {
    let blob = conn
        .query_row(
            "SELECT bytes FROM map_tiles WHERE zoom = ? AND tile_x = ? AND tile_y = ?",
            params![i64::from(tile_id.zoom), i64::from(tile_id.x), i64::from(tile_id.y)],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    Ok(blob)
}

fn save_bytes(conn: &Connection, tile_id: TileId, bytes: &[u8]) -> Result<(), TileError> {
    let now_ms = i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default(),
    )
    .unwrap_or(i64::MAX);
    conn.execute(
        "INSERT OR REPLACE INTO map_tiles (zoom, tile_x, tile_y, bytes, fetched_at_ms)
         VALUES (?, ?, ?, ?, ?)",
        params![
            i64::from(tile_id.zoom),
            i64::from(tile_id.x),
            i64::from(tile_id.y),
            bytes,
            now_ms,
        ],
    )?;
    Ok(())
}

/// Valid tile-id ranges per XYZ scheme: `0 <= x, y < 2^zoom`.
fn tile_within_world(tile_id: TileId) -> bool {
    let Some(max) = 1_u32.checked_shl(u32::from(tile_id.zoom)) else { return false };
    tile_id.x < max && tile_id.y < max
}

/// While the target tile is in flight, pull the closest cached
/// ancestor out of the LRU and return it with a UV rect cropped to the
/// quadrant that corresponds to the target. Walkers will stretch that
/// crop over the target tile's screen slot — same "zoom blur" look
/// Google/Yandex maps use while detail tiles are loading.
fn ancestor_crop(memory: &mut LruCache<TileId, Texture>, tile_id: TileId) -> Option<TextureWithUv> {
    for k in 1..=tile_id.zoom {
        let shift = u32::from(k);
        let Some(grid) = 1_u32.checked_shl(shift) else { continue };
        let ancestor = TileId {
            x: tile_id.x >> shift,
            y: tile_id.y >> shift,
            zoom: tile_id.zoom.saturating_sub(k),
        };
        // Probe with `peek` so ancestor misses don't disturb LRU order —
        // otherwise every fallback probe would promote zoom-out levels and
        // bulldoze the current-zoom working set on the next put.
        if memory.peek(&ancestor).is_none() {
            continue;
        }
        // The ancestor we actually return *must* be promoted: this slot
        // is in active use as a fallback and would otherwise age out and
        // get evicted by the next incoming tile delivery, causing the
        // crop to disappear mid-zoom (the "black square flicker" bug).
        let texture = memory.get(&ancestor)?;
        let mask = grid.saturating_sub(1);
        let sub_x = (tile_id.x & mask) as f32;
        let sub_y = (tile_id.y & mask) as f32;
        let step = 1.0 / grid as f32;
        let uv = egui::Rect::from_min_max(
            egui::pos2(sub_x * step, sub_y * step),
            egui::pos2(step.mul_add(1.0, sub_x * step), step.mul_add(1.0, sub_y * step)),
        );
        return Some(TextureWithUv { texture: texture.clone(), uv });
    }
    None
}

fn decode(bytes: &[u8]) -> Result<ColorImage, TileError> {
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice()))
}

#[derive(Debug, thiserror::Error)]
enum TileError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Decode(#[from] ImageError),
}
