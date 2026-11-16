//! Inline images for the terminal face: markdown image markers resolved
//! against the embedded payload and drawn with half blocks (U+2580 pairs
//! one glyph with two vertical pixels), which works on any truecolor or
//! 256-color terminal. Mono hosts keep the marker as a labelled
//! placeholder — an image is never a reason to garble a legacy console.

use std::collections::HashMap;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::tui_caps::ColorDepth;

/// One decoded, downsized image, ready to paint at a given grid size.
#[derive(Clone)]
pub struct Rendered {
    pub lines: Vec<Line<'static>>,
}

/// Cache key: the payload path, the grid size and the color depth.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    path: String,
    cols: u16,
    rows: u16,
    depth: u8,
}

#[derive(Default)]
pub struct ImageCache {
    entries: HashMap<Key, Option<Rendered>>,
}

impl ImageCache {
    /// Renders `bytes` into at most `cols` x `rows` cells. Returns `None`
    /// when the image cannot be decoded or the depth is mono.
    pub fn render(
        &mut self,
        path: &str,
        bytes: Option<&[u8]>,
        cols: u16,
        rows: u16,
        depth: ColorDepth,
    ) -> Option<Rendered> {
        if depth == ColorDepth::Mono || cols == 0 || rows == 0 {
            return None;
        }
        let key = Key {
            path: path.to_string(),
            cols,
            rows,
            depth: depth as u8,
        };
        if let Some(hit) = self.entries.get(&key) {
            return hit.clone();
        }
        let rendered = bytes.and_then(|bytes| {
            let image = image::load_from_memory(bytes).ok()?;
            let rgba = image.to_rgba8();
            // Every text cell carries two vertical pixels.
            let target_w = u32::from(cols);
            let target_h = u32::from(rows) * 2;
            let resized = image::imageops::resize(
                &rgba,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            );
            let mut lines = Vec::with_capacity(rows as usize);
            for row in 0..u32::from(rows) {
                let mut spans: Vec<Span<'static>> = Vec::with_capacity(cols as usize);
                for col in 0..target_w {
                    let top = resized.get_pixel(col, row * 2);
                    let bottom = resized.get_pixel(col, row * 2 + 1);
                    let fg = map_color(top.0, depth);
                    let bg = map_color(bottom.0, depth);
                    spans.push(Span::styled("▀", Style::default().fg(fg).bg(bg)));
                }
                lines.push(Line::from(spans));
            }
            Some(Rendered { lines })
        });
        self.entries.insert(key, rendered.clone());
        rendered
    }
}

/// The grid rows a `cols`-wide render needs so the aspect ratio survives
/// (each cell carries two vertical pixels). `None` when the bytes do not
/// decode as an image.
pub fn suggested_rows(bytes: &[u8], cols: u16) -> Option<u16> {
    if cols == 0 {
        return None;
    }
    let image = image::load_from_memory(bytes).ok()?;
    let (width, height) = image::GenericImageView::dimensions(&image);
    if width == 0 || height == 0 {
        return None;
    }
    let rows = u64::from(cols) * u64::from(height) / (u64::from(width) * 2);
    Some(u16::try_from(rows).unwrap_or(u16::MAX).max(1))
}

/// Paints one sRGB pixel at the host's color depth.
fn map_color(rgba: [u8; 4], depth: ColorDepth) -> Color {
    let (r, g, b) = (rgba[0], rgba[1], rgba[2]);
    match depth {
        ColorDepth::TrueColor => Color::Rgb(r, g, b),
        ColorDepth::Ansi256 => Color::Indexed(ansi256(r, g, b)),
        ColorDepth::Basic16 => basic16(r, g, b),
        ColorDepth::Mono => Color::Reset,
    }
}

/// The xterm 256-color palette's 6x6x6 cube.
fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let q = |v: u8| -> u8 {
        let level = u32::from(v) * 5 / 255;
        (level * 51) as u8
    };
    let cube_r = u32::from(q(r) / 51);
    let cube_g = u32::from(q(g) / 51);
    let cube_b = u32::from(q(b) / 51);
    16 + (36 * cube_r + 6 * cube_g + cube_b) as u8
}

/// The classic 16-color approximation.
fn basic16(r: u8, g: u8, b: u8) -> Color {
    let bright = u16::from(r) + u16::from(g) + u16::from(b) > 382;
    let r = r > 96;
    let g = g > 96;
    let b = b > 96;
    match (r, g, b, bright) {
        (false, false, false, _) => Color::Black,
        (true, false, false, false) => Color::DarkGray,
        (true, false, false, true) => Color::Red,
        (false, true, false, false) => Color::Green,
        (false, true, false, true) => Color::LightGreen,
        (false, false, true, false) => Color::Blue,
        (false, false, true, true) => Color::LightBlue,
        (true, true, false, false) => Color::Yellow,
        (false, true, true, false) => Color::Cyan,
        (true, false, true, false) => Color::Magenta,
        (true, true, false, true) => Color::LightYellow,
        (false, true, true, true) => Color::LightCyan,
        (true, false, true, true) => Color::LightMagenta,
        (true, true, true, false) => Color::Gray,
        _ => Color::White,
    }
}

/// A markdown image marker: `![alt](path)` alone on its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub line: usize,
    pub alt: String,
    pub path: String,
}

/// Extracts the standalone image markers of a markdown document (inline
/// markers inside prose keep rendering as text — the TUI has no line
/// reflow for mixed content).
pub fn image_markers(body: &str) -> Vec<Marker> {
    let mut markers = Vec::new();
    for (line, text) in body.lines().enumerate() {
        let trimmed = text.trim();
        if !trimmed.starts_with("![") {
            continue;
        }
        let Some(close_alt) = trimmed.find("](") else {
            continue;
        };
        let Some(close_path) = trimmed.rfind(')') else {
            continue;
        };
        if close_path <= close_alt + 2 {
            continue;
        }
        let alt = trimmed[2..close_alt].to_string();
        let path = trimmed[close_alt + 2..close_path].trim().to_string();
        if !path.is_empty() && !path.starts_with("http") {
            markers.push(Marker { line, alt, path });
        }
    }
    markers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_extract_only_standalone_local_images() {
        let body = "# Title\n![shot](assets/shot.png)\ninline ![x](a.png) text\n![remote](https://e/x.png)\n";
        let markers = image_markers(body);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].path, "assets/shot.png");
        assert_eq!(markers[0].alt, "shot");
        assert_eq!(markers[0].line, 1);
    }

    #[test]
    fn mono_and_empty_areas_render_nothing() {
        let mut cache = ImageCache::default();
        assert!(cache.render("x", Some(b"not an image"), 8, 4, ColorDepth::Mono).is_none());
        assert!(cache.render("x", Some(b"not an image"), 8, 4, ColorDepth::TrueColor).is_none());
        assert!(cache.render("x", None, 0, 4, ColorDepth::TrueColor).is_none());
    }

    #[test]
    fn ansi256_cube_stays_in_the_palette() {
        assert_eq!(ansi256(0, 0, 0), 16);
        assert!(ansi256(255, 255, 255) <= 231);
        assert!(ansi256(255, 0, 0) >= 16);
    }
}
