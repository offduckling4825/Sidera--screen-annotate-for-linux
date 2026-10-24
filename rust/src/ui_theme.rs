//! Semantic UI colours. Actual annotation ink and whiteboard
//! backgrounds never change when the interface theme changes.
use tiny_skia::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preference { #[default] Light, Dark, System }

impl Preference {
    pub fn parse(value: &str) -> Option<Self> {
        match value { "light" => Some(Self::Light), "dark" => Some(Self::Dark),
            "system" => Some(Self::System), _ => None }
    }
    pub fn key(self) -> &'static str {
        match self { Self::Light => "light", Self::Dark => "dark", Self::System => "system" }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Light => "浅色", Self::Dark => "深色", Self::System => "跟随系统" }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub preference: Preference,
    pub system_dark: bool,
}

impl Theme {
    pub fn dark(self) -> bool {
        self.preference == Preference::Dark || (self.preference == Preference::System && self.system_dark)
    }
    pub fn palette(self) -> &'static Palette { if self.dark() { &DARK } else { &LIGHT } }
}

pub struct Palette {
    pub paper: u32, pub surface: u32, pub border: u32,
    pub text: u32, pub muted: u32,
    pub disabled_surface: u32, pub disabled_border: u32, pub disabled_ink: u32,
    pub accent: u32, pub accent_light: u32, pub accent_line: u32, pub on_accent: u32,
    pub success: u32, pub on_success: u32,
    pub danger: u32, pub danger_light: u32, pub warning: u32, pub warning_light: u32,
    pub eraser: u32, pub splash_header: u32,
}

pub const LIGHT: Palette = Palette {
    paper: 0xFAFBF7, surface: 0xF0F4EF, border: 0xCBD7CE,
    text: 0x263E3A, muted: 0x5C7069,
    disabled_surface: 0xECEEEA, disabled_border: 0xD7DAD5, disabled_ink: 0x9CA39E,
    accent: 0x216F62, accent_light: 0xDCEEE5, accent_line: 0x8DB5A3, on_accent: 0xFFFFFF,
    success: 0x2F774C, on_success: 0xFFFFFF,
    danger: 0xA44234, danger_light: 0xF8E9E3, warning: 0x906017, warning_light: 0xFFF0D7,
    eraser: 0xD5E5EB, splash_header: 0xEDF4EE,
};

pub const DARK: Palette = Palette {
    paper: 0x1D2B27, surface: 0x293C34, border: 0x4B6659,
    text: 0xECF4EE, muted: 0xB3C7BB,
    disabled_surface: 0x243028, disabled_border: 0x34443A, disabled_ink: 0x768A7C,
    accent: 0x8CD6B5, accent_light: 0x294D40, accent_line: 0x5E9C82, on_accent: 0x102D22,
    success: 0x8CD0A0, on_success: 0x142F20,
    danger: 0xFFAB98, danger_light: 0x4B2D29, warning: 0xF0C784, warning_light: 0x473A25,
    eraser: 0x476578, splash_header: 0x253D32,
};

pub fn alpha(rgb: u32, a: u8) -> Color {
    Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
}
pub fn c(rgb: u32) -> Color { alpha(rgb, 255) }

fn luminance(rgb: u32) -> f32 {
    let channel = |shift: u32| {
        let s = ((rgb >> shift) & 255u32) as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// Checkmarks and page labels sit on user-selected colours, not UI surfaces.
/// Pick black/white by actual sRGB contrast, independent of the selected theme.
pub fn on_color(background: Color) -> Color {
    let rgb = background.to_color_u8();
    let rgb = (u32::from(rgb.red()) << 16) | (u32::from(rgb.green()) << 8) | u32::from(rgb.blue());
    c(if luminance(rgb) > 0.179 { 0x000000 } else { 0xFFFFFF })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_preferences_override_system_and_unknown_values_are_not_silently_saved() {
        for system_dark in [false, true] {
            assert!(!Theme { preference: Preference::Light, system_dark }.dark());
            assert!(Theme { preference: Preference::Dark, system_dark }.dark());
            assert_eq!(Theme { preference: Preference::System, system_dark }.dark(), system_dark);
        }
        for p in [Preference::Light, Preference::Dark, Preference::System] {
            assert_eq!(Preference::parse(p.key()), Some(p));
        }
        assert_eq!(Preference::parse("unknown"), None);
    }

    #[test]
    fn both_palettes_keep_text_and_actions_readable() {
        let contrast = |a, b| {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        };
        for p in [&LIGHT, &DARK] {
            for (fg, bg) in [(p.text, p.paper), (p.muted, p.paper), (p.text, p.surface),
                (p.muted, p.surface), (p.accent, p.accent_light), (p.on_accent, p.accent),
                (p.on_success, p.success), (p.danger, p.danger_light), (p.warning, p.warning_light),
                (p.text, p.eraser), (p.muted, p.splash_header)] {
                assert!(contrast(fg, bg) >= 4.5, "low contrast {fg:06x} on {bg:06x}");
            }
            // Minimum opacity used by the page wheel's neighbouring numbers.
            let channel = |shift| (((p.text >> shift) & 255u32) as f32 * 0.8
                + ((p.surface >> shift) & 255u32) as f32 * 0.2).round() as u32;
            let faded_text = (channel(16) << 16) | (channel(8) << 8) | channel(0);
            assert!(contrast(faded_text, p.surface) >= 4.5);
        }
        for rgb in [0x000000, 0xFFFFFF, 0x0F3D2E, 0x111D19, 0xFF0000, 0xFF8800, 0x0080FF] {
            let chosen = on_color(c(rgb)).to_color_u8();
            let fg = if chosen.red() == 0 { 0 } else { 0xFFFFFF };
            assert!(contrast(fg, rgb) >= 4.5);
        }
    }
}
