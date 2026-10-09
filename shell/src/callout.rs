//! The generic quote/callout block — the egui mirror of hikari's alert
//! family (`HkAlert` for info / warning / error / success) plus the
//! markdown blockquote's plain level: five levels in one component, so
//! every notice the fallback face shows rides the same chrome.
//!
//! Visual contract (HkAlert.scss, md size):
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │▐▌ ⚠  Title (text, 600)                    │  4pt start edge, full
//! │▐▌     Body copy (text-secondary, 1.5).    │  accent ink
//! └────────────────────────────────────────────┘
//! ```
//!
//! - fill: the accent mixed into the pane background at 8%;
//! - hairline: the accent at 15%, 1pt, every side;
//! - start edge: 4pt of the full accent (3pt for the plain level);
//! - radius 8pt; padding 12×16 (plain: 7×14, no fill, no hairline —
//!   `.hk-markdown blockquote` at its 14px base);
//! - an 18pt lucide glyph tinted with the accent leads the text column
//!   (alert-triangle for warning/error, info for info, check-circle for
//!   success; the plain level carries no glyph);
//! - the body column is caller-owned (`add_body`): notices embed their
//!   own link rows the same way HkAlert takes a default slot.

use eframe::egui;
use eframe::egui::{
    Color32, CornerRadius, Frame, Margin, Rect, RichText, Stroke, TextureHandle, Vec2,
};

use crate::fallback::{Theme, mix};

/// The five notice levels. The four colored ones map one-to-one onto
/// hikari's `HkAlert` variants; `Normal` is the markdown blockquote —
/// a plain quoted paragraph with a neutral edge and no ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // the five-level contract; notices use Warning today
pub(crate) enum CalloutLevel {
    /// The blockquote: neutral 3pt edge, no fill, no glyph.
    Normal,
    /// Primary ink — hikari's `info`.
    Info,
    /// Warning ink — hikari's `warning`.
    Warning,
    /// Error ink — hikari's `error`.
    Error,
    /// Success ink — hikari's `success`.
    Success,
}

impl CalloutLevel {
    /// Every level, in declaration order — the rendering tests sweep this.
    #[allow(dead_code)]
    pub(crate) const ALL: [CalloutLevel; 5] = [
        CalloutLevel::Normal,
        CalloutLevel::Info,
        CalloutLevel::Warning,
        CalloutLevel::Error,
        CalloutLevel::Success,
    ];

    /// The level's ink: the start edge, the glyph and the hairline all
    /// derive from this one color (HkAlert tints `hk-alert-icon` with it).
    fn accent(self, theme: &Theme) -> Color32 {
        match self {
            CalloutLevel::Normal => theme.text_tertiary,
            CalloutLevel::Info => theme.primary,
            CalloutLevel::Warning => theme.warning,
            CalloutLevel::Error => theme.error,
            CalloutLevel::Success => theme.success,
        }
    }
}

/// The glyph textures the colored levels lead their text column with —
/// borrowed from the face's shared caption-icon set (one rasterizer, one
/// source of lucide truth; the tests load them headless through the same
/// path).
pub(crate) struct CalloutIcons<'a> {
    pub(crate) alert: &'a TextureHandle,
    pub(crate) info: &'a TextureHandle,
    pub(crate) check: &'a TextureHandle,
}

impl CalloutLevel {
    /// The glyph for this level, if it carries one (`None` for Normal).
    fn glyph<'a>(self, icons: &'a CalloutIcons<'a>) -> Option<&'a TextureHandle> {
        match self {
            CalloutLevel::Normal => None,
            CalloutLevel::Info => Some(icons.info),
            // HkAlert pins AlertTriangle on BOTH warning and error.
            CalloutLevel::Warning | CalloutLevel::Error => Some(icons.alert),
            CalloutLevel::Success => Some(icons.check),
        }
    }
}

/// Renders one quote block as the next item of `ui`'s document flow:
/// full available width, its own height, following whatever the caller
/// laid out before it — never a pinned position.
///
/// `title` is optional (HkAlert's `hk-alert-no-title` centers the row);
/// `add_body` appends caller content under the text copy (the degrade
/// notices embed their download-link row there).
///
/// Returns the painted rect — the rendering tests assert the flow
/// geometry on it.
pub(crate) fn callout(
    ui: &mut egui::Ui,
    theme: &Theme,
    icons: &CalloutIcons<'_>,
    level: CalloutLevel,
    title: Option<&str>,
    body: &str,
    add_body: &mut dyn FnMut(&mut egui::Ui),
) -> Rect {
    let accent = level.accent(theme);
    let plain = level == CalloutLevel::Normal;
    let pane = theme.pane_bg_override.unwrap_or(theme.background);
    let mut frame = Frame::default()
        .inner_margin(if plain {
            // The blockquote: 0.5em/1em at hikari's 14px base.
            Margin::symmetric(14, 7)
        } else {
            Margin::symmetric(16, 12)
        })
        .corner_radius(CornerRadius::same(8));
    if !plain {
        frame = frame
            .fill(mix(pane, accent, 0.08))
            .stroke(Stroke::new(1.0f32, mix(pane, accent, 0.15)));
    }
    let painted = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if let Some(glyph) = level.glyph(icons) {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                ui.painter_at(rect).image(
                    glyph.id(),
                    rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    accent,
                );
                ui.add_space(12.0);
            }
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                if let Some(title) = title {
                    ui.label(RichText::new(title).strong().size(14.0).color(theme.text));
                    ui.add_space(2.0);
                }
                // hikari's .hk-alert-text: text-secondary composited over
                // the tinted fill at 85%.
                let fill = mix(pane, accent, 0.08);
                ui.label(RichText::new(body).size(13.5).color(if plain {
                    theme.text_secondary
                } else {
                    mix(fill, theme.text_secondary, 0.85)
                }));
                ui.add_space(4.0);
                add_body(ui);
            });
        });
    });
    // The accent edge paints OVER the frame's left hairline, following
    // the frame's corner arcs exactly — a plain rounded strip would get
    // its radius clamped down to half the strip width and poke outside
    // the frame's silhouette at the corners. HkAlert's 4pt
    // inline-start border, 3pt for the plain blockquote.
    let edge = if plain { 3.0 } else { 4.0 };
    let rect = painted.response.rect;
    left_edge(ui.painter(), rect, edge, accent);
    rect
}

/// The inline-start accent bar: the strip between the frame's outer left
/// arc (radius 8) and the inner arc (radius 8 − width), traced as one
/// convex polygon — CSS border geometry, without epaint's radius-clamped
/// rounded rectangles.
fn left_edge(painter: &egui::Painter, rect: Rect, width: f32, color: Color32) {
    const R: f32 = 8.0;
    let l = rect.left();
    let t = rect.top();
    let b = rect.bottom();
    let ri = (R - width).max(0.0);
    // Quarter-arc polyline (3 points, 22.5° steps).
    let arc = |cx: f32, cy: f32, r: f32, from_deg: f32, to_deg: f32| {
        (0..=2)
            .map(|step| {
                let deg = from_deg + (to_deg - from_deg) * step as f32 / 2.0;
                let rad = deg.to_radians();
                egui::pos2(cx + r * rad.cos(), cy + r * rad.sin())
            })
            .collect::<Vec<_>>()
    };
    let mut pts = Vec::new();
    // Outer boundary, top → bottom: the top-left arc (pointing up at
    // −90°, sweeping to 180°/left), the straight run, the bottom-left
    // arc; then the inner boundary back up (radii reduced by the width).
    pts.extend(arc(l + R, t + R, R, -90.0, -180.0));
    pts.extend(arc(l + R, b - R, R, 180.0, 90.0));
    pts.extend(arc(l + width + ri, b - R, ri, 90.0, 180.0));
    pts.extend(arc(l + width + ri, t + R, ri, 180.0, 270.0));
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fallback::CaptionIcons;

    /// A headless egui context — enough of a pass loop to lay real
    /// widgets out (no window, no GPU; the glyph rasterizer is pure CPU).
    fn with_panel(f: impl FnOnce(&mut egui::Ui, &Theme, &CalloutIcons<'_>)) {
        let ctx = egui::Context::default();
        let icons = CaptionIcons::load(&ctx);
        let callout_icons = CalloutIcons {
            alert: &icons.alert,
            info: &icons.info,
            check: &icons.check,
        };
        let theme = Theme::dark(None);
        let mut f = Some(f);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(560.0);
                if let Some(f) = f.take() {
                    f(ui, &theme, &callout_icons);
                }
            });
        });
    }

    /// Every level renders in the flow: full width, positive height, no
    /// panic — and the block FOLLOWS the item laid out before it (the
    /// position contract: never pinned).
    #[test]
    fn every_level_renders_in_the_flow_after_the_previous_item() {
        with_panel(|ui, theme, icons| {
            for (index, level) in CalloutLevel::ALL.into_iter().enumerate() {
                ui.label("previous item");
                let previous_bottom = ui.next_widget_position().y;
                ui.add_space(8.0);
                let rect = callout(ui, theme, icons, level, Some("Title"), "Body", &mut |_| {});
                assert!(
                    rect.width() > 400.0,
                    "{level:?} spans the available width ({})",
                    rect.width()
                );
                assert!(rect.height() > 16.0, "{level:?} has height");
                assert!(
                    rect.top() >= previous_bottom,
                    "{level:?} follows item {index} in the document flow ({} >= {})",
                    rect.top(),
                    previous_bottom,
                );
            }
        });
    }

    /// A title row makes the block taller than the title-less spelling,
    /// and the caller's body content (a link button) renders inside the
    /// painted rect.
    #[test]
    fn title_and_link_rows_change_the_block() {
        with_panel(|ui, theme, icons| {
            let level = CalloutLevel::Warning;
            let plain = callout(ui, theme, icons, level, None, "Body", &mut |_| {});
            let titled = callout(ui, theme, icons, level, Some("Title"), "Body", &mut |_| {});
            assert!(
                titled.height() > plain.height() + 14.0,
                "a title row adds height ({} > {})",
                titled.height(),
                plain.height()
            );

            let mut link_rect = None;
            let with_link = callout(ui, theme, icons, level, None, "Body", &mut |ui| {
                let response = ui.button("open the download page");
                link_rect = Some(response.rect);
            });
            let link_rect = link_rect.expect("the link rendered");
            assert!(link_rect.height() > 8.0 && link_rect.width() > 24.0);
            assert!(
                with_link.contains(link_rect.min),
                "the caller content renders inside the block"
            );
        });
    }
    /// Long CJK copy wraps instead of overflowing the block, and the
    /// level→ink mapping matches the hikari contract exactly.
    #[test]
    fn long_copy_wraps_and_the_level_inks_match_hikari() {
        with_panel(|ui, theme, icons| {
            let long = "未检测到 WebView2 运行时".repeat(24);
            let top = ui.cursor().top();
            callout(
                ui,
                theme,
                icons,
                CalloutLevel::Error,
                Some("长文"),
                &long,
                &mut |_| {},
            );
            let height = ui.cursor().bottom() - top;
            assert!(height > 60.0, "a long body wraps into lines (h={height})");

            // The four inked levels carry their hikari accents; Normal
            // carries the blockquote's tertiary edge.
            assert_eq!(CalloutLevel::Normal.accent(theme), theme.text_tertiary);
            assert_eq!(CalloutLevel::Info.accent(theme), theme.primary);
            assert_eq!(CalloutLevel::Warning.accent(theme), theme.warning);
            assert_eq!(CalloutLevel::Error.accent(theme), theme.error);
            assert_eq!(CalloutLevel::Success.accent(theme), theme.success);
        });
    }
}
