//! The license / content documents' markdown rendering for the egui
//! face — the SAME dialect `shell/web/src/richText.ts` implements, so
//! one authored document reads identically on both faces:
//!
//! - blocks: `#` / `##` / `###` headings, a line of 3+ `-` or `=` →
//!   horizontal rule, consecutive `- ` lines gather into bullets, `> `
//!   quotes stay muted and indented; blank lines separate paragraphs and
//!   EACH source line is its own row — the web's `<br/>` hard break, so
//!   legal text keeps its line folds instead of being reflowed;
//! - inline: `[label](https://…)` and bare http(s) URLs render in the
//!   accent colour, underlined, and CLICK (the system browser opens
//!   them — the same `ShellExecuteW` path the download-page link uses);
//!   `**bold**` renders on the primary text colour.
//!
//! Deliberately NOT implemented (matching the web dialect): ordered
//! lists, code blocks, nested lists, italics — numbered legal clauses
//! like "1. 定义" must survive as literal prose.

use eframe::egui::{self, Align, Color32, CornerRadius, FontId, Sense, Stroke, TextFormat, Vec2};

use crate::fallback::Theme;

/// One inline run of a source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Seg {
    Text(String),
    Strong(String),
    Link { label: String, url: String },
}

/// Sentence / bracket punctuation that closes a sentence around a bare
/// URL — trimmed off the link so the target stays clean (the web's
/// TRAILING_PUNCT, full-width CJK closers included).
const TRAILING_PUNCT: &[char] = &[
    ')', ']', '}', '.', ',', ';', ':', '!', '?', '\'', '"', '，', '。', '；', '：', '！', '？',
    '）', '】', '》', '’', '”',
];

/// A markdown link starting at `rest`: `[label](https://…)`.
fn md_link_at(rest: &str) -> Option<usize> {
    let open = rest.find('[')?;
    let after_open = &rest[open + 1..];
    let close = after_open.find(']')?;
    let label = &after_open[..close];
    let after_close = &after_open[close + 1..];
    let paren = after_close.strip_prefix('(')?;
    let end = paren.find(')')?;
    let url = &paren[..end];
    if label.is_empty() || !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    Some(open)
}

/// A `**bold**` run starting at `rest`; the content itself carries no
/// asterisks (the web dialect's same restriction), and an empty run is
/// not bold.
fn bold_at(rest: &str) -> Option<usize> {
    let open = rest.find("**")?;
    let after = &rest[open + 2..];
    let close = after.find("**")?;
    (!after[..close].is_empty()).then_some(open)
}

/// A bare http(s) URL starting at `rest`, with the trailing sentence
/// punctuation excluded.
fn bare_url_at(rest: &str) -> Option<(usize, usize)> {
    let http = rest.find("http://");
    let https = rest.find("https://");
    let start = match (http, https) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let tail = &rest[start..];
    let end = tail
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '&' | '"' | '\''))
        .unwrap_or(tail.len());
    let mut trimmed = &tail[..end];
    while trimmed
        .chars()
        .last()
        .is_some_and(|c| TRAILING_PUNCT.contains(&c))
    {
        trimmed = &trimmed[..trimmed.len() - trimmed.chars().last().unwrap().len_utf8()];
    }
    (!trimmed.is_empty()).then_some((start, start + trimmed.len()))
}

/// Splits one source line into inline runs — the web's transform order
/// (markdown link, then bold, then bare URL), left to right.
pub(crate) fn parse_inline(line: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let link = md_link_at(rest);
        let bold = bold_at(rest);
        let url = bare_url_at(rest);
        // Earliest match wins; a tie prefers the explicit markdown link.
        let earliest = [
            link.map(|i| (i, 0u8)),
            bold.map(|i| (i, 1)),
            url.map(|(i, _)| (i, 2)),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some((start, kind)) = earliest else {
            segs.push(Seg::Text(rest.to_string()));
            break;
        };
        if start > 0 {
            segs.push(Seg::Text(rest[..start].to_string()));
        }
        let consumed = match kind {
            0 => {
                let open = start;
                let after_open = &rest[open + 1..];
                let close = after_open.find(']').unwrap();
                let label = after_open[..close].to_string();
                let paren = &after_open[close + 2..];
                let end = paren.find(')').unwrap();
                let url = paren[..end].to_string();
                segs.push(Seg::Link { label, url });
                open + 1 + close + 2 + end + 1
            }
            1 => {
                let open = start;
                let after = &rest[open + 2..];
                let close = after.find("**").unwrap();
                segs.push(Seg::Strong(after[..close].to_string()));
                open + 2 + close + 2
            }
            _ => {
                let (from, to) = bare_url_at(rest).unwrap();
                let url = rest[from..to].to_string();
                segs.push(Seg::Link {
                    label: url.clone(),
                    url,
                });
                to
            }
        };
        rest = &rest[consumed..];
    }
    segs
}

fn format_for(color: Color32, underline: bool, size: f32, mono: bool) -> TextFormat {
    let font_id = if mono {
        FontId::monospace(size)
    } else {
        FontId::proportional(size)
    };
    TextFormat {
        font_id,
        color,
        underline: if underline {
            Stroke::new(1.0f32, color)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    }
}

/// Paints one inline run as a wrapped row, with clickable links.
fn paint_segments(
    ui: &mut egui::Ui,
    theme: &Theme,
    segs: &[Seg],
    size: f32,
    color: Color32,
    indent: f32,
) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = (ui.available_width() - indent).max(48.0);
    for seg in segs {
        match seg {
            Seg::Text(text) => {
                job.append(text, 0.0, format_for(color, false, size, false));
            }
            Seg::Strong(text) => {
                job.append(text, 0.0, format_for(theme.text, false, size, false));
            }
            Seg::Link { label, .. } => {
                job.append(label, 0.0, format_for(theme.primary, true, size, false));
            }
        }
    }
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let mut link_rects: Vec<(egui::Rect, String)> = Vec::new();
    // Link hit regions: locate each link's label inside the laid-out
    // rows (the label text is the marker; a label crossing a wrap is
    // split across the rows it touches).
    for row in &galley.rows {
        let row_text: String = row.glyphs.iter().map(|g| g.chr).collect();
        for seg in segs {
            let Seg::Link { label, url } = seg else {
                continue;
            };
            if let Some(at) = row_text.find(label.as_str()) {
                let start = at;
                let end = at + label.chars().count();
                let first = row.glyphs.get(start);
                let last = row.glyphs.get(end.saturating_sub(1));
                if let (Some(first), Some(last)) = (first, last) {
                    let y0 = row.pos.y;
                    let x0 = first.pos.x;
                    let x1 = last.pos.x + last.advance_width;
                    link_rects.push((
                        egui::Rect::from_min_max(
                            egui::pos2(x0, y0),
                            egui::pos2(x1, y0 + row.height()),
                        ),
                        url.clone(),
                    ));
                }
            }
        }
    }

    ui.horizontal(|ui| {
        if indent > 0.0 {
            ui.add_space(indent);
        }
        let size = Vec2::new(galley.size().x.max(1.0), galley.size().y);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let painter = ui.painter_at(rect);
        painter.galley(rect.min, galley.clone(), color);
        let pointer = response
            .hover_pos()
            .or_else(|| response.interact_pointer_pos());
        let hovered_link = pointer.and_then(|pos| {
            let local = egui::pos2(pos.x - rect.min.x, pos.y - rect.min.y);
            link_rects
                .iter()
                .find(|(hit, _)| hit.contains(local))
                .map(|(_, url)| url.clone())
        });
        if let Some(url) = hovered_link {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                crate::fallback::open_external(&url);
            }
        }
    });
}

/// Renders one markdown document into `ui` — the shared path behind the
/// license pane's bodies and the content steps.
pub(crate) fn render(ui: &mut egui::Ui, theme: &Theme, source: &str, base_size: f32) {
    let body_color = theme.text_secondary;
    for raw in source.split('\n') {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(6.0);
            continue;
        }
        if trimmed.chars().count() >= 3 && trimmed.chars().all(|c| c == '-' || c == '=') {
            ui.add_space(4.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, theme.border);
            ui.add_space(4.0);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            paint_head(ui, theme, rest, 13.0);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            paint_head(ui, theme, rest, 14.0);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            paint_head(ui, theme, rest, 16.0);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("> ") {
            let segs = parse_inline(rest);
            paint_segments(ui, theme, &segs, base_size, body_color, 8.0);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            let mut segs = vec![Seg::Text("•  ".to_string())];
            segs.extend(parse_inline(rest));
            paint_segments(ui, theme, &segs, base_size, body_color, 12.0);
            continue;
        }
        let segs = parse_inline(trimmed);
        paint_segments(ui, theme, &segs, base_size, body_color, 0.0);
    }
}

fn paint_head(ui: &mut egui::Ui, theme: &Theme, text: &str, size: f32) {
    let segs = parse_inline(text);
    let _ = Align::LEFT;
    paint_segments(ui, theme, &segs, size, theme.text, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_runs_split_links_bold_and_plain() {
        let segs =
            parse_inline("- **GLM 5.3 / 5.3 Flash** — 智谱 AI — [zhipu.ai](https://zhipu.ai)");
        assert_eq!(
            segs,
            vec![
                Seg::Text("- ".into()),
                Seg::Strong("GLM 5.3 / 5.3 Flash".into()),
                Seg::Text(" — 智谱 AI — ".into()),
                Seg::Link {
                    label: "zhipu.ai".into(),
                    url: "https://zhipu.ai".into()
                },
            ]
        );
    }

    #[test]
    fn bare_urls_stop_at_punctuation() {
        let segs = parse_inline("协议全文见 https://github.com/celestia-island/sysl。");
        assert_eq!(
            segs,
            vec![
                Seg::Text("协议全文见 ".into()),
                Seg::Link {
                    label: "https://github.com/celestia-island/sysl".into(),
                    url: "https://github.com/celestia-island/sysl".into()
                },
                Seg::Text("。".into()),
            ]
        );
    }

    #[test]
    fn markdown_links_win_over_bare_detection() {
        let segs = parse_inline("见 [官网](https://zhipu.ai) 与 https://deepseek.ai");
        assert!(matches!(segs[1], Seg::Link { ref label, .. } if label == "官网"));
        assert!(
            matches!(segs.last(), Some(Seg::Link { label, .. }) if label == "https://deepseek.ai")
        );
    }

    #[test]
    fn plain_prose_is_one_run() {
        let segs = parse_inline("1. 定义：本协议中的术语…");
        assert_eq!(segs, vec![Seg::Text("1. 定义：本协议中的术语…".into())]);
    }
}
