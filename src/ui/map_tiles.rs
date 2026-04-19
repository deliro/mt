use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use eframe::egui::ColorImage;
use image::ImageError;
use lru::LruCache;
use rusqlite::{Connection, OptionalExtension, params};
use tracing::{error, warn};
use walkers::sources::{Attribution, OpenStreetMap, TileSource};
use walkers::{Texture, TextureWithUv, TileId, Tiles};

const MEMORY_CAPACITY: usize = 512;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const UV_UNIT: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
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
    request_tx: mpsc::Sender<TileId>,
    tile_rx: mpsc::Receiver<(TileId, ColorImage)>,
    ctx: egui::Context,
}

impl SqliteTiles {
    pub fn open(db_path: Option<PathBuf>, ctx: egui::Context) -> Self {
        let source = OpenStreetMap;
        let attribution = source.attribution();
        let tile_size = source.tile_size();

        let (request_tx, request_rx) = mpsc::channel::<TileId>();
        let (tile_tx, tile_rx) = mpsc::channel::<(TileId, ColorImage)>();
        let ctx_thread = ctx.clone();

        let spawn = thread::Builder::new()
            .name("map-tiles".into())
            .spawn(move || run_loop(db_path, request_rx, tile_tx, ctx_thread));
        if let Err(e) = spawn {
            error!(%e, "map-tiles: failed to spawn worker — tiles will not load");
        }

        let cap = NonZeroUsize::new(MEMORY_CAPACITY).unwrap_or(NonZeroUsize::MIN);
        Self {
            attribution,
            tile_size,
            memory: LruCache::new(cap),
            request_tx,
            tile_rx,
            ctx,
        }
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

        if tile_id.zoom > OSM_MAX_ZOOM {
            return None;
        }

        if let Some(slot) = self.memory.get(&tile_id) {
            return slot
                .as_ref()
                .map(|texture| TextureWithUv { texture: texture.clone(), uv: UV_UNIT });
        }

        let _ = self.memory.put(tile_id, None);
        let _ = self.request_tx.send(tile_id);
        None
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
    request_rx: mpsc::Receiver<TileId>,
    tile_tx: mpsc::Sender<(TileId, ColorImage)>,
    ctx: egui::Context,
) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            error!(%e, "map-tiles: tokio runtime build failed");
            return;
        }
    };
    let client = match reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()
    {
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
        while let Ok(tile_id) = request_rx.recv() {
            let bytes = {
                let hit = conn.as_ref().and_then(|c| match load_bytes(c, tile_id) {
                    Ok(opt) => opt,
                    Err(e) => {
                        warn!(?tile_id, %e, "tile select failed");
                        None
                    }
                });
                if let Some(bytes) = hit {
                    bytes
                } else {
                    match fetch(&client, tile_id).await {
                        Ok(bytes) => {
                            if let Some(c) = conn.as_ref()
                                && let Err(e) = save_bytes(c, tile_id, &bytes)
                            {
                                warn!(?tile_id, %e, "tile insert failed");
                            }
                            bytes
                        }
                        Err(e) => {
                            warn!(?tile_id, %e, "tile fetch failed");
                            continue;
                        }
                    }
                }
            };
            match decode(&bytes) {
                Ok(image) => {
                    if tile_tx.send((tile_id, image)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                Err(e) => warn!(?tile_id, %e, "tile decode failed"),
            }
        }
    });
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
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default(),
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
