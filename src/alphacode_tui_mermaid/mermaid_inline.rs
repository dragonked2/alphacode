//! Inline raster image support, decoupled from the Mermaid diagram pipeline.
//!
//! Real images (pasted screenshots, `read` of an image file, generated images)
//! are fundamentally different from Mermaid diagrams: they arrive as base64
//! payloads, never need SVG layout/aspect buckets, and should be rendered
//! "fit to width" rather than cropped to an estimated height.
//!
//! This module provides a small, lazy API:
//!
//! * [`inline_image_dims`] - cheap, header-only dimensions (cached by id). Used
//!   at *prepare* time to compute placeholder height without decoding the whole
//!   image.
//! * [`materialize_inline_image`] - full decode + PNG-to-disk + insert into the
//!   shared render cache. Used at *draw* time, only for images currently on
//!   screen, so a session with many images only ever decodes the ones you look
//!   at.
//!
//! Both share a stable content id so the placeholder computed at prepare time
//! lines up with the bytes rendered at draw time. Rendering itself reuses the
//! existing [`crate::render_image_widget_fit`] path via the shared
//! `RENDER_CACHE` (keyed by id -> on-disk PNG path).

use super::*;

/// Cap on the dimension cache. Header parsing is cheap, but a long session can
/// accumulate many distinct images; bound the metadata map so it cannot grow
/// without limit.
const INLINE_DIMS_MAX: usize = 256;

#[derive(Clone, Copy)]
struct InlineDims {
    width: u32,
    height: u32,
}

/// Cache of `id -> (width, height)` plus an insertion-order queue used to
/// bound the map: `(by_id, eviction_order)`.
type InlineDimsCache = (HashMap<u64, InlineDims>, VecDeque<u64>);

/// Cache of `id -> (width, height)` so repeated prepare passes never re-parse
/// the same image header. Bounded by insertion order.
static INLINE_DIMS_CACHE: LazyLock<Mutex<InlineDimsCache>> =
    LazyLock::new(|| Mutex::new((HashMap::new(), VecDeque::new())));

/// Stable content id for an inline image, derived from its media type and
/// base64 payload. No decoding is performed.
pub fn inline_image_id(media_type: &str, data_b64: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    media_type.hash(&mut hasher);
    data_b64.as_bytes().hash(&mut hasher);
    hasher.finish()
}

fn dims_cache_get(id: u64) -> Option<InlineDims> {
    INLINE_DIMS_CACHE.lock().ok()?.0.get(&id).copied()
}

fn dims_cache_put(id: u64, dims: InlineDims) {
    if let Ok(mut guard) = INLINE_DIMS_CACHE.lock() {
        let (map, order) = &mut *guard;
        if map.insert(id, dims).is_none() {
            order.push_back(id);
            while order.len() > INLINE_DIMS_MAX {
                if let Some(old) = order.pop_front() {
                    map.remove(&old);
                }
            }
        }
    }
}

/// Cheap dimensions for an inline image: `(id, width, height)`.
///
/// Tries a header-only parse of a decoded prefix first (so a multi-megabyte
/// screenshot only touches its first few KB), and falls back to a full decode
/// only if the header could not be understood. Results are cached by id.
pub fn inline_image_dims(media_type: &str, data_b64: &str) -> Option<(u64, u32, u32)> {
    let id = inline_image_id(media_type, data_b64);
    if let Some(dims) = dims_cache_get(id) {
        return Some((id, dims.width, dims.height));
    }

    // Header-only fast path: decode just a prefix of the base64 payload.
    if let Some((w, h)) = dims_from_b64_prefix(data_b64) {
        let dims = InlineDims {
            width: w,
            height: h,
        };
        dims_cache_put(id, dims);
        return Some((id, w, h));
    }

    // Fallback: full decode (only happens once per image, then cached).
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let (w, h) = image.dimensions();
    let dims = InlineDims {
        width: w,
        height: h,
    };
    dims_cache_put(id, dims);
    Some((id, w, h))
}

/// Materialize an inline image for rendering: decode it, write a PNG-equivalent
/// file to the shared cache directory, and register it in `RENDER_CACHE` so the
/// existing `render_image_widget_*` paths can draw it by id.
///
/// Idempotent and cheap on repeat (returns the cached entry without re-decoding
/// once the file exists). Returns `(id, width, height)` on success.
pub fn materialize_inline_image(media_type: &str, data_b64: &str) -> Option<(u64, u32, u32)> {
    let id = inline_image_id(media_type, data_b64);
    materialize_inline_image_by_id(id, media_type, data_b64)
}

/// Cheap presence probe: is this inline image already registered in the
/// in-memory render cache? One mutex lock + bounded key scan; no payload
/// hashing, no payload clone, no filesystem access. Used by the per-frame
/// draw path so steady-state scrolling never touches the multi-megabyte
/// payload.
///
/// Matches ANY render profile for the hash, not just the default: mermaid
/// diagrams rendered toward an inline aspect goal are cached under an
/// aspect-tagged profile key, and a default-only probe would report them as
/// missing forever (the draw path would then spin on prewarms that succeed
/// without ever flipping this probe, leaving a permanently blank placeholder).
pub fn inline_image_is_materialized(id: u64) -> bool {
    RENDER_CACHE
        .lock()
        .map(|cache| cache.entries.keys().any(|(hash, _)| *hash == id))
        .unwrap_or(false)
}

/// [`materialize_inline_image`] for callers that already know the stable
/// content id, skipping the full-payload hash. Also primes the decoded-source
/// cache so the first fit/scale render does not decode the same bytes a second
/// time from disk.
pub fn materialize_inline_image_by_id(
    id: u64,
    media_type: &str,
    data_b64: &str,
) -> Option<(u64, u32, u32)> {
    if let Ok(mut cache) = RENDER_CACHE.lock()
        && let Some(existing) = cache.get(id, None, Some(RenderProfile::default()))
    {
        return Some((id, existing.width, existing.height));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?;
    let (width, height) = image.dimensions();
    dims_cache_put(id, InlineDims { width, height });

    let ext = inline_image_extension(media_type);
    let path = {
        let cache = RENDER_CACHE.lock().ok()?;
        cache.cache_dir.join(format!("{:016x}_inline.{}", id, ext))
    };
    if !path.exists() && fs::write(&path, &bytes).is_err() {
        return None;
    }
    // Prime the source cache with the already-decoded pixels so the follow-up
    // render does not re-open + re-decode the file we just wrote.
    if let Ok(mut source) = SOURCE_CACHE.lock() {
        source.insert(id, path.clone(), image);
    }
    if let Ok(mut cache) = RENDER_CACHE.lock() {
        cache.insert(
            id,
            RenderProfile::default(),
            CachedDiagram {
                path,
                width,
                height,
            },
        );
        return Some((id, width, height));
    }

    None
}

fn inline_image_extension(media_type: &str) -> &'static str {
    match media_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        _ => "img",
    }
}

/// Every extension [`materialize_inline_image_by_id`] can write, so
/// [`rediscover_inline_image`] probes exactly the files that can exist.
const INLINE_EXTENSIONS: [&str; 7] = ["png", "jpg", "gif", "webp", "bmp", "ico", "img"];

/// Re-register an inline image from its persisted cache file.
///
/// Payloads are staged in the UI's payload registry only until first
/// materialization (which writes the decoded bytes to the cache dir); after
/// that the base64 copy is dropped to keep multi-megabyte screenshots from
/// being resident twice. If the in-memory `RENDER_CACHE` entry is later
/// LRU-evicted, this path restores it from disk without needing the payload.
///
/// Returns `(id, width, height)` when the cache file exists and decodes.
pub fn rediscover_inline_image(id: u64) -> Option<(u64, u32, u32)> {
    let cache_dir = RENDER_CACHE.lock().ok()?.cache_dir.clone();
    let path = INLINE_EXTENSIONS
        .iter()
        .map(|ext| cache_dir.join(format!("{:016x}_inline.{}", id, ext)))
        .find(|path| path.exists())?;
    let image = image::open(&path).ok()?;
    let (width, height) = image.dimensions();
    dims_cache_put(id, InlineDims { width, height });
    if let Ok(mut source) = SOURCE_CACHE.lock() {
        source.insert(id, path.clone(), image);
    }
    if let Ok(mut cache) = RENDER_CACHE.lock() {
        cache.insert(
            id,
            RenderProfile::default(),
            CachedDiagram {
                path,
                width,
                height,
            },
        );
        return Some((id, width, height));
    }
    None
}


/// Decode a bounded prefix of the base64 payload and try to read image
/// dimensions straight from the container header (PNG/JPEG/GIF/BMP/WEBP).
fn dims_from_b64_prefix(data_b64: &str) -> Option<(u32, u32)> {
    // 16 KB of base64 -> 12 KB of header bytes, plenty for every supported
    // container's dimension fields while staying far cheaper than a full decode.
    const PREFIX_B64_CHARS: usize = 16 * 1024;
    let take = data_b64.len().min(PREFIX_B64_CHARS);
    // base64 decodes in 4-char groups; trim to a group boundary so the prefix
    // is self-consistent without trailing padding.
    let take = take - (take % 4);
    if take == 0 {
        return None;
    }
    let prefix = &data_b64.as_bytes()[..take];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(prefix)
        .ok()?;
    dimensions_from_header(&bytes)
}

/// Parse image dimensions directly from container header bytes. Mirrors the
/// lightweight parser used by the `read` tool, but lives here so the inline
/// renderer has no cross-crate dependency for header sniffing.
pub(crate) fn dimensions_from_header(data: &[u8]) -> Option<(u32, u32)> {
    // PNG: 8-byte signature then IHDR (width/height as big-endian u32).
    if data.len() > 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        if width > 0 && height > 0 {
            return Some((width, height));
        }
    }

    // JPEG: scan for a Start-Of-Frame marker.
    if data.len() > 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            // SOF0 (baseline) / SOF1 / SOF2 (progressive) etc.
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                if width > 0 && height > 0 {
                    return Some((width, height));
                }
                return None;
            }
            if i + 3 < data.len() {
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + len;
            } else {
                break;
            }
        }
    }

    // GIF: logical screen descriptor (little-endian u16).
    if data.len() > 10 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        let width = u16::from_le_bytes([data[6], data[7]]) as u32;
        let height = u16::from_le_bytes([data[8], data[9]]) as u32;
        if width > 0 && height > 0 {
            return Some((width, height));
        }
    }

    // BMP: DIB header (BITMAPINFOHEADER) width/height at offset 18/22.
    if data.len() > 26 && &data[0..2] == b"BM" {
        let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        if width > 0 && height != 0 {
            return Some((width as u32, height.unsigned_abs()));
        }
    }

    // WEBP (VP8X/VP8L/VP8): "RIFF"...."WEBP".
    if data.len() > 30 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        let fourcc = &data[12..16];
        if fourcc == b"VP8X" && data.len() > 30 {
            let w = 1
                + (u32::from(data[24]) | (u32::from(data[25]) << 8) | (u32::from(data[26]) << 16));
            let h = 1
                + (u32::from(data[27]) | (u32::from(data[28]) << 8) | (u32::from(data[29]) << 16));
            return Some((w, h));
        }
        if fourcc == b"VP8 " && data.len() > 30 {
            // Lossy: dimensions live in the key-frame header.
            let w = (u16::from_le_bytes([data[26], data[27]]) & 0x3FFF) as u32;
            let h = (u16::from_le_bytes([data[28], data[29]]) & 0x3FFF) as u32;
            if w > 0 && h > 0 {
                return Some((w, h));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png_b64() -> String {
        // 1x1 transparent PNG.
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string()
    }

    #[test]
    fn inline_dims_reads_png_header_without_full_decode() {
        let b64 = tiny_png_b64();
        let (id, w, h) = inline_image_dims("image/png", &b64).expect("dims");
        assert_eq!((w, h), (1, 1));
        // Cached and id-stable.
        let again = inline_image_dims("image/png", &b64).expect("dims again");
        assert_eq!(again.0, id);
    }

    #[test]
    fn inline_id_is_stable_and_distinct() {
        let a = inline_image_id("image/png", "AAAA");
        let b = inline_image_id("image/png", "AAAA");
        let c = inline_image_id("image/png", "BBBB");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
