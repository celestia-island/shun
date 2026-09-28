//! The install terminal — a collapsible monospace output pane fed by
//! the flow's structured log events (`shun::flow::FlowLog`).
//!
//! Same-origin with the hikari web terminal (`shell/web/src/components`):
//! the neighboring external-UI repo reuses the pair for remote-SSH
//! device consoles and monitoring/reboot landing pages (hover a binary,
//! see its log). Keep this component self-contained — state plus
//! render, zero wizard knowledge — so it lifts cleanly.

use egui::{Align, CornerRadius, Frame, Layout, Response, ScrollArea, Sense, Stroke, Ui, Vec2};

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
}

/// One rendered row's height (12pt monospace + leading).
const ROW_H: f32 = 15.0;
/// The expanded body's cap — the web pane clips at 240px; 180pt is the
/// same proportion against the wizard's taller window.
const BODY_MAX: f32 = 180.0;
/// The collapsed strip (header row + frame chrome).
const HEADER_H: f32 = 34.0;
/// Frame inner margin + stroke allowance, both sides.
const CHROME_H: f32 = 20.0;

/// Terminal state + renderer. The pane collapses behind its header row;
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
    /// A terminal that starts `open` or collapsed. Both faces default
    /// collapsed (the web pane's `expanded = false`); error lines force
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
    /// even from collapsed (the web pane's `logExpanded` rule).
    pub fn push(&mut self, kind: LineKind, text: String) {
        if matches!(kind, LineKind::Error) {
            self.open = true;
        }
        self.lines.push(Line { kind, text });
        let len = self.lines.len();
        if len > MAX_LINES {
            self.lines.drain(0..len - MAX_LINES);
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.seen = 0;
    }

    /// The card's exact height in its CURRENT state — header plus, when
    /// expanded, the body capped at [`BODY_MAX`]. Callers reserve this
    /// before centering anything around the card (the running pane pins
    /// the strip to its bottom edge), so the card can never push past
    /// the pane or overlap the progress block.
    pub fn height_hint(&self) -> f32 {
        let body = if self.open {
            (self.lines.len() as f32 * ROW_H + 10.0).min(BODY_MAX)
        } else {
            0.0
        };
        HEADER_H + body + CHROME_H
    }

    /// Renders the collapsible pane; `title`/`expand`/`collapse` come
    /// from the caller's i18n table.
    /// One slim card: the header row (toggle + count), and — only when
    /// expanded — the capped scroll body inside the SAME frame, so a
    /// collapsed log collapses to a 34pt strip instead of an empty
    /// stretched box. Content draws from the card's TOP-LEFT (the web
    /// pane's reading order); the card itself is sized by an EXACT
    /// rect — egui's auto-sized frames inherit whatever max_rect drifts
    /// in from the surrounding pane, which measured full-window-wide
    /// and past the bottom edge.
    pub fn render(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        title: &str,
        expand: &str,
        collapse: &str,
    ) -> Response {
        let width = ui.available_width();
        let (mut card, _) =
            ui.allocate_exact_size(Vec2::new(width, self.height_hint()), Sense::hover());
        // Never past the pane: the caller's flow can leave a few points
        // of estimation error (the running pane reserves the strip from
        // an ESTIMATED progress-block height), and the card must stay
        // inside the window — clamp its bottom to the available rect's
        // bottom edge.
        let bottom = ui.max_rect().bottom().min(card.bottom());
        card.set_bottom(bottom);
        card.set_top((bottom - self.height_hint()).max(ui.max_rect().top()));
        let response = ui.interact(card, ui.id().with("terminal-card"), Sense::hover());
        ui.allocate_new_ui(
            egui::UiBuilder::new()
                .max_rect(card)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                // Tight chrome metrics inside the card: the face's
                // uniform 36pt interact height would balloon the header
                // row and give every log line a 36pt band with the text
                // floating centered in it — the exact misrender this
                // component shipped with.
                ui.spacing_mut().interact_size.y = 16.0;
                Frame::default()
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0f32, theme.border))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(card.width() - 20.0);
                        // `ui.horizontal` — NOT `with_layout(left_to_right)`:
                        // the latter hands the child the FULL remaining
                        // rect (212pt of it here), pushing the body below
                        // the card and out the window — the exact
                        // misrender this component shipped with.
                        ui.horizontal(|ui| {
                            let label = if self.open { collapse } else { expand };
                            let toggle = ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .size(12.0)
                                            .color(theme.text_secondary),
                                    )
                                    .frame(false),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if toggle.clicked() {
                                self.open = !self.open;
                                if self.open {
                                    self.seen = self.lines.len();
                                }
                            }
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!("{title} · {}", self.lines.len()))
                                    .size(12.0)
                                    .color(theme.text_tertiary),
                            );
                        });
                        if self.open {
                            ui.add_space(6.0);
                            let body_h = self.height_hint() - HEADER_H - CHROME_H;
                            egui::ScrollArea::vertical()
                                .id_salt("install-terminal")
                                .auto_shrink([false, false])
                                .max_height(body_h.max(30.0))
                                .show(ui, |ui| {
                                    ui.set_width(card.width() - 20.0);
                                    // Dense log rows: no inter-row gaps (the
                                    // web pane's lines are flush).
                                    ui.spacing_mut().item_spacing.y = 1.0;
                                    if self.lines.is_empty() {
                                        ui.label(
                                            egui::RichText::new("…")
                                                .size(12.0)
                                                .color(theme.text_tertiary),
                                        );
                                        return;
                                    }
                                    let fresh = self.seen != self.lines.len();
                                    self.seen = self.lines.len();
                                    // Newest-first renders the list reversed
                                    // and pins the fresh end at the TOP;
                                    // oldest-first stays chronological and
                                    // pins the tail — the web pane's two
                                    // orders, verbatim.
                                    let rows: Vec<&Line> = if self.oldest_first {
                                        self.lines.iter().collect()
                                    } else {
                                        self.lines.iter().rev().collect()
                                    };
                                    let mut first: Option<Response> = None;
                                    let mut last: Option<Response> = None;
                                    for line in rows {
                                        let (glyph, color) = match line.kind {
                                            LineKind::Echo => (" ", theme.terminal_fg()),
                                            LineKind::Step => ("»", theme.text_secondary),
                                            LineKind::Ok => ("·", theme.success),
                                            LineKind::Error => ("×", theme.error),
                                        };
                                        let row = ui
                                            .horizontal(|ui| {
                                                ui.monospace(
                                                    egui::RichText::new(glyph)
                                                        .size(12.0)
                                                        .color(color),
                                                );
                                                ui.monospace(
                                                    egui::RichText::new(&line.text)
                                                        .size(12.0)
                                                        .color(color),
                                                );
                                            })
                                            .response;
                                        first = first.or_else(|| Some(row.clone()));
                                        last = Some(row);
                                    }
                                    if fresh {
                                        // Pin the FRESH end: the first row
                                        // when newest-first, the last when
                                        // chronological.
                                        if self.oldest_first {
                                            if let Some(last) = last {
                                                last.scroll_to_me(Some(Align::BOTTOM));
                                            }
                                        } else if let Some(first) = first {
                                            first.scroll_to_me(Some(Align::TOP));
                                        }
                                    }
                                });
                        }
                    });
            },
        );
        response
    }
}
