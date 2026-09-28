//! The install terminal — a collapsible monospace output pane fed by
//! the flow's structured log events (`shun::flow::FlowLog`).
//!
//! Same-origin with the hikari web terminal (`shell/web/src/components`):
//! the neighboring external-UI repo reuses the pair for remote-SSH
//! device consoles and monitoring/reboot landing pages (hover a binary,
//! see its log). Keep this component self-contained — state plus
//! render, zero wizard knowledge — so it lifts cleanly.

use egui::{Align, CornerRadius, Frame, Layout, Response, ScrollArea, Stroke, Ui};

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

/// Terminal state + renderer. The pane collapses behind its header row;
/// new lines snap the view back to the fresh end (top when newest-first,
/// tail when oldest-first), quiet stretches leave the user's scroll
/// position alone. Scrollback is bounded.
pub struct Terminal {
    lines: Vec<Line>,
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

    /// Renders the collapsible pane; `title`/`expand`/`collapse` come
    /// from the caller's i18n table.
    /// One slim card: the header row (toggle + count), and — only when
    /// expanded — the capped scroll body inside the SAME frame, so a
    /// collapsed log collapses to a 34pt strip instead of an empty
    /// stretched box. Left-aligned content regardless of the pane's
    /// centered step alignment.
    pub fn render(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        title: &str,
        expand: &str,
        collapse: &str,
    ) -> Response {
        let body_h = 180.0f32;
        Frame::default()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
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
                    if toggle.clicked()
                    {
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
                    egui::ScrollArea::vertical()
                        .id_salt("install-terminal")
                        .auto_shrink([false, true])
                        .max_height(body_h)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
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
                            // Newest-first renders the list reversed and
                            // pins the fresh end at the TOP; oldest-first
                            // stays chronological and pins the tail —
                            // the web pane's two orders, verbatim.
                            let order = if self.oldest_first {
                                self.lines.iter().collect::<Vec<&Line>>()
                            } else {
                                self.lines.iter().rev().collect::<Vec<&Line>>()
                            };
                            let mut last: Option<Response> = None;
                            for line in order {
                                let (glyph, color) = match line.kind {
                                    LineKind::Echo => (" ", theme.terminal_fg()),
                                    LineKind::Step => ("»", theme.text_secondary),
                                    LineKind::Ok => ("·", theme.success),
                                    LineKind::Error => ("×", theme.error),
                                };
                                let row = ui
                                    .horizontal(|ui| {
                                        ui.monospace(
                                            egui::RichText::new(glyph).size(12.0).color(color),
                                        );
                                        ui.monospace(
                                            egui::RichText::new(&line.text).size(12.0).color(color),
                                        );
                                    })
                                    .response;
                                last = Some(row);
                            }
                            if fresh {
                                if let Some(last) = last {
                                    last.scroll_to_me(Some(if self.oldest_first {
                                        Align::BOTTOM
                                    } else {
                                        Align::TOP
                                    }));
                                }
                            }
                        });
                }
            })
            .response
    }
}
