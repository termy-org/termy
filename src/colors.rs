use crate::config::{CustomColors, SHELL_DECIDE_THEME_ID};
use crate::theme_store;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Rgb as AnsiRgb};
use crate::gpui::Rgba;
use termy_themes as themes;
use termy_themes::Rgb8;

#[derive(Clone)]
pub struct TerminalColors {
    pub ansi: [Rgba; 16],
    pub foreground: Rgba,
    pub background: Rgba,
    pub cursor: Rgba,
}

impl Default for TerminalColors {
    fn default() -> Self {
        Self {
            ansi: [
                rgba(0x00, 0x00, 0x00),
                rgba(0xCD, 0x00, 0x00),
                rgba(0x00, 0xCD, 0x00),
                rgba(0xCD, 0xCD, 0x00),
                rgba(0x00, 0x00, 0xEE),
                rgba(0xCD, 0x00, 0xCD),
                rgba(0x00, 0xCD, 0xCD),
                rgba(0xE5, 0xE5, 0xE5),
                rgba(0x7F, 0x7F, 0x7F),
                rgba(0xFF, 0x00, 0x00),
                rgba(0x00, 0xFF, 0x00),
                rgba(0xFF, 0xFF, 0x00),
                rgba(0x5C, 0x5C, 0xFF),
                rgba(0xFF, 0x00, 0xFF),
                rgba(0x00, 0xFF, 0xFF),
                rgba(0xFF, 0xFF, 0xFF),
            ],
            foreground: rgba(0xE5, 0xE5, 0xE5),
            background: rgba(0x1E, 0x1E, 0x1E),
            cursor: rgba(0xFF, 0xFF, 0xFF),
        }
    }
}

impl TerminalColors {
    pub fn from_theme(theme: &str, custom: &CustomColors) -> Self {
        let mut colors = if theme.eq_ignore_ascii_case(SHELL_DECIDE_THEME_ID) {
            Self::default()
        } else {
            match theme_store::load_installed_theme_colors(theme)
                .or_else(|| themes::resolve_theme(theme))
            {
                Some(theme_colors) => Self::from_theme_colors(theme_colors),
                None => Self::default(),
            }
        };
        colors.apply_custom(custom);
        colors
    }

    fn from_theme_colors(theme: themes::ThemeColors) -> Self {
        Self {
            ansi: theme.ansi.map(rgba_from_theme_rgb),
            foreground: rgba_from_theme_rgb(theme.foreground),
            background: rgba_from_theme_rgb(theme.background),
            cursor: rgba_from_theme_rgb(theme.cursor),
        }
    }

    fn apply_custom(&mut self, custom: &CustomColors) {
        if let Some(fg) = custom.foreground {
            self.foreground = rgba(fg.r, fg.g, fg.b);
        }
        if let Some(bg) = custom.background {
            self.background = rgba(bg.r, bg.g, bg.b);
        }
        if let Some(cursor) = custom.cursor {
            self.cursor = rgba(cursor.r, cursor.g, cursor.b);
        }
        for (i, color) in custom.ansi.iter().enumerate() {
            if let Some(c) = color {
                self.ansi[i] = rgba(c.r, c.g, c.b);
            }
        }
    }

    /// Convert an alacritty ANSI color to a GPUI Rgba
    pub fn convert(&self, color: AnsiColor) -> Rgba {
        match color {
            AnsiColor::Named(named) => self.named_color(named),
            AnsiColor::Spec(AnsiRgb { r, g, b }) => rgba(r, g, b),
            AnsiColor::Indexed(idx) => self.indexed_color(idx),
        }
    }

    fn named_color(&self, color: NamedColor) -> Rgba {
        match color {
            NamedColor::Black => self.ansi[0],
            NamedColor::Red => self.ansi[1],
            NamedColor::Green => self.ansi[2],
            NamedColor::Yellow => self.ansi[3],
            NamedColor::Blue => self.ansi[4],
            NamedColor::Magenta => self.ansi[5],
            NamedColor::Cyan => self.ansi[6],
            NamedColor::White => self.ansi[7],
            NamedColor::BrightBlack => self.ansi[8],
            NamedColor::BrightRed => self.ansi[9],
            NamedColor::BrightGreen => self.ansi[10],
            NamedColor::BrightYellow => self.ansi[11],
            NamedColor::BrightBlue => self.ansi[12],
            NamedColor::BrightMagenta => self.ansi[13],
            NamedColor::BrightCyan => self.ansi[14],
            NamedColor::BrightWhite => self.ansi[15],
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            _ => self.foreground,
        }
    }

    fn indexed_color(&self, idx: u8) -> Rgba {
        match idx {
            // Standard ANSI colors
            0..=15 => self.ansi[idx as usize],
            // 216 color cube (6x6x6)
            16..=231 => {
                let idx = idx - 16;
                let r = (idx / 36) % 6;
                let g = (idx / 6) % 6;
                let b = idx % 6;
                let to_component = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
                rgba(to_component(r), to_component(g), to_component(b))
            }
            // Grayscale (24 shades)
            232..=255 => {
                let gray = 8 + (idx - 232) * 10;
                rgba(gray, gray, gray)
            }
        }
    }
}

/// Helper to create Rgba from u8 components
fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

fn rgba_from_theme_rgb(color: Rgb8) -> Rgba {
    rgba(color.r, color.g, color.b)
}
