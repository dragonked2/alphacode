#![cfg_attr(test, allow(clippy::items_after_test_module))]

use super::{Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::bus::{Bus, BusEvent, FileOp, FileTouch};
use crate::alphacode_terminal_image::{ImageDisplayParams, ImageProtocol, display_image};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;

const DEFAULT_LIMIT: usize = 5000;
const MAX_LINE_LEN: usize = 2000;
const BINARY_DETECT_SIZE: usize = 8192;
const BINARY_NULL_THRESHOLD: f64 = 0.1;
const MAX_IMAGE_READ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_READ_BYTES: u64 = 100 * 1024 * 1024;
#[cfg(feature = "pdf")]
const MAX_PDF_OUTPUT_CHARS: usize = 100_000;

pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug)]
struct ReadInput {
    file_path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
    offset: Option<usize>,
    limit: Option<usize>,
}

impl ReadInput {
    /// Read the range fields, accepting a numeric string.
    ///
    /// The `deserialize_with` coercers these used to carry handled
    /// `"timeout": "30"`-style input; they are kept for the same reason, since
    /// providers that stringify tool arguments are common.
    fn optional_index(input: &Value, key: &str) -> Option<usize> {
        let value = input.get(key)?;
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }

    fn from_value(input: &Value) -> Result<Self> {
        let file_path = super::coerce_text_arg(
            input,
            "read",
            "file_path",
            &[
                "path",
                "file",
                "filename",
                "file_name",
                "filePath",
                "filepath",
                "target",
            ],
        )?;
        Ok(Self {
            file_path,
            start_line: Self::optional_index(input, "start_line"),
            end_line: Self::optional_index(input, "end_line"),
            offset: Self::optional_index(input, "offset"),
            limit: Self::optional_index(input, "limit"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadRangeStyle {
    OffsetLimit,
    StartEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NormalizedReadRange {
    offset: usize,
    limit: usize,
    style: ReadRangeStyle,
}

impl NormalizedReadRange {
    fn next_offset(self) -> usize {
        self.offset.saturating_add(self.limit)
    }

    fn next_start_line(self) -> usize {
        self.next_offset().saturating_add(1)
    }
}

fn normalize_read_range(params: &ReadInput) -> Result<NormalizedReadRange> {
    let has_start_end = params.start_line.is_some() || params.end_line.is_some();
    let has_mixed_offset = match (params.start_line, params.end_line, params.offset) {
        (Some(start_line), _, Some(offset)) => {
            if start_line == 0 {
                true
            } else {
                offset.checked_add(1) != Some(start_line)
            }
        }
        (None, Some(_), Some(offset)) => offset != 0,
        _ => params.offset.is_some(),
    };

    if has_start_end && has_mixed_offset {
        return Err(anyhow::anyhow!(
            "Use either start_line/end_line (1-based) or offset (0-based), not both. `limit` may be used with either style."
        ));
    }

    if has_start_end {
        let start_line = params.start_line.unwrap_or(1);
        if start_line == 0 {
            return Err(anyhow::anyhow!(
                "start_line must be 1 or greater (it is 1-based)."
            ));
        }

        let limit = if let Some(end_line) = params.end_line {
            if end_line == 0 {
                return Err(anyhow::anyhow!(
                    "end_line must be 1 or greater (it is 1-based)."
                ));
            }
            if end_line < start_line {
                return Err(anyhow::anyhow!(
                    "end_line ({}) must be greater than or equal to start_line ({}).",
                    end_line,
                    start_line
                ));
            }
            end_line.saturating_sub(start_line).saturating_add(1)
        } else {
            params.limit.unwrap_or(DEFAULT_LIMIT)
        };

        return Ok(NormalizedReadRange {
            offset: start_line - 1,
            limit,
            style: ReadRangeStyle::StartEnd,
        });
    }

    Ok(NormalizedReadRange {
        offset: params.offset.unwrap_or(0),
        limit: params.limit.unwrap_or(DEFAULT_LIMIT),
        style: ReadRangeStyle::OffsetLimit,
    })
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read file contents (text, images, PDFs). Use offset/limit for large files. Always read before editing."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file_path": {
                    "type": "string",
                    "description": "Path to a file."
                },
                "start_line": {
                    "type": "integer",
                    "description": "1-based start line for text files."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max text lines to read. Default 5000."
                }
            }
        })
    }

    fn execution_class(&self, _input: &Value) -> super::ToolExecutionClass {
        super::ToolExecutionClass::ReadOnly
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // The path is recovered before the range is normalized, because a bare
        // string or a truncated payload made `from_value` fail with "missing
        // field `file_path`" for a call that named the file correctly — and
        // `read` is the tool a model reaches for to recover, so a failed read
        // costs the whole turn.
        let params = ReadInput::from_value(&input)?;
        let range = normalize_read_range(&params)?;

        let path = ctx.resolve_path_guarded(Path::new(&params.file_path))?;

        // Check if file exists
        if !path.exists() {
            // Try to find similar files
            let suggestions = find_similar_files(&path);
            if suggestions.is_empty() {
                return Err(anyhow::anyhow!("File not found: {}", params.file_path));
            } else {
                return Err(anyhow::anyhow!(
                    "File not found: {}\nDid you mean: {}",
                    params.file_path,
                    suggestions.join(", ")
                ));
            }
        }

        // Check for image files and display in terminal if supported
        if is_image_file(&path) {
            return handle_image_file(&path, &params.file_path);
        }

        // Check for PDF files and extract text
        if is_pdf_file(&path) {
            return handle_pdf_file(&path, &params.file_path);
        }

        // Check for binary files
        if is_binary_file(&path) {
            return Ok(ToolOutput::new(format!(
                "Binary file detected: {}\nUse appropriate tools to handle binary files.",
                params.file_path
            )));
        }

        // Stream instead of loading the entire file. This keeps memory bounded
        // for large logs and counts lines in the same pass.
        let range_read = read_text_range(&path, range).await?;
        let mut output = range_read.output;
        let total_lines = range_read.lines_observed;
        let truncated_line_count = range_read.truncated_line_count;
        // When the scan stopped at the end of the requested window, the file
        // continues past it and the line count is a *lower bound*, not an
        // exact total.
        let more_lines = range_read.more_lines;
        let returned_lines = total_lines.saturating_sub(range.offset).min(range.limit);
        let end = if returned_lines == 0 {
            range.offset.min(total_lines)
        } else {
            range.offset.saturating_add(returned_lines)
        };

        // Publish file touch event for swarm coordination
        Bus::global().publish(BusEvent::FileTouch(FileTouch {
            session_id: ctx.session_id.clone(),
            path: path.to_path_buf(),
            op: FileOp::Read,
            intent: None,
            summary: Some(format!(
                "read lines {}-{} of {}",
                range.offset.saturating_add(1),
                end,
                total_lines
            )),
            detail: None,
        }));

        if truncated_line_count > 0 || more_lines {
            crate::logging::warn(&format!(
                "[tool:read] returned truncated output for {} in session {} (tool_call={} range={}..{} total_lines={} truncated_lines={})",
                params.file_path,
                ctx.session_id,
                ctx.tool_call_id,
                range.offset.saturating_add(1),
                end,
                total_lines,
                truncated_line_count
            ));
        }

        // Add metadata
        if more_lines {
            // The exact number of remaining lines is unknown without reading
            // the rest of the file, so state the lower bound honestly instead
            // of claiming a precise remainder.
            let continuation_hint = match range.style {
                ReadRangeStyle::OffsetLimit => {
                    format!("Use `offset={}` to continue.", range.next_offset())
                }
                ReadRangeStyle::StartEnd => {
                    format!("Use `start_line={}` to continue.", range.next_start_line())
                }
            };
            output.push_str(&format!("\n[{continuation_hint} more lines remain]"));
        }

        if output.is_empty() {
            let message = if total_lines == 0 {
                "(empty file)".to_string()
            } else if more_lines {
                format!("(no lines in the requested range; file has at least {total_lines} lines)")
            } else {
                format!("(no lines in the requested range; file contains {total_lines} lines)")
            };
            Ok(ToolOutput::new(message).with_metadata(json!({
                "tool": "read",
                "path": params.file_path,
                "lines": 0,
                "total_lines": total_lines,
                "total_lines_is_lower_bound": more_lines,
            })))
        } else {
            Ok(ToolOutput::new(output)
                .with_title(params.file_path.clone())
                .with_metadata(json!({
                    "tool": "read",
                    "path": params.file_path,
                    "start_line": range.offset.saturating_add(1),
                    "end_line": end,
                    "total_lines": total_lines,
                    "total_lines_is_lower_bound": more_lines,
                    "truncated_lines": truncated_line_count,
                })))
        }
    }
}

/// The outcome of a bounded range read.
struct RangeRead {
    /// The rendered lines for the requested window.
    output: String,
    /// Number of lines observed while scanning.
    ///
    /// This is **exact** when `more_lines` is false. When `more_lines` is
    /// true it is a **lower bound**: the file contains at least this many
    /// lines, and the scan stopped before reaching the end of the file.
    lines_observed: usize,
    /// Number of lines whose content was truncated at `MAX_LINE_LEN`.
    truncated_line_count: usize,
    /// True when the file continues past the requested window.
    ///
    /// The previous implementation scanned the whole file to count lines it
    /// never returned, so a 200k-line file was read in full to serve a
    /// 5 000-line window — a 40x I/O amplification. The scan now stops at the
    /// end of the window and reports the count as a lower bound instead.
    more_lines: bool,
}

/// Read only the requested window of lines, stopping at `end_exclusive`.
///
/// The scan stops as soon as the requested window is complete, so the bytes
/// read are proportional to the window rather than to the file size. The
/// line count is reported as a lower bound when the file continues past the
/// window.
async fn read_text_range(path: &Path, range: NormalizedReadRange) -> Result<RangeRead> {
    // Repeated-read fast path (Phase 4.3): a session typically touches the
    // same files repeatedly. When the file metadata is unchanged since a
    // previous read and its line ledger still covers the requested window,
    // replay from memory instead of re-opening and re-scanning the file (one
    // `metadata()` probe instead of the whole file read).
    if let Some(replay) = cached_window_for(path, &range)
        && let Some(replayed) = replay_cached_window(&range, &replay)
    {
        return Ok(replayed);
    }

    let uncached = read_text_range_uncached(path, range).await?;
    remember_cached_window(path, range, &uncached);
    Ok(uncached)
}

// === Repeated-read window cache ===

use std::time::SystemTime;

/// One line as the scanner observed it: enough to re-run the identical
/// `append_read_line` call without the file.
///
/// `Clone` is required because a cached window is cloned out of the map to
/// hand back an `Arc` without holding the map's lock while the caller reads.
#[derive(Clone)]
struct CachedReadLine {
    len: usize,
    last_byte: Option<u8>,
    ended_with_newline: bool,
    bytes: Vec<u8>,
}

struct CachedReadWindow {
    mtime: Option<SystemTime>,
    file_len: u64,
    /// Line index of the first cached line (0 unless the cached read itself
    /// started mid-file).
    start_index: usize,
    lines: Vec<CachedReadLine>,
    /// The cached scan ended at EOF, so `start_index + lines.len()` is the
    /// file's true line count. When false the window stops one line earlier
    /// deliberately and only same-or-earlier windows may replay.
    ended_at_eof: bool,
    /// The `more_lines` flag observed when scanning stopped exactly at this
    /// window's end.
    more_at_end: bool,
}

static READ_WINDOW_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, CachedReadWindow>>,
> = std::sync::OnceLock::new();

/// Cache bound. Entries are whole read windows; the common working set is a
/// handful of files, and a stale entry costs one `metadata()` probe.
const READ_WINDOW_CACHE_ENTRIES: usize = 24;
/// Total line bytes cached across the map before new entries are refused.
const READ_WINDOW_CACHE_BYTES: usize = 8 * 1024 * 1024;

fn read_window_cache()
-> std::sync::MutexGuard<'static, std::collections::BTreeMap<std::path::PathBuf, CachedReadWindow>>
{
    let map =
        READ_WINDOW_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
    map.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn window_bytes(entry: &CachedReadWindow) -> usize {
    entry.lines.iter().map(|line| line.bytes.len()).sum()
}

/// Look up a fresh window covering the requested range. The metadata probe
/// is the id check: a different mtime or size means the entry is stale and
/// the read falls through to the file.
fn cached_window_for(
    path: &Path,
    range: &NormalizedReadRange,
) -> Option<std::sync::Arc<CachedReadWindow>> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok();
    let file_len = metadata.len();
    // The guard must outlive `cached`: the entry borrows from the map the
    // guard owns, so a borrow of a temporary guard would dangle.
    let cache = read_window_cache();
    let cached = cache.get(path)?;
    if cached.mtime != mtime || cached.file_len != file_len {
        return None;
    }
    // The replay must not request lines beyond what the cached scan saw:
    // beyond a non-EOF window the ledger simply does not have them.
    let span_end = cached.start_index + cached.lines.len();
    if range.offset > span_end {
        return None;
    }
    if range.next_offset() > span_end && !cached.ended_at_eof {
        return None;
    }
    Some(std::sync::Arc::new(CachedReadWindow {
        mtime: cached.mtime,
        file_len: cached.file_len,
        start_index: cached.start_index,
        lines: cached.lines.clone(),
        ended_at_eof: cached.ended_at_eof,
        more_at_end: cached.more_at_end,
    }))
}

fn replay_cached_window(
    range: &NormalizedReadRange,
    replay: &CachedReadWindow,
) -> Option<RangeRead> {
    let end_exclusive = range.next_offset();
    let span_end = replay.start_index + replay.lines.len();
    if range.offset < replay.start_index || range.offset > span_end {
        return None;
    }
    if end_exclusive > span_end && !replay.ended_at_eof {
        return None;
    }

    let mut output = String::with_capacity(range.limit.min(2000) * 80);
    let mut truncated_line_count = 0usize;
    for (offset_from_start, line) in replay.lines.iter().enumerate() {
        let index = replay.start_index + offset_from_start;
        if index >= end_exclusive {
            continue;
        }
        append_read_line(
            &mut output,
            ReadLinePreview {
                index,
                len: line.len,
                last_byte: line.last_byte,
                ended_with_newline: line.ended_with_newline,
                bytes: &line.bytes,
            },
            *range,
            &mut truncated_line_count,
        )
        .ok()?;
    }

    let hit_end = end_exclusive.min(span_end);
    let more = if hit_end < span_end {
        // The cached ledger continues past the requested stop, so the file
        // certainly does: the original scanner would have seen the same.
        true
    } else if replay.ended_at_eof {
        false
    } else {
        replay.more_at_end
    };
    Some(RangeRead {
        output,
        lines_observed: hit_end,
        truncated_line_count,
        more_lines: more,
    })
}

fn remember_cached_window(path: &Path, range: NormalizedReadRange, read: &RangeRead) {
    // Only windows built from observed lines are cacheable; the replay needs
    // the per-line ledger, so the reader would have to expose it. Reuse the
    // cheap whole-content case instead: files small enough to hold fully.
    if range.offset != 0 {
        return;
    }
    if read.output.len() > 1024 * 1024 {
        return;
    }
    // The ledger is not available from `RangeRead`; skip populating from the
    // processed output and only cache when a full scan reached EOF.
    if read.more_lines {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let Ok(mtime) = metadata.modified() else {
        return;
    };
    let Ok(content) = std::fs::read(path) else {
        return;
    };
    if content.len() > READ_WINDOW_CACHE_BYTES / 4 {
        return;
    }
    let lines: Vec<CachedReadLine> = {
        let mut entries = Vec::new();
        for piece in content.split_inclusive(|b| *b == b'\n') {
            let ended_with_newline = piece.last() == Some(&b'\n');
            let body = if ended_with_newline {
                &piece[..piece.len() - 1]
            } else {
                piece
            };
            if body.contains(&b'\0') {
                // Binary-ish content: outputs would not match the renderer.
                return;
            }
            entries.push(CachedReadLine {
                len: body.len(),
                last_byte: body.last().copied(),
                ended_with_newline,
                bytes: body.to_vec(),
            });
            if entries.len() > 20_000 {
                return;
            }
        }
        entries
    };
    let bytes_needed = window_bytes_len(&lines);
    let mut cache = read_window_cache();
    if cache.len() >= READ_WINDOW_CACHE_ENTRIES {
        cache.pop_first();
    }
    let total: usize = cache.values().map(window_bytes).sum();
    if total + bytes_needed > READ_WINDOW_CACHE_BYTES {
        return;
    }
    cache.insert(
        path.to_path_buf(),
        CachedReadWindow {
            mtime: Some(mtime),
            file_len: metadata.len(),
            start_index: 0,
            lines,
            ended_at_eof: true,
            more_at_end: false,
        },
    );
}

fn window_bytes_len(lines: &[CachedReadLine]) -> usize {
    lines.iter().map(|line| line.bytes.len()).sum()
}

async fn read_text_range_uncached(path: &Path, range: NormalizedReadRange) -> Result<RangeRead> {
    use tokio::io::AsyncBufReadExt as _;

    let file = tokio::fs::File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(file);
    let mut output = String::with_capacity(range.limit.min(2000) * 80);
    let mut line_bytes = Vec::with_capacity(MAX_LINE_LEN + 1);
    let mut line_len = 0usize;
    let mut last_byte = None;
    let mut total_lines = 0usize;
    let mut truncated_line_count = 0usize;
    let end_exclusive = range.next_offset();

    loop {
        // Bounded read: the window is complete, so stop scanning. The
        // remaining file is never read. The one `fill_buf` below only
        // establishes whether the file continues past the window.
        if total_lines >= end_exclusive {
            let more = !reader.fill_buf().await?.is_empty();
            return Ok(RangeRead {
                output,
                lines_observed: total_lines,
                truncated_line_count,
                more_lines: more,
            });
        }

        let (consumed, ended_line, at_eof) = {
            let buffer = reader.fill_buf().await?;
            if buffer.is_empty() {
                (0, false, true)
            } else {
                let newline = buffer.iter().position(|byte| *byte == b'\n');
                let content_len = newline.unwrap_or(buffer.len());
                let selected = total_lines >= range.offset && total_lines < end_exclusive;
                if selected {
                    let remaining_capture = (MAX_LINE_LEN + 1).saturating_sub(line_bytes.len());
                    line_bytes.extend_from_slice(&buffer[..content_len.min(remaining_capture)]);
                }
                if content_len > 0 {
                    line_len = line_len.saturating_add(content_len);
                    last_byte = Some(buffer[content_len - 1]);
                }
                let ended_line = newline.is_some();
                let consumed = content_len + usize::from(ended_line);
                (consumed, ended_line, false)
            }
        };

        if at_eof {
            // `str::lines` does not add an extra row after a trailing newline.
            if line_len > 0 {
                append_read_line(
                    &mut output,
                    ReadLinePreview {
                        index: total_lines,
                        len: line_len,
                        last_byte,
                        ended_with_newline: false,
                        bytes: &line_bytes,
                    },
                    range,
                    &mut truncated_line_count,
                )?;
                total_lines = total_lines.saturating_add(1);
            }
            break;
        }

        reader.consume(consumed);
        if ended_line {
            append_read_line(
                &mut output,
                ReadLinePreview {
                    index: total_lines,
                    len: line_len,
                    last_byte,
                    ended_with_newline: true,
                    bytes: &line_bytes,
                },
                range,
                &mut truncated_line_count,
            )?;
            total_lines = total_lines.saturating_add(1);
            line_len = 0;
            last_byte = None;
            line_bytes.clear();
        }
    }

    Ok(RangeRead {
        output,
        lines_observed: total_lines,
        truncated_line_count,
        // The scan reached the end of the file, so the count is exact.
        more_lines: false,
    })
}

struct ReadLinePreview<'a> {
    index: usize,
    len: usize,
    last_byte: Option<u8>,
    ended_with_newline: bool,
    bytes: &'a [u8],
}

fn append_read_line(
    output: &mut String,
    line: ReadLinePreview<'_>,
    range: NormalizedReadRange,
    truncated_line_count: &mut usize,
) -> Result<()> {
    use std::fmt::Write as _;

    if line.index < range.offset || line.index >= range.next_offset() {
        return Ok(());
    }

    let line_num = line.index.saturating_add(1);
    let effective_len = line.len.saturating_sub(usize::from(
        line.ended_with_newline && line.last_byte == Some(b'\r'),
    ));
    if effective_len > MAX_LINE_LEN {
        *truncated_line_count = truncated_line_count.saturating_add(1);
        let prefix_len = MAX_LINE_LEN.min(line.bytes.len());
        let prefix = match std::str::from_utf8(&line.bytes[..prefix_len]) {
            Ok(prefix) => prefix,
            Err(error) if error.error_len().is_none() => {
                std::str::from_utf8(&line.bytes[..error.valid_up_to()])?
            }
            Err(error) => return Err(error.into()),
        };
        let _ = writeln!(output, "{line_num:>5}\t{prefix}...");
    } else {
        let bytes = &line.bytes[..effective_len.min(line.bytes.len())];
        let content = std::str::from_utf8(bytes)?;
        let _ = writeln!(output, "{line_num:>5}\t{content}");
    }
    Ok(())
}

#[cfg(test)]
mod tests;

use std::io::Read;

fn is_binary_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        let binary_exts = [
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "zip", "tar", "gz", "bz2", "xz",
            "7z", "rar", "exe", "dll", "so", "dylib", "o", "a", "class", "pyc", "wasm", "mp3",
            "mp4", "avi", "mov", "mkv", "flac", "ogg", "wav",
        ];
        if binary_exts.contains(&ext.as_str()) {
            return true;
        }
    }

    if let Ok(mut file) = std::fs::File::open(path) {
        let mut buf = [0u8; BINARY_DETECT_SIZE];
        if let Ok(n) = file.read(&mut buf)
            && n > 0
        {
            let null_count = buf[..n].iter().filter(|&&b| b == 0).count();
            if null_count as f64 > n as f64 * BINARY_NULL_THRESHOLD {
                return true;
            }
            // UTF-8 text outside ASCII (for example Japanese source or Arabic
            // logs) is still text. Classify by decoding and control characters
            // instead of requiring most bytes to be ASCII-printable.
            let sample = match std::str::from_utf8(&buf[..n]) {
                Ok(sample) => sample,
                Err(error) if error.error_len().is_none() => {
                    match std::str::from_utf8(&buf[..error.valid_up_to()]) {
                        Ok(sample) => sample,
                        Err(_) => return true,
                    }
                }
                Err(_) => return true,
            };
            let char_count = sample.chars().count();
            let control_count = sample
                .chars()
                .filter(|ch| ch.is_control() && !matches!(ch, '\t' | '\n' | '\r' | '\u{000c}'))
                .count();
            if char_count > 0 && control_count.saturating_mul(10) > char_count {
                return true;
            }
        }
    }

    false
}

fn find_similar_files(path: &Path) -> Vec<String> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path.file_name().map(|s| s.to_string_lossy().to_lowercase());
    let target_name = match &filename {
        Some(name) => name.clone(),
        None => return Vec::new(),
    };

    let mut suggestions = Vec::new();

    // Strategy 1: exact sibling files in the same directory
    if let Ok(entries) = std::fs::read_dir(parent) {
        let mut scored: Vec<(usize, String)> = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let score = file_name_similarity_score(&name, &target_name);
            if score < usize::MAX {
                let display = entry.path().display().to_string();
                scored.push((score, display));
            }
        }
        scored.sort_by(|a, b| a.0.cmp(&b.0));
        suggestions = scored.into_iter().take(3).map(|(_, path)| path).collect();
    }

    // Strategy 2: if no sibling matches, try parent's parent (common typo)
    if suggestions.is_empty()
        && let Some(grandparent) = parent.parent()
        && let Ok(entries) = std::fs::read_dir(grandparent)
    {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let score = file_name_similarity_score(&name, &target_name);
            if score <= 2 {
                suggestions.push(entry.path().display().to_string());
                if suggestions.len() >= 3 {
                    break;
                }
            }
        }
    }

    suggestions
}

fn file_name_similarity_score(name: &str, target: &str) -> usize {
    if *name == *target {
        0
    } else if name.contains(target) || target.contains(name) {
        1
    } else {
        // Extension-aware: penalize mismatched extensions (e.g. .rs vs .ts)
        let target_ext = std::path::Path::new(target)
            .extension()
            .map(|e| e.to_ascii_lowercase());
        let name_ext = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_ascii_lowercase());
        let ext_penalty = if target_ext.is_some() && target_ext != name_ext {
            2
        } else {
            0
        };

        // Stem similarity (filename without extension). file_stem() yields
        // an OsString; convert to String so contains/len/levenshtein work.
        let target_stem = std::path::Path::new(target)
            .file_stem()
            .map(|s| s.to_ascii_lowercase().to_string_lossy().to_string())
            .unwrap_or_default();
        let name_stem = std::path::Path::new(name)
            .file_stem()
            .map(|s| s.to_ascii_lowercase().to_string_lossy().to_string())
            .unwrap_or_default();

        if !target_stem.is_empty() && !name_stem.is_empty() {
            // Check stem containment first (very fast)
            if name_stem.contains(&target_stem) || target_stem.contains(&name_stem) {
                return 1 + ext_penalty;
            }
            let dist = levenshtein_distance(&target_stem, &name_stem);
            let max_len = target_stem.len().max(name_stem.len());
            if max_len == 0 {
                return usize::MAX;
            }
            if dist <= max_len / 3 && dist > 0 {
                return 2 + dist + ext_penalty;
            }
        }

        // Fallback: full name Levenshtein
        let dist = levenshtein_distance(target, name);
        let max_len = target.len().max(name.len());
        if max_len == 0 {
            return usize::MAX;
        }
        if dist <= max_len / 3 && dist > 0 {
            2 + dist + ext_penalty
        } else {
            usize::MAX
        }
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    crate::alphacode_core::util::levenshtein(a, b)
}

/// Check if a file is an image based on extension
fn is_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
        )
    } else {
        false
    }
}

/// Handle reading an image file - display in terminal if supported AND return base64 for model vision
fn handle_image_file(path: &Path, file_path: &str) -> Result<ToolOutput> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();
    if file_size > MAX_IMAGE_READ_BYTES {
        return Ok(ToolOutput::new(format!(
            "Image: {file_path} ({:.1} MB)\nImage exceeds the safe read limit of {} MB and was not loaded.",
            file_size as f64 / 1024.0 / 1024.0,
            MAX_IMAGE_READ_BYTES / (1024 * 1024)
        ))
        .with_title(format!("📷 {file_path}")));
    }

    let protocol = ImageProtocol::detect();

    use std::io::Read as _;
    let mut data = Vec::with_capacity(file_size as usize);
    std::fs::File::open(path)?
        .take(MAX_IMAGE_READ_BYTES.saturating_add(1))
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_IMAGE_READ_BYTES {
        return Ok(ToolOutput::new(format!(
            "Image: {file_path}\nImage grew beyond the safe read limit and was not loaded."
        ))
        .with_title(format!("📷 {file_path}")));
    }

    let dimensions = get_image_dimensions_from_data(&data);

    let dim_str = dimensions
        .map(|(w, h)| format!("{}x{}", w, h))
        .unwrap_or_else(|| "unknown".to_string());

    let size_str = if file_size < 1024 {
        format!("{} bytes", file_size)
    } else if file_size < 1024 * 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", file_size as f64 / 1024.0 / 1024.0)
    };

    let mut terminal_displayed = false;
    if protocol.is_supported() {
        let params = ImageDisplayParams::from_terminal();
        match display_image(path, &params) {
            Ok(true) => {
                terminal_displayed = true;
            }
            Ok(false) => {}
            Err(e) => {
                crate::logging::info(&format!("Warning: Failed to display image: {}", e));
            }
        }
    }

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let media_type = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => "image/png",
    };

    const MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024;
    let mut output = if file_size <= MAX_IMAGE_SIZE {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
        let display_note = if terminal_displayed {
            "Displayed in terminal. "
        } else {
            ""
        };
        ToolOutput::new(format!(
            "Image: {} ({})\nDimensions: {}\n{}Image sent to model for vision analysis.",
            file_path, size_str, dim_str, display_note
        ))
        .with_labeled_image(media_type, b64, file_path.to_string())
    } else {
        let display_note = if terminal_displayed {
            "\nDisplayed in terminal."
        } else {
            ""
        };
        ToolOutput::new(format!(
            "Image: {} ({})\nDimensions: {}\nImage too large for vision (max 20MB).{}",
            file_path, size_str, dim_str, display_note
        ))
    };

    output = output.with_title(format!("📷 {}", file_path));
    Ok(output)
}

/// Get image dimensions from raw data (duplicated from tui::image for convenience)
fn get_image_dimensions_from_data(data: &[u8]) -> Option<(u32, u32)> {
    // PNG: check signature and parse IHDR chunk
    if data.len() > 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((width, height));
    }

    // JPEG: look for SOF0/SOF2 markers
    if data.len() > 2 && data[0] == 0xFF && data[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            // SOF0 (baseline) or SOF2 (progressive)
            if marker == 0xC0 || marker == 0xC2 {
                let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((width, height));
            }
            // Skip to next marker
            if i + 3 < data.len() {
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + len;
            } else {
                break;
            }
        }
    }

    // GIF: parse header
    if data.len() > 10 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") {
        let width = u16::from_le_bytes([data[6], data[7]]) as u32;
        let height = u16::from_le_bytes([data[8], data[9]]) as u32;
        return Some((width, height));
    }

    None
}

/// Check if a file is a PDF based on extension
fn is_pdf_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext.to_string_lossy().to_lowercase() == "pdf"
    } else {
        false
    }
}

/// Handle reading a PDF file - extract text content
#[cfg(feature = "pdf")]
fn handle_pdf_file(path: &Path, file_path: &str) -> Result<ToolOutput> {
    // Get file metadata
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size > MAX_PDF_READ_BYTES {
        return Ok(ToolOutput::new(format!(
            "PDF: {file_path} ({:.1} MB)\nPDF exceeds the safe extraction limit of {} MB and was not loaded.",
            file_size as f64 / 1024.0 / 1024.0,
            MAX_PDF_READ_BYTES / (1024 * 1024)
        )));
    }

    let size_str = if file_size < 1024 {
        format!("{} bytes", file_size)
    } else if file_size < 1024 * 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", file_size as f64 / 1024.0 / 1024.0)
    };

    // Extract text from PDF
    match crate::alphacode_pdf::extract_text(path) {
        Ok(text) => {
            let mut output = String::new();
            output.push_str(&format!("PDF: {} ({})\n", file_path, size_str));
            output.push_str(&format!("{}\n", "=".repeat(60)));

            // pdf_extract uses form feed `\x0c` as the page separator. Count
            // without collecting a potentially huge vector of page slices.
            let page_count = text.matches('\x0c').count() + 1;

            output.push_str(&format!("Pages: {}\n\n", page_count));

            let mut output_chars = output.chars().count();
            let mut output_truncated = false;
            for (i, page) in text.split('\x0c').enumerate() {
                let page_text = page.trim();
                if !page_text.is_empty() {
                    if output_chars >= MAX_PDF_OUTPUT_CHARS {
                        output_truncated = true;
                        break;
                    }
                    output.push_str(&format!("--- Page {} ---\n", i + 1));
                    output_chars += format!("--- Page {} ---\n", i + 1).chars().count();
                    // Cap both individual pages and total returned text.
                    let remaining_chars = MAX_PDF_OUTPUT_CHARS.saturating_sub(output_chars);
                    let page_limit = 10_000.min(remaining_chars);
                    if page_text.chars().count() > page_limit {
                        output.push_str(crate::util::truncate_str(page_text, page_limit));
                        output.push_str("\n... (page truncated)\n");
                        output_chars = MAX_PDF_OUTPUT_CHARS;
                        output_truncated = true;
                        break;
                    } else {
                        output.push_str(page_text);
                        output_chars += page_text.chars().count();
                    }
                    output.push_str("\n\n");
                }
            }
            if output_truncated {
                output.push_str("\n[PDF text output capped at 100,000 characters.]\n");
            }

            Ok(ToolOutput::new(output))
        }
        Err(e) => {
            // Fall back to metadata only if text extraction fails
            Ok(ToolOutput::new(format!(
                "PDF: {} ({})\nCould not extract text: {}\nThis may be a scanned/image-based PDF.",
                file_path, size_str, e
            )))
        }
    }
}

/// Handle reading a PDF file when PDF support is not compiled in.
#[cfg(not(feature = "pdf"))]
fn handle_pdf_file(path: &Path, file_path: &str) -> Result<ToolOutput> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size > MAX_PDF_READ_BYTES {
        return Ok(ToolOutput::new(format!(
            "PDF: {file_path} ({:.1} MB)\nPDF exceeds the safe inspection limit of {} MB.",
            file_size as f64 / 1024.0 / 1024.0,
            MAX_PDF_READ_BYTES / (1024 * 1024)
        )));
    }

    let size_str = if file_size < 1024 {
        format!("{} bytes", file_size)
    } else if file_size < 1024 * 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", file_size as f64 / 1024.0 / 1024.0)
    };

    Ok(ToolOutput::new(format!(
        "PDF: {} ({})\nPDF text extraction is not available in this build. Rebuild with the `pdf` feature enabled to extract text.",
        file_path, size_str
    )))
}
