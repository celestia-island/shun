//! The install terminal — the web face's LogPane, egui-side: a
//! frameless activity strip below the progress block. Monospace lines
//! with HH:MM:SS stamps, colored per kind, fading with distance from
//! the fresh end; the header bar carries the title, a preview of the
//! newest line and the fold chevron. Same-origin with hikari's
//! LogPane (`shell/web/src/components/LogPane.tsx`) — keep the pair in
//! lockstep.

use egui::{Align, Color32, Response, ScrollArea, Sense, Stroke, Ui, Vec2};

use crate::fallback::Theme;

/// How a line is tinted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Plain echoed output (script stdout).
    Echo,
    /// A flow step marker (phase transitions, script begin).
    Step,
    /// A successful operation (file written, command done).
    Ok,
    /// A failure line.
    Error,
}

/// One terminal line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    kind: LineKind,
    text: String,
    /// Local wall-clock stamp (HH:MM:SS) at push time.
    hms: (u8, u8, u8),
}

/// The expanded body's cap — the web pane clips at 240px; 180pt is the
/// same proportion against the wizard's taller window.
const BODY_MAX: f32 = 180.0;
/// The header bar's height.
const HEADER_H: f32 = 26.0;

/// Local wall clock, HH:MM:SS. Windows reads GetLocalTime; everywhere
/// else falls back to UTC (the stamp is a decorative ordering aid).
#[cfg(windows)]
fn local_hms() -> (u8, u8, u8) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut now = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: plain call writing into our own zeroed struct.
    unsafe { GetLocalTime(&mut now) };
    (now.wHour as u8, now.wMinute as u8, now.wSecond as u8)
}

#[cfg(not(windows))]
fn local_hms() -> (u8, u8, u8) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() % 86_400)
        .unwrap_or(0);
    (
        (secs / 3600) as u8,
        ((secs / 60) % 60) as u8,
        (secs % 60) as u8,
    )
}

/// Terminal state + renderer. The pane folds behind its header bar;
/// new lines snap the view back to the fresh end (top when newest-first,
/// tail when oldest-first), quiet stretches leave the user's scroll
/// position alone. Scrollback is bounded.
pub struct Terminal {
    lines: Vec<Line>,
    /// Debug builds only: the visual-debug hook forces the drawer open.
    #[cfg(debug_assertions)]
    pub open: bool,
    #[cfg(not(debug_assertions))]
    open: bool,
    /// The manifest's `shell.log-order`: `true` renders chronologically
    /// (oldest first, tail-pinned); the default mirrors the web pane —
    /// newest first, top-pinned.
    oldest_first: bool,
    /// Line count at the last render — a delta means fresh output.
    seen: usize,
}

const MAX_LINES: usize = 2000;

impl Terminal {
    /// A terminal that starts `open` or folded. Both faces default
    /// folded (the web pane's `expanded = false`); error lines force
    /// the drawer open wherever they land.
    pub fn new(open: bool, oldest_first: bool) -> Self {
        Self {
            lines: Vec::new(),
            open,
            oldest_first,
            seen: 0,
        }
    }

    /// Appends a line. An error always surfaces: it opens the drawer
    /// even from folded (the web pane's `logExpanded` rule).
    pub fn push(&mut self, kind: LineKind, text: String) {
        if matches!(kind, LineKind::Error) {
            self.open = true;
        }
        self.lines.push(Line {
            kind,
            text,
            hms: local_hms(),
        });
        let len = self.lines.len();
        if len > MAX_LINES {
            self.lines.drain(0..len - MAX_LINES);
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.seen = 0;
    }

    /// The strip's exact height in its CURRENT state — header plus,
    /// when expanded, the body capped at [`BODY_MAX`]. Callers reserve
    /// this before laying out anything around the strip (the running
    /// pane pins it to its bottom edge), so it can never overlap the
    /// progress block or leave the window.
    pub fn height_hint(&self) -> f32 {
        let body = if self.open { BODY_MAX + 6.0 } else { 0.0 };
        HEADER_H + body
    }

    /// The fresh end's line — the header's preview text (the web
    /// pane's `previewText`).
    fn preview(&self) -> &str {
        if self.lines.is_empty() {
            return "";
        }
        if self.oldest_first {
            &self.lines[0].text
        } else {
            &self.lines[self.lines.len() - 1].text
        }
    }

    /// The web pane's fade mask: full strength at the fresh end,
    /// 72% at 55% depth, 26% at the far end — applied per line by
    /// distance (a per-text alpha ramp needs no background knowledge,
    /// unlike the CSS gradient overlay).
    fn fade(depth: f32) -> f32 {
        if depth <= 0.55 {
            1.0 - 0.28 * (depth / 0.55)
        } else {
            0.72 - 0.46 * ((depth - 0.55) / 0.45)
        }
    }

    /// Renders the strip; `title` comes from the caller's i18n table.
    /// Frameless by design (the web pane's look): the header bar is the
    /// click-to-fold surface carrying the title, a preview of the
    /// newest line and the fold chevron; the body scrolls without a
    /// scrollbar and clips at the cap.
    pub fn render(&mut self, ui: &mut Ui, theme: &Theme, title: &str) -> Response {
        let width = ui.available_width();
        let bar_resp = ui.allocate_exact_size(Vec2::new(width, HEADER_H), Sense::click());
        let bar = bar_resp.1.on_hover_cursor(egui::CursorIcon::PointingHand);
        let bar_rect = bar_resp.0;
        let painter = ui.painter();
        // Header: title · N left, newest-line preview center (clipped),
        // chevron right — the web bar's three spans.
        let count = self.lines.len();
        let head = format!("{title} · {count}");
        painter.text(
            bar_rect.left_top() + egui::vec2(0.0, 2.0),
            egui::Align2::LEFT_TOP,
            &head,
            egui::FontId::proportional(12.0),
            theme.text_tertiary,
        );
        let chev_w = 26.0;
        let head_w = painter
            .layout_no_wrap(head, egui::FontId::proportional(12.0), Color32::WHITE)
            .size()
            .x;
        let preview_rect = egui::Rect::from_min_max(
            egui::pos2(bar_rect.left() + head_w + 14.0, bar_rect.top() + 3.0),
            egui::pos2(bar_rect.right() - chev_w - 6.0, bar_rect.bottom()),
        );
        if preview_rect.width() > 20.0 {
            // Clip the preview to its span (the web span's
            // text-overflow ellipsis analog).
            let galley = painter.layout_no_wrap(
                self.preview().to_owned(),
                egui::FontId::monospace(11.0),
                theme.text_tertiary,
            );
            painter.with_clip_rect(preview_rect).galley(
                preview_rect.left_top(),
                galley,
                theme.text_tertiary,
            );
        }
        // The fold chevron (the lucide chevron-down/up analog), primary
        // on hover like the web toggle.
        let chev_zone = egui::Rect::from_min_max(
            egui::pos2(bar_rect.right() - chev_w, bar_rect.top()),
            egui::pos2(bar_rect.right(), bar_rect.bottom()),
        );
        let chev_hover = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| chev_zone.contains(p));
        let chev_color = if chev_hover {
            theme.primary
        } else {
            theme.text_secondary
        };
        let cx = chev_zone.center().x;
        let cy = bar_rect.center().y + 1.0;
        let stroke = Stroke::new(1.6, chev_color);
        let (a, b, c) = if self.open {
            (
                egui::pos2(cx - 4.0, cy - 2.0),
                egui::pos2(cx, cy + 2.0),
                egui::pos2(cx + 4.0, cy - 2.0),
            )
        } else {
            (
                egui::pos2(cx - 4.0, cy + 2.0),
                egui::pos2(cx, cy - 2.0),
                egui::pos2(cx + 4.0, cy + 2.0),
            )
        };
        painter.line_segment([a, b], stroke);
        painter.line_segment([b, c], stroke);
        if bar.clicked() {
            self.open = !self.open;
            if self.open {
                self.seen = self.lines.len();
            }
        }
        let mut last: Option<Response> = None;
        if self.open {
            ui.add_space(6.0);
            ScrollArea::vertical()
                .id_salt("install-terminal")
                .auto_shrink([false, false])
                .max_height(BODY_MAX)
                .scroll_bar_visibility(
                    egui::containers::scroll_area::ScrollBarVisibility::AlwaysHidden,
                )
                .show(ui, |ui| {
                    ui.set_width(width);
                    // Dense flush rows — the web pane's 4px gaps.
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if self.lines.is_empty() {
                        ui.label(
                            egui::RichText::new("…")
                                .size(11.0)
                                .color(theme.text_tertiary),
                        );
                        return;
                    }
                    let fresh = self.seen != self.lines.len();
                    self.seen = self.lines.len();
                    // Newest-first renders the list reversed and pins the
                    // fresh end at the TOP; oldest-first stays chronological
                    // and pins the tail — the web pane's two orders.
                    let rows: Vec<&Line> = if self.oldest_first {
                        self.lines.iter().collect()
                    } else {
                        self.lines.iter().rev().collect()
                    };
                    let total = rows.len().max(1) as f32;
                    for (index, line) in rows.iter().enumerate() {
                        // Distance from the fresh end drives the fade ramp.
                        let depth = index as f32 / total;
                        let alpha = Self::fade(depth);
                        let tint = |color: Color32| -> Color32 {
                            Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                (f32::from(color.a()) * alpha).min(255.0) as u8,
                            )
                        };
                        // The web line: a muted stamp + the kind-colored
                        // text (step/primary, ok/success, error, echo muted).
                        let text_color = match line.kind {
                            LineKind::Echo => theme.text_secondary,
                            LineKind::Step => theme.primary,
                            LineKind::Ok => theme.success,
                            LineKind::Error => theme.error,
                        };
                        let stamp =
                            format!("{:02}:{:02}:{:02}", line.hms.0, line.hms.1, line.hms.2);
                        let row = ui
                            .horizontal(|ui| {
                                ui.monospace(
                                    egui::RichText::new(stamp)
                                        .size(11.0)
                                        .color(tint(theme.text_tertiary)),
                                );
                                ui.add_space(8.0);
                                ui.monospace(
                                    egui::RichText::new(line.text.as_str())
                                        .size(11.0)
                                        .color(tint(text_color)),
                                );
                            })
                            .response;
                        last = Some(row);
                    }
                    if fresh {
                        // Pin the FRESH end: the first row when newest-first,
                        // the last when chronological.
                        if let Some(row) = last.clone() {
                            row.scroll_to_me(Some(if self.oldest_first {
                                Align::BOTTOM
                            } else {
                                Align::TOP
                            }));
                        }
                    }
                });
        }
        last.unwrap_or(bar)
    }
}
