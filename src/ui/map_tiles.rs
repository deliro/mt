use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use eframe::egui::ColorImage;
use image::ImageError;
use lru::LruCache;
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{Semaphore, mpsc};
use tracing::{error, warn};
use walkers::sources::{Attribution, OpenStreetMap, TileSource};
use walkers::{Texture, TextureWithUv, TileId, Tiles};

const MEMORY_CAPACITY: usize = 512;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
/// Max concurrent HTTP fetches. OSM usage policy discourages massive
/// parallelism from a single client; 8 is comfortable and matches what
/// modern browsers do per origin.
const MAX_PARALLEL: usize = 8;
/// Upper bound on queued-but-not-yet-fetched requests. Kept large
/// enough that normal panning / zooming won't hit it; when it does,
/// the least-recently-queued entry is dropped.
const MAX_PENDING: usize = 1024;
const UV_UNIT: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
/// OSM serves tiles up to zoom 19. Walkers' internal zoom cap is 26,
/// so requests at zoom 20+ reach us and must be dropped before we try
/// to fetch them.
pub const OSM_MAX_ZOOM: u8 = 19;

/// `walkers::Tiles` implementation that caches raw tile bytes in the
/// application's `SQLite` DB (same file as the rest of `HistoryStore`).
///
/// A background thread owns its own `Connection` and an HTTP client;
/// the UI thread only talks to it over mpsc channels.
pub struct SqliteTiles {
    attribution: Attribution,
    tile_size: u32,
    memory: LruCache<TileId, Option<Texture>>,
    request_tx: mpsc::UnboundedSender<TileId>,
    tile_rx: mpsc::UnboundedReceiver<(TileId, ColorImage)>,
    ctx: egui::Context,
}

impl SqliteTiles {
    pub fn open(db_path: Option<PathBuf>, ctx: egui::Context) -> Self {
        let source = OpenStreetMap;
        let attribution = source.attribution();
        let tile_size = source.tile_size();

        let (request_tx, request_rx) = mpsc::unbounded_channel::<TileId>();
        let (tile_tx, tile_rx) = mpsc::unbounded_channel::<(TileId, ColorImage)>();
        let ctx_thread = ctx.clone();

        let spawn = thread::Builder::new()
            .name("map-tiles".into())
            .spawn(move || run_loop(db_path, request_rx, tile_tx, ctx_thread));
        if let Err(e) = spawn {
            error!(%e, "map-tiles: failed to spawn worker — tiles will not load");
        }

        let cap = NonZeroUsize::new(MEMORY_CAPACITY).unwrap_or(NonZeroUsize::MIN);
        Self { attribution, tile_size, memory: LruCache::new(cap), request_tx, tile_rx, ctx }
    }

    /// Drop the in-memory texture cache so the next frame re-requests
    /// tiles — used after the user clears the `SQLite` cache.
    pub fn purge_memory(&mut self) {
        self.memory.clear();
    }
}

impl Tiles for SqliteTiles {
    fn at(&mut self, tile_id: TileId) -> Option<TextureWithUv> {
        while let Ok((id, image)) = self.tile_rx.try_recv() {
            let texture = Texture::from_color_image(image, &self.ctx);
            let _ = self.memory.put(id, Some(texture));
        }

        if tile_id.zoom > OSM_MAX_ZOOM || !tile_within_world(tile_id) {
            // Walkers' flood-fill keeps expanding east/south past the
            // world edge without wrapping, so x = 2^zoom (and similar)
            // land here. Mark as permanently unavailable so we don't
            // re-issue a doomed HTTP request next frame.
            let _ = self.memory.put(tile_id, None);
            return None;
        }

        match self.memory.get(&tile_id) {
            Some(Some(texture)) => {
                return Some(TextureWithUv { texture: texture.clone(), uv: UV_UNIT });
            }
            Some(None) => {
                // Fetch already in flight. Fall through to the
                // ancestor fallback so the tile slot isn't just
                // black while we wait.
            }
            None => {
                let _ = self.memory.put(tile_id, None);
                let _ = self.request_tx.send(tile_id);
            }
        }

        ancestor_crop(&self.memory, tile_id)
    }

    fn attribution(&self) -> Attribution {
        self.attribution.clone()
    }

    fn tile_size(&self) -> u32 {
        self.tile_size
    }
}

// —— background worker ——

fn run_loop(
    db_path: Option<PathBuf>,
    mut request_rx: mpsc::UnboundedReceiver<TileId>,
    tile_tx: mpsc::UnboundedSender<(TileId, ColorImage)>,
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
            drain_pending(&sem, &mut pending, observed_zoom, &client, &http_tx);
            tokio::select! {
                req = request_rx.recv() => {
                    let Some(tile_id) = req else { break };
                    observed_zoom = tile_id.zoom;
                    if !handle_request(
                        tile_id, conn.as_ref(), &tile_tx, &ctx, &mut pending,
                    ) {
                        break;
                    }
                }
                Some((tile_id, bytes)) = http_rx.recv() => {
                    if !handle_fetched(
                        tile_id, &bytes, conn.as_ref(), &tile_tx, &ctx,
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
) {
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
        tokio::spawn(async move {
            let _permit = permit;
            match fetch(&client, tile_id).await {
                Ok(bytes) => {
                    let _ = http_tx.send((tile_id, bytes));
                }
                Err(e) => warn!(?tile_id, %e, "tile fetch failed"),
            }
        });
    }
}

/// Returns `false` if the UI end of `tile_tx` closed (time to shut
/// down the worker).
fn handle_request(
    tile_id: TileId,
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    ctx: &egui::Context,
    pending: &mut VecDeque<TileId>,
) -> bool {
    let hit = conn.and_then(|c| match load_bytes(c, tile_id) {
        Ok(opt) => opt,
        Err(e) => {
            warn!(?tile_id, %e, "tile select failed");
            None
        }
    });
    if let Some(bytes) = hit {
        return deliver_tile(tile_id, &bytes, tile_tx, ctx);
    }
    pending.push_back(tile_id);
    while pending.len() > MAX_PENDING {
        let _ = pending.pop_front();
    }
    true
}

fn handle_fetched(
    tile_id: TileId,
    bytes: &[u8],
    conn: Option<&Connection>,
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    ctx: &egui::Context,
) -> bool {
    if let Some(c) = conn
        && let Err(e) = save_bytes(c, tile_id, bytes)
    {
        warn!(?tile_id, %e, "tile insert failed");
    }
    deliver_tile(tile_id, bytes, tile_tx, ctx)
}

fn deliver_tile(
    tile_id: TileId,
    bytes: &[u8],
    tile_tx: &mpsc::UnboundedSender<(TileId, ColorImage)>,
    ctx: &egui::Context,
) -> bool {
    match decode(bytes) {
        Ok(image) => {
            if tile_tx.send((tile_id, image)).is_err() {
                return false;
            }
            ctx.request_repaint();
            true
        }
        Err(e) => {
            warn!(?tile_id, %e, "tile decode failed");
            true
        }
    }
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
fn ancestor_crop(
    memory: &LruCache<TileId, Option<Texture>>,
    tile_id: TileId,
) -> Option<TextureWithUv> {
    for k in 1..=tile_id.zoom {
        let shift = u32::from(k);
        let Some(grid) = 1_u32.checked_shl(shift) else { continue };
        let ancestor = TileId {
            x: tile_id.x >> shift,
            y: tile_id.y >> shift,
            zoom: tile_id.zoom.saturating_sub(k),
        };
        let Some(Some(texture)) = memory.peek(&ancestor) else { continue };
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
