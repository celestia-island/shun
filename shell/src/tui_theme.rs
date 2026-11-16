//! The terminal face's palette, glyph set and text helpers — the aoba-
//! style layer that keeps colors and spacing in one place.
//!
//! Everything derives from the probed [TerminalCaps]: a mono console gets
//! weight instead of color, a modern terminal gets the semantic palette,
//! and the glyph set flips to ASCII when the terminal cannot draw box
//! characters. Labels follow the reference TUI's rule: an indicator, the
//! bold label padded to a fixed column, and two spaces before the value
//! (aoba's LABEL_PADDING_EXTRA).

use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};
use unicode_width::UnicodeWidthStr;

use crate::tui_caps::{ColorDepth, TerminalCaps};

/// Label column width (value starts after `LABEL_WIDTH + PAD`).
pub const LABEL_WIDTH: usize = 18;
/// Extra breathing room between the label and the value (aoba's
/// `LABEL_PADDING_EXTRA`).
pub const LABEL_PAD: usize = 2;

/// The glyph set: unicode for modern terminals, ASCII for legacy ones.
#[derive(Clone, Copy)]
pub struct Glyphs {
    pub done: &'static str,
    pub active: &'static str,
    pub todo: &'static str,
    pub ok: &'static str,
    pub bad: &'static str,
    pub warn: &'static str,
    pub check_on: &'static str,
    pub check_off: &'static str,
    pub sep: &'static str,
    pub arrow: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    done: "✓",
    active: "●",
    todo: "○",
    ok: "✓",
    bad: "✗",
    warn: "⚠",
    check_on: "[x]",
    check_off: "[ ]",
    sep: " · ",
    arrow: "→",
};

const ASCII: Glyphs = Glyphs {
    done: "v",
    active: ">",
    todo: " ",
    ok: "y",
    bad: "n",
    warn: "!",
    check_on: "[x]",
    check_off: "[ ]",
    sep: " | ",
    arrow: "->",
};

/// The resolved style set (colors already degraded for the host).
#[derive(Clone)]
pub struct Theme {
    pub caps: TerminalCaps,
    pub glyphs: Glyphs,
    pub accent: Style,
    pub ok: Style,
    pub warn: Style,
    pub error: Style,
    pub muted: Style,
    pub title: Style,
    pub key: Style,
    pub plain: Style,
    pub selected: Style,
    pub border_set: border::Set,
}

impl Theme {
    pub fn new(caps: &TerminalCaps) -> Self {
        // Semantic colors per depth: RGB on truecolor, the ANSI-256 cube on
        // 256-color, the classic 16 on basic, weight only on mono.
        let pick = |rgb: (u8, u8, u8), ansi256: u8, basic: Color| match caps.color {
            ColorDepth::Mono => Style::default(),
            ColorDepth::Basic16 => Style::default().fg(basic),
            ColorDepth::Ansi256 => Style::default().fg(Color::Indexed(ansi256)),
            ColorDepth::TrueColor => Style::default().fg(Color::Rgb(rgb.0, rgb.1, rgb.2)),
        };
        let weight = |style: Style, modifier: Modifier| {
            if caps.color == ColorDepth::Mono {
                style.add_modifier(modifier)
            } else {
                style
            }
        };

        let accent = weight(pick((86, 182, 255), 75, Color::Cyan), Modifier::BOLD);
        let ok = pick((116, 196, 118), 78, Color::Green);
        let warn = weight(pick((238, 190, 96), 221, Color::Yellow), Modifier::ITALIC);
        let error = weight(pick((238, 110, 110), 203, Color::Red), Modifier::BOLD);
        let muted = pick((128, 138, 150), 245, Color::DarkGray);
        let title = accent.add_modifier(Modifier::BOLD);
        let key = accent.add_modifier(Modifier::BOLD);
        let plain = Style::default();
        let selected = if caps.color == ColorDepth::Mono {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            accent.add_modifier(Modifier::REVERSED)
        };

        Self {
            caps: caps.clone(),
            glyphs: if caps.unicode { UNICODE } else { ASCII },
            accent,
            ok,
            warn,
            error,
            muted,
            title,
            key,
            plain,
            selected,
            border_set: if caps.unicode {
                border::ROUNDED
            } else {
                ascii_border()
            },
        }
    }

    /// A bordered block with the theme's border set and a padded title.
    pub fn block(&self, title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_set(self.border_set)
            .border_style(self.border_style())
            .title(Span::styled(self.pad_title(title), self.title))
    }

    /// The rail's right-only border variant.
    pub fn rail_block(&self, title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::RIGHT)
            .border_set(self.border_set)
            .border_style(self.border_style())
            .title(Span::styled(self.pad_title(title), self.title))
    }

    /// Titles get one space of breathing room on each side (the aoba rule).
    pub fn pad_title(&self, title: &str) -> String {
        format!(" {title} ")
    }

    /// A label/value line: indicator column, bold label padded to the label
    /// column plus the extra pad, then the value.
    pub fn kv(&self, label: &str, value: Vec<Span<'static>>) -> Line<'static> {
        let mut spans = vec![Span::raw("  "), Span::styled(label.to_string(), self.key)];
        // Display width, not char count — CJK labels would otherwise
        // misalign every value column after them.
        let width = label.width();
        let pad = if width < LABEL_WIDTH {
            LABEL_WIDTH - width + LABEL_PAD
        } else {
            LABEL_PAD
        };
        spans.push(Span::raw(" ".repeat(pad)));
        spans.extend(value);
        Line::from(spans)
    }

    /// A checkbox line (checked → ok accent, unchecked → muted).
    pub fn checkbox(&self, checked: bool, label: &str) -> Line<'static> {
        let mark = if checked {
            self.glyphs.check_on
        } else {
            self.glyphs.check_off
        };
        let style = if checked { self.ok } else { self.muted };
        Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{mark} "), style),
            Span::styled(label.to_string(), self.plain),
        ])
    }

    /// The footer's key hints. A legacy ASCII console cannot draw the
    /// arrow glyphs the localized hints lead with, so it falls back to
    /// the English hints (the precedent for live pane hints) with the
    /// separators and arrows transliterated.
    pub fn hint(&self, footer: &'static str) -> String {
        if self.caps.unicode {
            footer.to_string()
        } else {
            Self::asciify(crate::tui_i18n::EN.footer)
        }
    }

    /// Transliterates the known unicode hint characters; anything else
    /// non-ASCII is dropped rather than mojibake'd.
    pub fn asciify(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '↑' => out.push_str("Up"),
                '↓' => out.push_str("Dn"),
                '→' => out.push_str("->"),
                '·' => out.push('|'),
                '—' | '–' => out.push('-'),
                c if c.is_ascii() => out.push(c),
                _ => {}
            }
        }
        out
    }

    fn border_style(&self) -> Style {
        if self.caps.color == ColorDepth::Mono {
            Style::default()
        } else {
            self.muted
        }
    }
}

/// The ASCII border set (legacy consoles cannot draw box characters).
fn ascii_border() -> border::Set {
    border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        vertical_left: "|",
        vertical_right: "|",
        horizontal_top: "-",
        horizontal_bottom: "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui_caps::{ImageSupport, TermKind};

    fn caps(color: ColorDepth, unicode: bool) -> TerminalCaps {
        TerminalCaps {
            kind: TermKind::WindowsTerminal,
            color,
            unicode,
            mouse: true,
            images: ImageSupport::None,
        }
    }

    #[test]
    fn mono_drops_colors_keeps_weight_and_switches_to_ascii() {
        let theme = Theme::new(&caps(ColorDepth::Mono, false));
        assert_eq!(theme.accent.fg, None);
        assert_eq!(theme.glyphs.active, ">");
        assert_eq!(theme.border_set.top_left, "+");
    }

    #[test]
    fn truecolor_uses_the_rgb_palette_and_unicode_glyphs() {
        let theme = Theme::new(&caps(ColorDepth::TrueColor, true));
        assert_eq!(theme.accent.fg, Some(Color::Rgb(86, 182, 255)));
        assert_eq!(theme.glyphs.active, "●");
        assert_eq!(theme.border_set.top_left, "╭");
    }

    #[test]
    fn kv_lines_pad_the_label_column() {
        let theme = Theme::new(&caps(ColorDepth::TrueColor, true));
        let line = theme.kv("Install to", vec![Span::raw("C:/Apps")]);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with("  Install to"));
        let value_at = text.find("C:/Apps").expect("value present");
        assert_eq!(value_at, 2 + LABEL_WIDTH + LABEL_PAD);
    }
}
