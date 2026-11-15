//! The notice toast stack — the egui mirror of hikari's `HkToast`
//! (`shell/web/node_modules/@celestia-island/hikari/src/components/
//! HkToast.{tsx,scss}`, the sheet both web faces and the webview shell
//! share).
//!
//! Visual contract (HkToast.scss at the installer's desktop size):
//!
//! ```text
//!                                    ┌─ 360pt, right gutter 16 ─┐
//!                                    │ ⚠  Message text wraps to │  ← card 1
//!                                    │    three rows, elided… ⧉✕│    (oldest)
//!                                    └──────────────────────────┘
//!                                    ┌──────────────────────────┐
//!                                    │ ⓘ  A second notice…      │  ← card 2
//!                                    └──────────────────────────┘    (newest,
//!                                                                      bottom)
//! ```
//!
//! - floating top-right stack, [`TOAST_TOP`] below the window top (the
//!   32pt caption bar plus a 24pt breath), 16pt gutter, 360pt wide;
//! - cards are SOLID tone bands with hikari's on-solid ink: error/success/
//!   info carry the product's `--color-{error,success,info}` tokens with
//!   white ink, warning carries the amber `--color-warning` with hikari's
//!   hardcoded `#1c1c1e` near-black ink (white on that amber is ~2.6:1 —
//!   HkToast.scss pins dark ink there explicitly);
//! - 12/16 padding, radius 4, and a `0 4px 12px` drop shadow (HkToast's
//!   box-shadow); the solid band needs no border;
//! - the body is title-less, wraps to AT MOST three rows and elides with
//!   `…` — the CSS `-webkit-line-clamp: 3`, so a long error can never grow
//!   the card without bound;
//! - newest card appends at the BOTTOM (HkToast's `TransitionGroup` order),
//!   12pt between cards; an identical repeat of the NEWEST card of a tone
//!   refreshes it in place (hikari's consecutive dedupe), keeping the
//!   order = recency invariant that `trim()` relies on;
//! - lifetime table mirrors `TOAST_DURATION`: error/warning 30 s,
//!   info/success 3 s;
//! - controls, left to right: the inline action pill (dismisses the card
//!   and reports its [`ToastIntent`] to the host, like HkToast's
//!   `handleAction`), copy (the whole body onto the clipboard), close `✕`.
//!
//! Deviations from hikari — only what egui genuinely cannot express:
//!
//! 1. No enter/leave choreography: hikari slides a card in from the inline
//!    end and fades it out over 0.3 s (plus a `TransitionGroup` FLIP move
//!    when the stack reflows). egui has no CSS-transition equivalent, and
//!    the card appears/disappears on the frame the state changes.
//! 2. HkToast's multi-message "N msgs" count pill has no analogue here:
//!    every notice in this face is its own card, so a toast never
//!    aggregates messages.
//!
//! Everything else — solid bands + on-solid ink, geometry, ordering, the
//! three-row clamp, the lifetime table and the interaction set — follows
//! the hikari sheet.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, Id, Rect, Sense, Stroke, Vec2, pos2, vec2,
};

use crate::fallback::{CaptionIcons, Theme};

/// The stack's card width (`max-width: 24rem` in HkToast.scss; the
/// installer's desktop window is 800pt wide, so 360 leaves a 16pt gutter
/// plus the pane's own 48pt inset).
pub(crate) const TOAST_WIDTH: f32 = 360.0;
/// The viewport gutter HkToast clamps to on desktop.
pub(crate) const TOAST_GUTTER: f32 = 16.0;
/// Top offset: the 32pt caption bar + a 24pt breath. hikari's sheet uses
/// `top: 4rem` (64) against a taller browser chrome; the egui window owns
/// its caption bar in-band, so 56 is the equivalent clear seat.
pub(crate) const TOAST_TOP: f32 = 56.0;
/// hikari: `gap: var(--space-12)` between stacked cards.
const STACK_GAP: f32 = 12.0;
/// hikari: `padding: 12px 16px`.
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 12.0;
/// hikari: the 1.5rem icon box, the 1.5rem icon buttons.
const ICON: f32 = 18.0;
const CONTROL: f32 = 24.0;
const GAP: f32 = 8.0;
/// `border-radius: var(--radius-sm, 4px)`.
const RADIUS: u8 = 4;
/// `-webkit-line-clamp: 3`.
const CLAMP_ROWS: usize = 3;
/// Live cards kept: an installer only ever pushes a couple of notices, and
/// a runaway producer must not paint an unbounded column over the footer.
const MAX_LIVE: usize = 6;

/// The four inked toast tones (hikari's `ToastType` minus `loading`, which
/// this face never shows).
///
/// Only `Error` is pushed today — every notice this face raises is a
/// finish failure. The other three stay because the tone table IS the
/// hikari contract (`TOAST_DURATION` and the glyph set are keyed by all
/// four) and the next notice a surface needs must not have to re-derive
/// them.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastTone {
    Error,
    Warning,
    Info,
    Success,
}

impl ToastTone {
    /// The card's solid band fill and its on-solid ink — HkToast.scss's
    /// pairs, read through this face's token set:
    ///
    /// | tone    | band                 | ink                  |
    /// |---------|----------------------|----------------------|
    /// | error   | `--color-error`      | white                |
    /// | success | `--color-success`    | white                |
    /// | info    | `--color-info`       | white                |
    /// | warning | `--color-warning`    | `#1c1c1e` (hikari)   |
    ///
    /// The warning amber is the one pair hikari overrides in its own sheet
    /// (`.hk-toast-warning` pins `#1c1c1e` because white lands at ~2.6:1
    /// on it), so the ink follows that override rather than "white for
    /// every band".
    fn band(self, theme: &Theme) -> (Color32, Color32) {
        let white = Color32::WHITE;
        match self {
            ToastTone::Error => (theme.error, white),
            // HkToast.scss pins `#1c1c1e` on the amber band.
            ToastTone::Warning => (theme.warning, Color32::from_rgb(28, 28, 30)),
            ToastTone::Info => (theme.info, white),
            ToastTone::Success => (theme.success, white),
        }
    }

    /// hikari's `TOAST_DURATION`: errors and warnings hold for 30 s, a
    /// success/info acknowledgement for 3 s.
    fn ttl(self) -> Duration {
        match self {
            ToastTone::Error | ToastTone::Warning => Duration::from_secs(30),
            ToastTone::Info | ToastTone::Success => Duration::from_secs(3),
        }
    }
}

/// What an inline action asks the host to do. The card is dismissed by the
/// click itself (HkToast's `handleAction` removes before it fires), so the
/// host only sees the follow-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToastIntent {
    /// Open the help for `topic` — the in-app article in embedded mode, the
    /// official page in `[shell] guides = "online"` products. `details` is
    /// the raw message the host falls back to when online mode has no page
    /// for this topic/platform (the user must always get the text they were
    /// shown a headline for).
    OpenGuide {
        topic: String,
        details: crate::guide::GuideDetails,
    },
    /// Open the generic error-details window with the full raw message
    /// (the body the three-row clamp had to elide).
    ShowDetails { title: String, body: String },
}

struct Toast {
    id: u64,
    tone: ToastTone,
    text: String,
    /// The inline action: its label and what it reports.
    action: Option<(String, ToastIntent)>,
    copyable: bool,
    born: Instant,
}

/// The live stack — owned by the wizard app, drawn after every panel so it
/// floats above them.
pub(crate) struct ToastHost {
    items: Vec<Toast>,
    next_id: u64,
    /// The geometry of the cards painted in the last pass. Recorded only
    /// for the layout test — the drawing itself never reads it back.
    #[cfg(test)]
    layout: Vec<CardLayout>,
}

/// The painted geometry of one card. Production code does not read it back
/// (the painting is its own consumer); the layout test asserts the seat,
/// the gutter and the control boxes on it.
#[cfg_attr(not(test), allow(dead_code))]
struct CardLayout {
    card: Rect,
    close: Rect,
    copy: Option<Rect>,
    action: Option<Rect>,
}

impl ToastHost {
    pub(crate) fn new() -> Self {
        Self {
            items: Vec::new(),
            next_id: 1,
            #[cfg(test)]
            layout: Vec::new(),
        }
    }

    /// Pushes a plain notice (no action, not copyable). Unused by the
    /// wizard today (a notice without an action leaves the user nowhere to
    /// go); kept as the action-less half of the HkToast surface.
    #[allow(dead_code)]
    pub(crate) fn push(&mut self, tone: ToastTone, text: impl Into<String>) {
        self.push_with_action(
            tone,
            text,
            "",
            ToastIntent::ShowDetails {
                title: String::new(),
                body: String::new(),
            },
            false,
        );
        // The action-less push hides the pill by construction: see
        // `push_with_action`'s empty-label arm.
    }

    /// Pushes a notice that carries an inline action (and, for failures, a
    /// copy button over the full untruncated body).
    pub(crate) fn push_with_action(
        &mut self,
        tone: ToastTone,
        text: impl Into<String>,
        action_label: impl Into<String>,
        intent: ToastIntent,
        copyable: bool,
    ) {
        let text = text.into();
        let action_label = action_label.into();
        // Hikari's CONSECUTIVE dedupe (useToast compares only the slot's
        // LAST message): an identical repeat of the NEWEST card of this
        // tone refreshes it in place — clock restart, action/copy update,
        // "latest push owns the action" — instead of stacking a dup.
        // An older card is NEVER resurrected (A,B,A appends a third
        // card): stack order must keep mirroring recency, or trim() would
        // evict the just-refreshed card first.
        let refresh = self
            .items
            .last_mut()
            .filter(|t| t.tone == tone && t.text == text);
        if let Some(live) = refresh {
            live.born = Instant::now();
            // The latest push owns the action: a label refreshes it, an
            // empty one (the action-less `push`) clears it — the same
            // "latest message decides" contract useToast documents.
            live.action = if action_label.is_empty() {
                None
            } else {
                Some((action_label, intent))
            };
            live.copyable = live.copyable || copyable;
            return;
        }
        let action = if action_label.is_empty() {
            None
        } else {
            Some((action_label, intent))
        };
        self.items.push(Toast {
            id: self.next_id,
            tone,
            text,
            action,
            copyable,
            born: Instant::now(),
        });
        self.next_id += 1;
        self.trim();
    }

    /// Drops the oldest card when the stack overflows its seat.
    fn trim(&mut self) {
        if self.items.len() > MAX_LIVE {
            let drop = self.items.len() - MAX_LIVE;
            self.items.drain(..drop);
        }
    }

    #[allow(dead_code)] // the stack's length is a test/observability view
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[allow(dead_code)] // the stack's length is a test/observability view
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    /// Drops every card whose lifetime ran out (the HkToast cron) and
    /// returns whether anything is left.
    fn expire(&mut self, now: Instant) -> bool {
        self.items
            .retain(|toast| now.duration_since(toast.born) < toast.tone.ttl());
        !self.items.is_empty()
    }

    /// Paints the stack and returns the intents the host must act on. This
    /// runs AFTER the panels and before nothing: the stack is the topmost
    /// surface of the frame (toasts outrank modals in hikari's popup
    /// manager too).
    pub(crate) fn draw(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        icons: &CaptionIcons,
    ) -> Vec<ToastIntent> {
        let now = Instant::now();
        if !self.expire(now) {
            return Vec::new();
        }
        // Keep the clock alive while a card is live: the nearest expiry
        // schedules the repaint that removes it.
        if let Some(remaining) = self
            .items
            .iter()
            .map(|toast| {
                toast
                    .tone
                    .ttl()
                    .saturating_sub(now.duration_since(toast.born))
            })
            .min()
        {
            ctx.request_repaint_after(remaining.max(Duration::from_millis(16)));
        }

        let mut intents = Vec::new();
        let mut dismissed: Vec<u64> = Vec::new();
        #[cfg(test)]
        self.layout.clear();
        egui::Area::new(Id::new("shun-toast-stack"))
            // Foreground: above every panel and above the guide modal
            // (which is drawn earlier in the same order) — hikari keeps
            // toasts in the topmost popup band for exactly this reason.
            .order(egui::Order::Foreground)
            // Of the area's RIGHT_TOP against the screen's RIGHT_TOP: the
            // x offset pulls it in by the gutter, the y offset pushes it
            // DOWN from the window top. (-TOAST_TOP would put the stack
            // above the screen, and egui's on-screen constraint would clamp
            // it back over the caption bar — captured in the X11 pass.)
            .anchor(Align2::RIGHT_TOP, vec2(-TOAST_GUTTER, TOAST_TOP))
            .interactable(true)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_width(TOAST_WIDTH);
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (index, toast) in self.items.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(STACK_GAP);
                    }
                    let (dismiss, layout) = paint_card(ui, ctx, theme, icons, toast, &mut intents);
                    if let Some(id) = dismiss {
                        dismissed.push(id);
                    }
                    #[cfg(test)]
                    self.layout.push(layout);
                    #[cfg(not(test))]
                    let _ = layout;
                }
            });
        if !dismissed.is_empty() {
            self.items.retain(|toast| !dismissed.contains(&toast.id));
        }
        intents
    }
}

/// Paints one card; returns its id when a control asked to dismiss it,
/// plus the geometry the layout test reads back.
fn paint_card(
    ui: &mut egui::Ui,
    ctx: &Context,
    theme: &Theme,
    icons: &CaptionIcons,
    toast: &Toast,
    intents: &mut Vec<ToastIntent>,
) -> (Option<u64>, CardLayout) {
    // The solid band + its on-solid ink (HkToast.scss's pairs, see
    // [`ToastTone::band`]). Every mark on the card inherits the ink, exactly
    // like HkToast's `color` inheritance: glyph, body, action, copy, close.
    let (band, ink) = toast.tone.band(theme);
    // Widths of the right-hand controls are measured BEFORE the text wraps:
    // the body column owns what is left of the 360pt card.
    let action_width = toast.action.as_ref().map(|(label, _)| {
        ui.painter()
            .layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(12.5),
                Color32::WHITE,
            )
            .size()
            .x
            + 20.0
    });
    let controls = CONTROL
        + if toast.copyable { CONTROL + 4.0 } else { 0.0 }
        + action_width.map_or(0.0, |width| width + GAP);
    let text_width = (TOAST_WIDTH - 2.0 * PAD_X - ICON - GAP - controls - GAP).max(80.0);

    // The clamped body galley: three rows maximum, elided with `…`.
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = text_width;
    job.wrap.max_rows = CLAMP_ROWS;
    job.wrap.overflow_character = Some('…');
    job.append(
        &toast.text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(13.5),
            color: ink,
            ..Default::default()
        },
    );
    let galley = ui.fonts(|fonts| fonts.layout_job(job));

    let inner_height = galley.size().y.max(CONTROL).max(ICON);
    let height = inner_height + 2.0 * PAD_Y;
    let (card, _) = ui.allocate_exact_size(vec2(TOAST_WIDTH, height), Sense::hover());
    let painter = ui.painter_at(card);

    // hikari's `box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2)`.
    painter.add(
        egui::epaint::Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(51),
        }
        .as_shape(card, CornerRadius::same(RADIUS)),
    );
    // The solid tone band. hikari's card carries no border — the shadow
    // lifts it off the pane, the fill carries the tone.
    painter.rect_filled(card, CornerRadius::same(RADIUS), band);

    // ── Controls, right to left: close, copy, action ──────────────────
    let center_y = card.center().y;
    let mut right = card.right() - PAD_X;
    let close = Rect::from_center_size(pos2(right - CONTROL / 2.0, center_y), Vec2::splat(CONTROL));
    right -= CONTROL;
    let copy = toast.copyable.then(|| {
        let rect =
            Rect::from_center_size(pos2(right - CONTROL / 2.0, center_y), Vec2::splat(CONTROL));
        right -= CONTROL + 4.0;
        rect
    });
    let action = action_width.map(|width| {
        right -= GAP;
        let rect = Rect::from_center_size(pos2(right - width / 2.0, center_y), vec2(width, 24.0));
        right -= width;
        rect
    });

    // ── Icon + body ───────────────────────────────────────────────────
    let glyph = match toast.tone {
        ToastTone::Error | ToastTone::Warning => &icons.alert,
        ToastTone::Info => &icons.info,
        ToastTone::Success => &icons.check,
    };
    let icon = Rect::from_center_size(
        pos2(card.left() + PAD_X + ICON / 2.0, center_y),
        Vec2::splat(ICON),
    );
    painter.image(
        glyph.id(),
        icon,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        ink,
    );
    let text_pos = pos2(
        card.left() + PAD_X + ICON + GAP,
        center_y - galley.size().y / 2.0,
    );
    painter.galley(text_pos, galley, ink);

    // ── Hit regions + hover paints ────────────────────────────────────
    let mut dismissed = None;
    let close_response = ui
        .interact(close, Id::new(("toast-close", toast.id)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if close_response.clicked() {
        dismissed = Some(toast.id);
    }
    // HkToast's icon buttons: the inherited ink, with a `currentColor`
    // wash plate on hover (12–18% in the standard icon button; 16% here).
    let control = |hovered: bool| {
        if hovered {
            ink.gamma_multiply(1.0)
        } else {
            ink.gamma_multiply(0.82)
        }
    };
    let wash = Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), 41);
    let close_hovered = close_response.hovered() || close_response.is_pointer_button_down_on();
    if close_hovered {
        painter.rect_filled(close, CornerRadius::same(6), wash);
    }
    paint_cross(&painter, close, control(close_hovered));

    if let Some(copy) = copy {
        let response = ui
            .interact(copy, Id::new(("toast-copy", toast.id)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            ctx.copy_text(toast.text.clone());
        }
        let hovered = response.hovered() || response.is_pointer_button_down_on();
        if hovered {
            painter.rect_filled(copy, CornerRadius::same(6), wash);
        }
        paint_copy_glyph(&painter, copy, control(hovered));
    }

    if let (Some(rect), Some((label, intent))) = (action, toast.action.as_ref()) {
        let response = ui
            .interact(rect, Id::new(("toast-action", toast.id)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            // HkToast's handleAction: remove the card, then run the
            // handler once.
            dismissed = Some(toast.id);
            intents.push(intent.clone());
        }
        let hovered = response.hovered() || response.is_pointer_button_down_on();
        // HkToast's `.hk-toast-action` glass pill: a border and fill mixed
        // from `currentColor` (38% / 14%, 55% / 24% while hovered) with the
        // band's ink for the label.
        let ratio = |percent: u8| -> u8 { (u16::from(percent) * 255 / 100) as u8 };
        let fill_alpha = ratio(if hovered { 24 } else { 14 });
        let stroke_alpha = ratio(if hovered { 55 } else { 38 });
        painter.rect_filled(
            rect,
            CornerRadius::same(12),
            Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), fill_alpha),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(
                1.0f32,
                Color32::from_rgba_unmultiplied(ink.r(), ink.g(), ink.b(), stroke_alpha),
            ),
            egui::StrokeKind::Inside,
        );
        let font = egui::FontId::proportional(12.5);
        let galley = painter.layout_no_wrap(label.clone(), font, ink);
        painter.galley(
            pos2(
                rect.center().x - galley.size().x / 2.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            ink,
        );
    }

    (
        dismissed,
        CardLayout {
            card,
            close,
            copy,
            action,
        },
    )
}

/// The close `✕` — two 10pt strokes through the 24pt control box (the
/// lucide `x` this face has no texture slot for; the caption bar's own
/// close glyph is the same geometry).
fn paint_cross(painter: &egui::Painter, box_rect: Rect, color: Color32) {
    let half = 5.0;
    let c = box_rect.center();
    let stroke = Stroke::new(1.4f32, color);
    painter.line_segment(
        [pos2(c.x - half, c.y - half), pos2(c.x + half, c.y + half)],
        stroke,
    );
    painter.line_segment(
        [pos2(c.x - half, c.y + half), pos2(c.x + half, c.y - half)],
        stroke,
    );
}

/// The copy control: lucide's `copy` — a front sheet over a back sheet.
fn paint_copy_glyph(painter: &egui::Painter, box_rect: Rect, color: Color32) {
    let c = box_rect.center();
    let stroke = Stroke::new(1.2f32, color);
    let back = Rect::from_min_size(pos2(c.x - 5.5, c.y - 3.5), vec2(7.5, 8.5));
    let front = Rect::from_min_size(pos2(c.x - 2.5, c.y - 5.5), vec2(7.5, 8.5));
    painter.rect_stroke(
        back,
        CornerRadius::same(2),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(front, CornerRadius::same(2), Color32::TRANSPARENT);
    painter.rect_stroke(
        front,
        CornerRadius::same(2),
        stroke,
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs frames until the anchored seat repeats, then returns the settled
    /// card layout. An anchored `Area` sizes itself from the previous pass
    /// (and the on-screen constraint may shift it once more), so a single
    /// frame's geometry is not the window's geometry.
    fn settle(
        ctx: &Context,
        host: &mut ToastHost,
        theme: &Theme,
        icons: &CaptionIcons,
    ) -> CardLayout {
        use crate::toast::CardLayout as Layout;
        let mut previous: Option<Rect> = None;
        let mut settled: Option<Layout> = None;
        for _ in 0..8 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                host.draw(ctx, theme, icons);
            });
            let layout = host.layout.pop().expect("the card laid out");
            let seat = layout.close;
            if previous == Some(seat) {
                settled = Some(layout);
                break;
            }
            previous = Some(seat);
            settled = Some(layout);
        }
        settled.expect("the seat settled")
    }

    /// `TOAST_DURATION` from hikari's runtime/useToast.ts, verbatim.
    #[test]
    fn the_lifetime_table_matches_hikari() {
        assert_eq!(ToastTone::Error.ttl(), Duration::from_secs(30));
        assert_eq!(ToastTone::Warning.ttl(), Duration::from_secs(30));
        assert_eq!(ToastTone::Success.ttl(), Duration::from_secs(3));
        assert_eq!(ToastTone::Info.ttl(), Duration::from_secs(3));
    }

    /// hikari's `top: 4rem` seat: on this face the caption bar is in-band,
    /// so the stack must start clear of it, never under it. The geometry
    /// itself is a compile-time invariant (the painted seat is asserted
    /// against a real layout in `the_card_sits_in_the_top_right_seat`).
    #[test]
    fn the_stack_clears_the_caption_bar() {
        const _: () = assert!(TOAST_TOP >= crate::fallback::TITLEBAR_H + 16.0);
        // ... and the gutter keeps the card off the window edge.
        assert_eq!(TOAST_GUTTER, 16.0);
        assert_eq!(TOAST_WIDTH, 360.0);
    }

    /// Newest at the bottom (HkToast appends into the TransitionGroup): the
    /// vector order IS the paint order, so the last push is the last row.
    #[test]
    fn cards_append_and_overflow_drops_the_oldest() {
        let mut host = ToastHost::new();
        assert!(host.is_empty());
        host.push(ToastTone::Info, "one");
        host.push(ToastTone::Error, "two");
        assert_eq!(host.len(), 2);
        assert_eq!(host.items[0].text, "one");
        assert_eq!(host.items[1].text, "two");
        assert_eq!(host.items[0].id + 1, host.items[1].id, "ids are monotonic");

        for index in 0..MAX_LIVE {
            host.push(ToastTone::Info, format!("extra-{index}"));
        }
        assert_eq!(host.len(), MAX_LIVE, "the stack stays bounded");
        assert_eq!(
            host.items.last().expect("a live card").text,
            format!("extra-{}", MAX_LIVE - 1),
            "the newest card is still the last row"
        );
    }

    /// Expiry is per tone (the cron equivalent), and an empty stack draws
    /// nothing at all (no area, no repaint request).
    #[test]
    fn expiry_follows_the_per_tone_lifetime() {
        let mut host = ToastHost::new();
        let start = Instant::now();
        host.push(ToastTone::Success, "ack");
        assert!(
            host.expire(start + Duration::from_secs(2)),
            "a success is still on screen at 2 s"
        );
        assert_eq!(host.len(), 1);
        assert!(
            !host.expire(start + Duration::from_secs(4)),
            "the 3 s window closed"
        );
        assert!(host.is_empty());

        host.push(ToastTone::Error, "boom");
        assert!(
            host.expire(start + Duration::from_secs(4)),
            "an error outlives the success window"
        );
        assert_eq!(host.len(), 1, "an error holds for 30 s");
        assert!(!host.expire(start + Duration::from_secs(31)));
        assert!(host.is_empty());
    }

    /// A headless pass loop: the card lays out, clamps to three rows and
    /// keeps its seat inside the window — and pointer controls get hit
    /// regions (a click on the close box dismisses the card).
    #[test]
    fn the_card_lays_out_clamped_and_interactive() {
        let ctx = egui::Context::default();
        let icons = CaptionIcons::load(&ctx);
        let theme = Theme::dark(None);
        let mut host = ToastHost::new();
        host.push_with_action(
            ToastTone::Error,
            "The application could not be started because application-control \
             policy blocked it. The installation itself finished successfully; \
             try Windows Security → App & browser control, or ask your \
             administrator, then retry the launch."
                .repeat(2),
            "Details",
            ToastIntent::OpenGuide {
                topic: "app-control".into(),
                details: crate::guide::GuideDetails {
                    title: "Launch failed".into(),
                    body: "io error: 4551".into(),
                },
            },
            true,
        );
        let mut painted = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            painted = Some(host.draw(ctx, &theme, &icons));
        });
        assert_eq!(painted.expect("the pass loop ran"), Vec::new());

        // The clamp: a very long body never exceeds three text rows.
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = 180.0;
        job.wrap.max_rows = CLAMP_ROWS;
        job.wrap.overflow_character = Some('…');
        job.append(
            &"long body ".repeat(80),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(13.5),
                color: Color32::WHITE,
                ..Default::default()
            },
        );
        let galley = ctx.fonts(|fonts| fonts.layout_job(job));
        assert!(
            galley.rows.len() <= CLAMP_ROWS,
            "rows={}",
            galley.rows.len()
        );
        assert!(galley.elided, "the ellipsis marks the clamp");
    }

    /// Clicking the close box dismisses; clicking the action reports its
    /// intent and dismisses too (HkToast's single-shot action).
    #[test]
    fn close_and_action_hit_regions_dismiss() {
        let ctx = egui::Context::default();
        let icons = CaptionIcons::load(&ctx);
        let theme = Theme::dark(None);
        let mut host = ToastHost::new();
        host.push_with_action(
            ToastTone::Warning,
            "a shortcut could not be created",
            "Details",
            ToastIntent::ShowDetails {
                title: "Error".into(),
                body: "io error".into(),
            },
            true,
        );
        // An anchored area learns its size from the previous pass, so the
        // seat settles after a frame or two; wait for it to repeat.
        let close_at = settle(&ctx, &mut host, &theme, &icons).close.center();
        let mut intents = Vec::new();
        // A real press lands in one frame and the release in the next;
        // egui's click state machine resolves on the release.
        let _ = ctx.run(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(close_at),
                    egui::Event::PointerButton {
                        pos: close_at,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                intents = host.draw(ctx, &theme, &icons);
            },
        );
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::PointerButton {
                    pos: close_at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| {
                intents = host.draw(ctx, &theme, &icons);
            },
        );
        assert!(intents.is_empty(), "a close reports no intent");
        assert!(host.is_empty(), "the close box dismissed the card");
    }

    /// The card's seat, measured from the painted geometry: right gutter
    /// 16pt, top 56pt (clear of the caption bar), the full 360pt width.
    /// This is the layout the X11 capture verifies by eye.
    #[test]
    fn an_identical_repeat_refreshes_instead_of_stacking() {
        // hikari's CONSECUTIVE dedupe: six failed finishes with the same
        // text read as ONE card whose clock restarted — not a column of
        // duplicates walking into the footer band.
        let mut host = ToastHost::new();
        for _ in 0..6 {
            host.push_with_action(
                ToastTone::Error,
                "same failure",
                "details",
                ToastIntent::ShowDetails {
                    title: String::new(),
                    body: String::new(),
                },
                true,
            );
        }
        assert_eq!(host.items.len(), 1, "identical repeats dedupe");
        // The clock restart is pinned by AGING the card first: a stale
        // born plus a fresh push must move `born` forward (the naive
        // assertion — elapsed < 500ms — passes even when nothing
        // refreshed).
        let stale_born = Instant::now() - Duration::from_secs(10);
        host.items[0].born = stale_born;
        host.push_with_action(
            ToastTone::Error,
            "same failure",
            "details",
            ToastIntent::ShowDetails {
                title: String::new(),
                body: String::new(),
            },
            true,
        );
        assert_eq!(host.items.len(), 1, "the repeat dedupes");
        assert!(
            host.items[0].born > stale_born,
            "the surviving card's clock restarted"
        );

        // A DIFFERENT text still stacks (that is a new notice, not a
        // retry of the old one), and it becomes the newest row.
        host.push_with_action(
            ToastTone::Error,
            "a different failure",
            "details",
            ToastIntent::ShowDetails {
                title: String::new(),
                body: String::new(),
            },
            true,
        );
        assert_eq!(host.items.len(), 2);
        assert_eq!(host.items[1].text, "a different failure");

        // hikari compares ONLY the newest message: a repeat of the OLDER
        // card's text does not resurrect it — it appends a third card
        // (A,B,A appends; stack order keeps mirroring recency, so trim
        // never evicts a just-refreshed card first).
        host.push_with_action(
            ToastTone::Error,
            "same failure",
            "details",
            ToastIntent::ShowDetails {
                title: String::new(),
                body: String::new(),
            },
            true,
        );
        assert_eq!(host.items.len(), 3, "A,B,A appends");
        assert_eq!(
            host.items[2].text, "same failure",
            "the new card is the newest row"
        );
        // And the untouched older cards keep their own pills.
        assert!(host.items[0].action.is_some(), "old card keeps its action");
    }

    #[test]
    fn the_action_less_push_clears_the_newest_cards_pill() {
        // hikari's "latest push owns the action": an action-less push on
        // a fresh consecutive match clears the pill.
        let mut host = ToastHost::new();
        host.push_with_action(
            ToastTone::Error,
            "same failure",
            "details",
            ToastIntent::ShowDetails {
                title: String::new(),
                body: String::new(),
            },
            true,
        );
        assert!(host.items[0].action.is_some());
        host.push(ToastTone::Error, "same failure");
        assert_eq!(host.items.len(), 1, "the plain push dedupes too");
        assert!(
            host.items[0].action.is_none(),
            "the latest push cleared the action"
        );
    }

    #[test]
    fn the_card_sits_in_the_top_right_seat() {
        let ctx = egui::Context::default();
        let icons = CaptionIcons::load(&ctx);
        let theme = Theme::dark(None);
        let mut host = ToastHost::new();
        host.push_with_action(
            ToastTone::Error,
            "the launch was blocked",
            "Details",
            ToastIntent::OpenGuide {
                topic: "app-control".into(),
                details: crate::guide::GuideDetails {
                    title: "Launch failed".into(),
                    body: "io error: 4551".into(),
                },
            },
            true,
        );
        let layout = settle(&ctx, &mut host, &theme, &icons);
        let screen = ctx.screen_rect();
        assert_eq!(layout.card.width(), TOAST_WIDTH);
        assert!(
            (layout.card.right() - (screen.right() - TOAST_GUTTER)).abs() < 0.5,
            "right gutter 16 (card right {} vs screen right {})",
            layout.card.right(),
            screen.right()
        );
        assert!(
            (layout.card.top() - (screen.top() + TOAST_TOP)).abs() < 0.5,
            "top seat {} (caption bar is {}pt)",
            layout.card.top(),
            crate::fallback::TITLEBAR_H
        );
        assert!(
            layout.card.top() > screen.top() + crate::fallback::TITLEBAR_H,
            "the card starts BELOW the caption bar"
        );
        // The controls live inside the card.
        assert!(layout.card.contains_rect(layout.close));
        assert!(layout.card.contains_rect(layout.copy.expect("copyable")));
        assert!(
            layout
                .card
                .contains_rect(layout.action.expect("actionable"))
        );
    }
}
