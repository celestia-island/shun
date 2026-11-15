//! The in-app help window — the egui mirror of hikari's `HkModal`
//! (`shell/web/node_modules/@celestia-island/hikari/src/components/
//! HkModal.{tsx,scss}`).
//!
//! One window, two document families:
//!
//! - the **help topics** (`shell/guide/<topic>/<locale>.md`, the same files
//!   the webview face imports): one short, plain-language article per
//!   failure family the classifier names (`shun::help::TOPICS` —
//!   app-control, permissions, disk-space, file-in-use, path-too-long,
//!   network, missing-files). The app-control article additionally embeds
//!   its three schematic figures;
//! - the generic **error details** mode: the raw failure message, verbatim,
//!   with a copy action (the hikari error landing's built-in copy,
//!   egui-side).
//!
//! In `[shell] guides = "online"` products the face never opens this
//! window for a topic — it hands the official page to the browser instead
//! (`shun::help::open_url`); the details family remains (`open_help` in
//! fallback.rs picks between them). The build materializes one-line
//! placeholders for online products, so the embedded loader stays
//! compilable but is never reachable in that mode.
//!
//! Visual contract (HkModal.scss, desktop):
//!
//! ```text
//! ░░░░░░░░░░░░░░ dim curtain over the WHOLE window (rgba(0,0,0,.45)) ░░░
//! ░  ┌────────────────────── 640pt ─────────────────────────┐          ░
//! ░  │ Title (17pt, 700)                              ✕ (28²)│  header  │
//! ░  ├───────────────────────────────────────────────────────┤  hairline│
//! ░  │  body — scrolls when it overflows (max 82% of the     │          │
//! ░  │  window) — markdown with the embedded figures         │          │
//! ░  ├───────────────────────────────────────────────────────┤  hairline│
//! ░  │                              [Official page] [ Close ] │  footer  │
//! ░  └───────────────────────────────────────────────────────┘          ░
//! ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//! ```
//!
//! - the curtain dims the ENTIRE client area and owns every click that
//!   does not land on the card, so the wizard underneath (footer buttons
//!   included) cannot be clicked through a modal;
//! - card: 640pt wide (92% of the window at most), 82% tall at most,
//!   radius 16 (`--radius-xl`), surface fill + 1pt border, and hikari's
//!   `0 16px 48px rgba(0,0,0,.2)` drop shadow;
//! - Esc closes (HkModal's keydown arm), a click on the curtain closes, a
//!   click INSIDE the card never does (the card registers its own click
//!   blocker above the curtain);
//! - light and dark both derive from the shared `Theme` tokens, so the
//!   window follows `shell.theme.mode` exactly like every other surface;
//! - the footer's secondary action jumps to the official help page for the
//!   open topic (`shun::help::help_url`) when the OS/distro has one; the
//!   window stays open behind the browser. Topics with no page (a missing
//!   install file is about this product, not the OS) hide it, and the
//!   generic details window carries the "contact the developer" line
//!   instead.
//!
//! Deviations from hikari — only what egui genuinely cannot express:
//!
//! 1. No open/close choreography (hikari animates opacity + a 3% zoom over
//!    0.32–0.42 s). egui has no CSS-transition equivalent, and the window
//!    appears/disappears on the frame the action lands.
//! 2. No 700 title weight: this face loads no bold face (the same limit
//!    every `strong()` label in the wizard works around — `strong()` only
//!    shifts the color). The title's hierarchy comes from its 17pt size and
//!    the primary text color, matching HkModal's proportions as closely as
//!    the font stack allows.

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Id, Layout, Rect, Sense, Stroke, Vec2, pos2, vec2,
};

use crate::fallback::{Theme, hairline_box};
use crate::markdown::{self, ImageBank};

/// The ten installer locales, in `wizard-strings.json`'s order — the
/// documents `app_control_source` embeds (the coverage tests sweep this).
#[cfg(test)]
const LOCALES: [&str; 10] = [
    "zh-Hans", "zh-Hant", "en", "ru", "ja", "ko", "fr", "es", "de", "pt",
];

/// hikari's window width (`width="lg"`).
const CARD_MAX_W: f32 = 640.0;
/// Never wider than this share of the window (the brief's 92%).
const CARD_MAX_W_RATIO: f32 = 0.92;
/// Never taller than this share of the window (the brief's 82%; hikari caps
/// the BODY at 70vh and lets the frame hug its content).
const CARD_MAX_H_RATIO: f32 = 0.82;
/// `border-radius: var(--radius-xl)`.
const CARD_RADIUS: u8 = 16;
/// `--hk-modal-padding-header: 1rem 1.5rem` roughly; content gutters 24.
const GUTTER: f32 = 24.0;
const HEADER_H: f32 = 56.0;
const FOOTER_H: f32 = 56.0;
/// The window close plate's box.
const CLOSE_BOX: f32 = 28.0;
/// `rgba(0, 0, 0, 0.45)`.
const SCRIM_ALPHA: u8 = 115;

/// The generic details window's content — also the payload a toast action
/// carries so an online-mode product can still show the raw message when
/// the topic has no official page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuideDetails {
    pub title: String,
    pub body: String,
}

/// Which document the window shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GuideDoc {
    /// An embedded help topic — one of `shun::help::TOPICS`.
    Topic(String),
    /// A raw failure message — the generic "error details" window.
    Details(GuideDetails),
}

/// The window's localized actions, handed in by the face (the guide module
/// owns the chrome, the app owns the copy).
pub(crate) struct GuideLabels<'a> {
    /// The footer's primary action (`notice_close`).
    pub close: &'a str,
    /// The details window's copy action (`error_copy`).
    pub copy: &'a str,
    /// The footer's secondary action (`notice_open_official`) — hidden when
    /// the topic has no official page on this platform.
    pub open_official: &'a str,
    /// The details window's footer lead (`notice_contact_developer`): the
    /// raw message with no article and no official page still needs a next
    /// step.
    pub contact_developer: &'a str,
}

/// The guide/window state. One instance lives in the wizard app; the
/// texture cache inside it is what keeps the figures decoded once.
pub(crate) struct GuideWindow {
    open: Option<GuideDoc>,
    images: ImageBank,
    /// The official help page for the open topic, resolved ONCE when the
    /// window opens (`help_url` probes `/etc/os-release` on Linux, so it
    /// must never run per frame).
    official_url: Option<String>,
}

impl GuideWindow {
    pub(crate) fn new() -> Self {
        Self {
            open: None,
            images: ImageBank::new(),
            official_url: None,
        }
    }

    /// Whether a document is showing. The app itself does not need it (it
    /// always draws, and `draw` is a no-op when closed); the tests and the
    /// screenshot hook read it.
    #[allow(dead_code)]
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
        self.official_url = None;
    }

    /// Opens the embedded guide for `topic`, resolving the topic's official
    /// page for this OS / distribution / locale at the same time. An
    /// unknown topic still opens (the English article is the catch-all)
    /// rather than failing into a dead action.
    pub(crate) fn open_topic(&mut self, topic: &str, language: &str) {
        self.open = Some(GuideDoc::Topic(topic.to_string()));
        self.official_url = shun::help::help_url(topic, language);
    }

    /// Opens the generic details window with a title and the raw body.
    pub(crate) fn open_details(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.open = Some(GuideDoc::Details(GuideDetails {
            title: title.into(),
            body: body.into(),
        }));
        // A raw message has no topic to look up.
        self.official_url = None;
    }

    /// Paints the modal (screen-space, above every panel). `language` picks
    /// the guide document; `labels` are the localized footer / copy actions.
    pub(crate) fn draw(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        language: &str,
        labels: &GuideLabels<'_>,
    ) {
        let Some(doc) = self.open.clone() else {
            return;
        };
        // HkModal's keydown arm: a closable window consumes Escape.
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close();
            return;
        }
        let screen = ctx.screen_rect();
        if screen.width() < 120.0 || screen.height() < 120.0 {
            return;
        }

        // The document: the authored title is the first `# ` line, the rest
        // is the body. Details mode takes the caller's title + raw message.
        let (title, body) = match &doc {
            GuideDoc::Topic(topic) => {
                let (heading, body) = split_document(guide_source(topic, language));
                (heading.unwrap_or_else(|| topic.clone()), body)
            }
            GuideDoc::Details(details) => (details.title.clone(), details.body.as_str()),
        };
        // The official-page jump belongs to topical documents only.
        let official = match &doc {
            GuideDoc::Topic(_) => self.official_url.clone(),
            GuideDoc::Details(_) => None,
        };

        let card_width = (screen.width() * CARD_MAX_W_RATIO).min(CARD_MAX_W);
        let max_height = screen.height() * CARD_MAX_H_RATIO;
        // The guide is a long document: it takes the full allowed height
        // and scrolls. The details window hugs its message (hikari lets the
        // frame hug its content) and only reaches the cap on a wall of text.
        let card_height = match &doc {
            GuideDoc::Topic(_) => max_height,
            GuideDoc::Details(_) => {
                let inner = card_width - 2.0 * GUTTER;
                let galley = ctx.fonts(|fonts| {
                    fonts.layout(
                        body.to_owned(),
                        egui::FontId::proportional(13.5),
                        theme.text,
                        inner,
                    )
                });
                // header + message + the copy row + footer, with the body's
                // own 20pt paddings.
                (HEADER_H + galley.size().y + 20.0 + 36.0 + 12.0 + FOOTER_H)
                    .clamp(220.0, max_height)
            }
        };
        let card = Rect::from_center_size(screen.center(), vec2(card_width, card_height));

        let mut close_requested = false;
        egui::Area::new(Id::new("shun-guide-modal"))
            // Foreground: above every panel (the toast stack is a later
            // Foreground area, so notices still paint above this window —
            // hikari's popup manager band order).
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(true)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                // Cloned, not borrowed: the child uis below need `&mut ui`.
                let painter = ui.painter().clone();

                // ── The dim curtain over the whole window ────────────
                painter.rect_filled(
                    screen,
                    CornerRadius::ZERO,
                    Color32::from_black_alpha(SCRIM_ALPHA),
                );
                // Registered FIRST: the card's blocker and its widgets below
                // are later widgets in the same layer, so they win the hit
                // test — a click inside the card never reaches this one.
                let backdrop = ui.interact(screen, Id::new("shun-guide-backdrop"), Sense::click());
                if backdrop.clicked() {
                    close_requested = true;
                }

                // ── The card ─────────────────────────────────────────
                painter.add(
                    egui::epaint::Shadow {
                        offset: [0, 16],
                        blur: 48,
                        spread: 0,
                        color: Color32::from_black_alpha(51),
                    }
                    .as_shape(card, CornerRadius::same(CARD_RADIUS)),
                );
                hairline_box(&painter, card, CARD_RADIUS, theme.border, theme.surface);
                // The card's own click blocker: a press on the window's
                // background must not close it (HkModal's content onClick
                // stopPropagation).
                let _ = ui.interact(card, Id::new("shun-guide-card"), Sense::click());

                // ── Header: title + close plate + hairline ───────────
                let header = Rect::from_min_size(card.min, vec2(card.width(), HEADER_H));
                let close = Rect::from_center_size(
                    pos2(header.right() - GUTTER - CLOSE_BOX / 2.0, header.center().y),
                    Vec2::splat(CLOSE_BOX),
                );
                let close_response = ui
                    .interact(close, Id::new("shun-guide-close"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if close_response.clicked() {
                    close_requested = true;
                }
                paint_cross(
                    &painter,
                    close,
                    if close_response.hovered() {
                        theme.text
                    } else {
                        theme.text_secondary
                    },
                );
                // HkModal's title: one line, ellipsized when the card runs
                // out of room (`text-overflow: ellipsis`).
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = (close.left() - GUTTER - (card.left() + GUTTER)).max(80.0);
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                job.append(
                    &title,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(17.0),
                        color: theme.text,
                        ..Default::default()
                    },
                );
                let galley = ui.fonts(|fonts| fonts.layout_job(job));
                painter.galley(
                    pos2(
                        card.left() + GUTTER,
                        header.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    theme.text,
                );
                divider(&painter, card.left(), card.right(), header.bottom(), theme);

                // ── Body: scrollable document ────────────────────────
                let footer_top = card.bottom() - FOOTER_H;
                let body_rect = Rect::from_min_max(
                    pos2(card.left() + GUTTER, header.bottom() + 1.0),
                    pos2(card.right() - GUTTER, footer_top - 1.0),
                );
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(body_rect)
                        .layout(Layout::top_down(Align::LEFT)),
                    |ui| {
                        ui.set_width(body_rect.width());
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(body_rect.height())
                            .show(ui, |ui| {
                                // HkModal's body padding: 1.5rem all around
                                // (the horizontal half is the rect's GUTTER).
                                ui.add_space(GUTTER - 4.0);
                                match &doc {
                                    GuideDoc::Topic(_) => markdown::render_with_images(
                                        ui,
                                        theme,
                                        body,
                                        13.5,
                                        &self.images,
                                    ),
                                    GuideDoc::Details(_) => {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(body)
                                                    .size(13.5)
                                                    .color(theme.text),
                                            )
                                            .wrap(),
                                        );
                                        ui.add_space(16.0);
                                        if footer_button(ui, theme, labels.copy).clicked() {
                                            ui.ctx().copy_text(body.to_owned());
                                        }
                                    }
                                }
                                ui.add_space(GUTTER - 4.0);
                            });
                    },
                );

                // ── Footer: close, plus the official page for a topic ──
                divider(&painter, card.left(), card.right(), footer_top, theme);
                // The details window has no article and no official page:
                // its footer lead is the "contact the developer" line, so a
                // raw message still ends with a next step.
                if matches!(&doc, GuideDoc::Details(_)) && !labels.contact_developer.is_empty() {
                    let galley = painter.layout_no_wrap(
                        labels.contact_developer.to_owned(),
                        egui::FontId::proportional(12.0),
                        theme.text_tertiary,
                    );
                    painter.galley(
                        pos2(
                            card.left() + GUTTER,
                            footer_top + (FOOTER_H - galley.size().y) / 2.0,
                        ),
                        galley,
                        theme.text_tertiary,
                    );
                }
                let footer_rect = Rect::from_min_max(pos2(card.left(), footer_top), card.max);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(footer_rect)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        ui.add_space(GUTTER);
                        if footer_button(ui, theme, labels.close).clicked() {
                            close_requested = true;
                        }
                        // The secondary action sits inline-start of the
                        // close button (right-to-left layout: added after
                        // it). The window STAYS open — the browser takes
                        // focus, and the article stays where the user left
                        // it when they come back.
                        if let Some(url) = &official {
                            if !labels.open_official.is_empty()
                                && footer_ghost_button(ui, theme, labels.open_official).clicked()
                            {
                                shun::help::open_url(url);
                            }
                        }
                    },
                );
            });

        if close_requested {
            self.close();
        }
    }
}

/// The footer's button — hikari's `secondary` (surface fill, hairline,
/// 6pt radius); the same chrome the callout's action uses, so the two
/// surfaces cannot drift.
fn footer_button(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    let width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        )
        .size()
        .x
        + 32.0;
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(13.0).color(theme.text))
            .fill(theme.surface)
            .stroke(Stroke::new(1.0f32, theme.border))
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(width, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The footer's secondary action — hikari's `ghost` variant: bare accent
/// text on the window surface, no plate. Used for the official-page jump
/// next to the primary close button.
fn footer_ghost_button(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    let width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        )
        .size()
        .x
        + 24.0;
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(13.0).color(theme.primary))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(width, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The 1pt window divider (HkModal's header/footer rule).
fn divider(painter: &egui::Painter, left: f32, right: f32, y: f32, theme: &Theme) {
    painter.rect_filled(
        Rect::from_min_max(pos2(left + 1.0, y), pos2(right - 1.0, y + 1.0)),
        CornerRadius::ZERO,
        theme.border,
    );
}

/// The `✕` close plate — two strokes through the box (the lucide `x`; the
/// caption bar's close glyph is the same geometry).
fn paint_cross(painter: &egui::Painter, box_rect: Rect, color: Color32) {
    let half = 6.0;
    let center = box_rect.center();
    let stroke = Stroke::new(1.5f32, color);
    painter.line_segment(
        [
            pos2(center.x - half, center.y - half),
            pos2(center.x + half, center.y + half),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(center.x - half, center.y + half),
            pos2(center.x + half, center.y - half),
        ],
        stroke,
    );
}

/// The embedded guide document for `topic` + `language`. The tree is
/// materialized by build.rs into OUT_DIR (real bytes for an embedded
/// product, one-line placeholders for `guides = "online"` — the face
/// never renders those, it opens `shun::help::help_url` instead). Ten
/// locales, English as the catch-all.
pub(crate) fn guide_source(topic: &str, language: &str) -> &'static str {
    match (topic, language) {
        ("app-control", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/zh-Hans.md"))
        }
        ("app-control", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/zh-Hant.md"))
        }
        ("app-control", "en") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/en.md")),
        ("app-control", "ru") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/ru.md")),
        ("app-control", "ja") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/ja.md")),
        ("app-control", "ko") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/ko.md")),
        ("app-control", "fr") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/fr.md")),
        ("app-control", "es") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/es.md")),
        ("app-control", "de") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/de.md")),
        ("app-control", "pt") => include_str!(concat!(env!("OUT_DIR"), "/guide/app-control/pt.md")),
        ("permissions", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/zh-Hans.md"))
        }
        ("permissions", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/zh-Hant.md"))
        }
        ("permissions", "en") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/en.md")),
        ("permissions", "ru") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/ru.md")),
        ("permissions", "ja") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/ja.md")),
        ("permissions", "ko") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/ko.md")),
        ("permissions", "fr") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/fr.md")),
        ("permissions", "es") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/es.md")),
        ("permissions", "de") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/de.md")),
        ("permissions", "pt") => include_str!(concat!(env!("OUT_DIR"), "/guide/permissions/pt.md")),
        ("disk-space", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/zh-Hans.md"))
        }
        ("disk-space", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/zh-Hant.md"))
        }
        ("disk-space", "en") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/en.md")),
        ("disk-space", "ru") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/ru.md")),
        ("disk-space", "ja") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/ja.md")),
        ("disk-space", "ko") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/ko.md")),
        ("disk-space", "fr") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/fr.md")),
        ("disk-space", "es") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/es.md")),
        ("disk-space", "de") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/de.md")),
        ("disk-space", "pt") => include_str!(concat!(env!("OUT_DIR"), "/guide/disk-space/pt.md")),
        ("file-in-use", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/zh-Hans.md"))
        }
        ("file-in-use", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/zh-Hant.md"))
        }
        ("file-in-use", "en") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/en.md")),
        ("file-in-use", "ru") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/ru.md")),
        ("file-in-use", "ja") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/ja.md")),
        ("file-in-use", "ko") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/ko.md")),
        ("file-in-use", "fr") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/fr.md")),
        ("file-in-use", "es") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/es.md")),
        ("file-in-use", "de") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/de.md")),
        ("file-in-use", "pt") => include_str!(concat!(env!("OUT_DIR"), "/guide/file-in-use/pt.md")),
        ("path-too-long", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/zh-Hans.md"))
        }
        ("path-too-long", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/zh-Hant.md"))
        }
        ("path-too-long", "en") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/en.md"))
        }
        ("path-too-long", "ru") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/ru.md"))
        }
        ("path-too-long", "ja") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/ja.md"))
        }
        ("path-too-long", "ko") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/ko.md"))
        }
        ("path-too-long", "fr") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/fr.md"))
        }
        ("path-too-long", "es") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/es.md"))
        }
        ("path-too-long", "de") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/de.md"))
        }
        ("path-too-long", "pt") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/path-too-long/pt.md"))
        }
        ("network", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/network/zh-Hans.md"))
        }
        ("network", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/network/zh-Hant.md"))
        }
        ("network", "en") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/en.md")),
        ("network", "ru") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/ru.md")),
        ("network", "ja") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/ja.md")),
        ("network", "ko") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/ko.md")),
        ("network", "fr") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/fr.md")),
        ("network", "es") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/es.md")),
        ("network", "de") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/de.md")),
        ("network", "pt") => include_str!(concat!(env!("OUT_DIR"), "/guide/network/pt.md")),
        ("missing-files", "zh-Hans") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/zh-Hans.md"))
        }
        ("missing-files", "zh-Hant") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/zh-Hant.md"))
        }
        ("missing-files", "en") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/en.md"))
        }
        ("missing-files", "ru") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/ru.md"))
        }
        ("missing-files", "ja") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/ja.md"))
        }
        ("missing-files", "ko") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/ko.md"))
        }
        ("missing-files", "fr") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/fr.md"))
        }
        ("missing-files", "es") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/es.md"))
        }
        ("missing-files", "de") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/de.md"))
        }
        ("missing-files", "pt") => {
            include_str!(concat!(env!("OUT_DIR"), "/guide/missing-files/pt.md"))
        }
        // Unknown topic or locale: the English app-control article is the
        // last resort (an unknown topic's own "en" would be just as wrong
        // and harder to find).
        (_, _) => include_str!(concat!(env!("OUT_DIR"), "/guide/", "app-control", "/en.md")),
    }
}

/// [`guide_source`] narrowed to the app-control topic. The window itself is
/// topic-driven now (every family opens its own article), so this wrapper
/// survives only for the coverage tests, which read clearer through it.
#[cfg(test)]
pub(crate) fn app_control_source(language: &str) -> &'static str {
    guide_source(shun::help::TOPIC_APP_CONTROL, language)
}

/// Splits an authored document into the modal title and the body.
///
/// The contract with the authoring side: the FIRST line is `# <title>`, and
/// everything after it is the body. The split is defensive on purpose —
/// these files are authored by hand in ten languages:
///
/// - a UTF-8 BOM (a Windows editor's gift) is stripped before matching;
/// - CRLF line endings are accepted (the `\r` never reaches the title);
/// - a document WITHOUT the heading is not an error: the whole file is the
///   body and the caller supplies the title (the topic name), so a
///   malformed file degrades to readable content instead of panicking or
///   opening an empty window.
pub(crate) fn split_document(source: &str) -> (Option<String>, &str) {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let (first, rest) = match source.find('\n') {
        Some(end) => (&source[..end], &source[end + 1..]),
        None => (source, ""),
    };
    let first = first.trim_end_matches('\r').trim();
    if let Some(title) = first.strip_prefix("# ") {
        let title = title.trim();
        if !title.is_empty() {
            return (Some(title.to_string()), rest);
        }
    }
    (None, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every topic × locale resolves to a document the window can show.
    /// While an article is not authored yet build.rs materializes a
    /// one-line placeholder: that degrades to a titled-by-fallback window
    /// (the topic name), never a blank one.
    #[test]
    fn every_topic_locale_resolves_to_a_document() {
        for topic in shun::help::TOPICS {
            for language in LOCALES {
                let source = guide_source(topic, language);
                assert!(
                    !source.trim().is_empty(),
                    "{topic}/{language}: a document is always embedded"
                );
                let (title, body) = split_document(source);
                match title {
                    Some(title) => assert!(!title.trim().is_empty(), "{topic}/{language}: title"),
                    // The placeholder: one line, no heading. Authored
                    // articles always carry the `# ` line.
                    None => assert!(
                        body.lines().count() <= 2,
                        "{topic}/{language}: an untitled document must be the build placeholder"
                    ),
                }
            }
        }
    }

    /// The authored contract for the one fully-authored topic: the first
    /// line is the `# ` title, the rest is a non-empty body, and the three
    /// figures are referenced. (The other six topics are authored by the
    /// content lane; their coverage is the test above.)
    #[test]
    fn app_control_keeps_its_titled_article_and_figures() {
        for language in LOCALES {
            let (title, body) = split_document(app_control_source(language));
            let title =
                title.unwrap_or_else(|| panic!("{language}: the first line is `# <title>`"));
            assert!(!title.trim().is_empty(), "{language}: a written title");
            assert!(
                body.trim().len() > 200,
                "{language}: the body carries the guide ({} chars)",
                body.trim().len()
            );
            for figure in 1..=3 {
                let reference = format!("guide/app-control-{figure}.png");
                assert!(
                    body.contains(&reference),
                    "{language}: the body references {reference}"
                );
            }
        }
    }

    /// BOM + CRLF (a hand-authored file may carry either) split cleanly.
    #[test]
    fn the_title_split_tolerates_a_bom_and_crlf() {
        let (title, body) = split_document("\u{feff}# 标题\r\n\r\n第一段。\r\n第二行。\r\n");
        assert_eq!(title.as_deref(), Some("标题"));
        assert!(body.starts_with("\r\n第一段。"), "body={body:?}");
        assert!(body.contains("第二行。"));
    }

    /// A document without a heading (or with an empty one) never panics:
    /// the caller's fallback title wins and the whole text stays the body.
    #[test]
    fn a_missing_heading_degrades_to_body_only() {
        let (title, body) = split_document("no heading here\nsecond line");
        assert!(title.is_none());
        assert_eq!(body, "no heading here\nsecond line");

        let (title, body) = split_document("#\nbody");
        assert!(title.is_none(), "an empty heading is not a title");
        assert_eq!(body, "#\nbody");

        let (title, body) = split_document("");
        assert!(title.is_none());
        assert_eq!(body, "");
    }

    /// The window's lifecycle: open/close state only (drawing needs a pass
    /// loop, which the toast/markdown tests already exercise against the
    /// same egui APIs).
    #[test]
    fn the_window_tracks_its_document() {
        let mut window = GuideWindow::new();
        assert!(!window.is_open());
        window.open_topic(shun::help::TOPIC_APP_CONTROL, "en");
        assert!(window.is_open());
        assert_eq!(
            window.open,
            Some(GuideDoc::Topic(shun::help::TOPIC_APP_CONTROL.to_string()))
        );
        window.close();
        assert!(!window.is_open());
        assert!(window.official_url.is_none(), "closing drops the page");
        window.open_details("Launch failed", "io error: 4551");
        assert!(window.is_open());
        assert_eq!(
            window.open,
            Some(GuideDoc::Details(GuideDetails {
                title: "Launch failed".into(),
                body: "io error: 4551".into()
            }))
        );
        assert!(
            window.official_url.is_none(),
            "a raw message has no topic to look up"
        );
    }

    /// The window resolves and caches the topic's official page exactly as
    /// the core names it (`help_url` for the RUNNING host — so the
    /// assertion stays host-independent by comparing against the same
    /// call), and a raw message has no topic to look up.
    #[test]
    fn the_window_caches_the_topics_official_page() {
        let mut window = GuideWindow::new();
        for topic in shun::help::TOPICS {
            window.open_topic(topic, "en");
            assert_eq!(
                window.official_url,
                shun::help::help_url(topic, "en"),
                "{topic}: the window resolves the page the core names"
            );
        }
        window.open_details("x", "y");
        assert!(
            window.official_url.is_none(),
            "a raw message has no topic to look up"
        );
        window.close();
        assert!(window.official_url.is_none(), "closing drops the page");
    }

    /// The footer action hides where the vendor hosts no page: none of the
    /// three OS tables has one for a missing install file (that failure is
    /// about this product, not the OS). The PURE resolver takes the OS and
    /// distro explicitly, so this holds on every host running the tests.
    #[test]
    fn topics_without_an_official_page_hide_the_footer_action() {
        for os in ["windows", "macos", "linux"] {
            assert_eq!(
                shun::help::help_url_for(shun::help::TOPIC_MISSING_FILES, "en", os, None),
                None,
                "{os}: no vendor page for a missing install file"
            );
        }
        // A topic the host DOES document still resolves.
        assert!(
            shun::help::help_url_for(shun::help::TOPIC_APP_CONTROL, "en", "windows", None)
                .is_some(),
            "the app-control remedy is a Microsoft page"
        );
    }

    /// Measures one article's rendered height at the modal's body width
    /// (640pt card − 2×24pt gutters − 8pt scrollbar).
    fn article_height(topic: &str, language: &str) -> f32 {
        let ctx = egui::Context::default();
        let theme = Theme::dark(None);
        let (_, body) = split_document(guide_source(topic, language));
        // Two passes: a fresh context has no font metrics on pass one.
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |_ui| {});
        });
        let mut height = None;
        let mut pending = Some(());
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(608.0);
                if pending.take().is_some() {
                    let top = ui.next_widget_position().y;
                    markdown::render_with_images(ui, &theme, body, 13.5, &ImageBank::new());
                    height = Some(ui.next_widget_position().y - top);
                }
            });
        });
        height.expect("the pass loop ran")
    }

    /// Audit (the round's item 8): every topic × locale renders inside the
    /// modal's body column, and the longest article needs the scroll
    /// viewport rather than growing the card past its 82% cap. `cargo test
    /// -- --nocapture` prints the measured heights.
    #[test]
    fn every_topic_article_renders_and_the_longest_scrolls() {
        // A 600pt-tall window (the wizard's own size): 82% card, minus the
        // 56pt header, 56pt footer and the body's 20+20pt padding.
        let viewport = 600.0 * CARD_MAX_H_RATIO - HEADER_H - FOOTER_H - 40.0;
        let mut tallest = (String::new(), 0.0_f32);
        let mut per_topic = Vec::new();
        for topic in shun::help::TOPICS {
            let mut topic_max = (String::new(), 0.0_f32);
            for language in LOCALES {
                let height = article_height(topic, language);
                assert!(height > 0.0, "{topic}/{language}: renders");
                assert!(
                    height < 6000.0,
                    "{topic}/{language}: the article stays article-sized ({height})"
                );
                if height > topic_max.1 {
                    topic_max = (language.to_string(), height);
                }
                if height > tallest.1 {
                    tallest = (format!("{topic}/{language}"), height);
                }
            }
            per_topic.push(format!("{topic} {:.0}pt ({})", topic_max.1, topic_max.0));
        }
        eprintln!(
            "article heights at the modal body width — {} | tallest {} at {:.0}pt (scroll viewport {:.0}pt)",
            per_topic.join(", "),
            tallest.0,
            tallest.1,
            viewport
        );
        assert!(
            tallest.1 > viewport,
            "the longest article ({:.0}pt) exercises the scroll body (viewport {viewport:.0}pt)",
            tallest.1
        );
    }

    /// An unknown topic still resolves to the English document — a future
    /// failure kind whose topic nobody localized must not open a blank
    /// window.
    #[test]
    fn unknown_topics_still_render_the_english_guide() {
        let english = app_control_source("en");
        assert_eq!(guide_source("xx-unknown", "en"), english);
        assert_eq!(guide_source("", ""), english);
        assert!(!english.is_empty());
        // Every KNOWN topic keeps its own English article instead of the
        // catch-all (the interim matrix sent them all to app-control).
        for topic in shun::help::TOPICS {
            let own = guide_source(topic, "en");
            if topic == shun::help::TOPIC_APP_CONTROL {
                continue;
            }
            assert_ne!(own, english, "{topic}/en must not serve app-control");
        }
    }
}
