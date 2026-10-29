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
pub enum Accent { #[default] Teal, Blue, Green, Amber, Violet, Rose }

impl Accent {
    pub fn parse(value: &str) -> Option<Self> {
        match value { "teal" => Some(Self::Teal), "blue" => Some(Self::Blue),
            "green" => Some(Self::Green), "amber" => Some(Self::Amber),
            "violet" => Some(Self::Violet), "rose" => Some(Self::Rose), _ => None }
    }
    pub fn key(self) -> &'static str {
        match self { Self::Teal => "teal", Self::Blue => "blue", Self::Green => "green",
            Self::Amber => "amber", Self::Violet => "violet", Self::Rose => "rose" }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Teal => "青绿", Self::Blue => "蓝色", Self::Green => "绿色",
            Self::Amber => "琥珀", Self::Violet => "紫色", Self::Rose => "玫红" }
    }
    pub fn next(self) -> Self {
        match self { Self::Teal => Self::Blue, Self::Blue => Self::Green, Self::Green => Self::Amber,
            Self::Amber => Self::Violet, Self::Violet => Self::Rose, Self::Rose => Self::Teal }
    }
    fn colors(self, dark: bool) -> (u32, u32, u32) {
        match (self, dark) {
            (Self::Teal, false) => (0x216F62, 0xDCEEE5, 0x8DB5A3),
            (Self::Teal, true) => (0x8CD6B5, 0x294D40, 0x5E9C82),
            (Self::Blue, false) => (0x2C5FA8, 0xDCE8F7, 0x8FB2D8),
            (Self::Blue, true) => (0x9CC7FF, 0x233A5A, 0x5D87B3),
            (Self::Green, false) => (0x2F774C, 0xDDEFE4, 0x8CBBA1),
            (Self::Green, true) => (0x8CD0A0, 0x24462F, 0x5C9A72),
            (Self::Amber, false) => (0x7A5208, 0xF7ECD7, 0xCEB17B),
            (Self::Amber, true) => (0xF0C784, 0x453720, 0xA8874E),
            (Self::Violet, false) => (0x6B4AA5, 0xE9E1F5, 0xAE9ACF),
            (Self::Violet, true) => (0xCDB5FF, 0x34264D, 0x8E72BF),
            (Self::Rose, false) => (0xA44260, 0xF6E1E7, 0xD19AA9),
            (Self::Rose, true) => (0xFFB0C2, 0x4A2833, 0xB87A8C),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub preference: Preference,
    pub system_dark: bool,
    pub accent: Accent,
}

impl Theme {
    pub fn dark(self) -> bool {
        self.preference == Preference::Dark || (self.preference == Preference::System && self.system_dark)
    }
    pub fn palette(self) -> Palette {
        let mut palette = if self.dark() { DARK } else { LIGHT };
        let (accent, light, line) = self.accent.colors(self.dark());
        palette.accent = accent;
        palette.accent_light = light;
        palette.accent_line = line;
        palette.on_accent = match on_color(c(accent)).to_color_u8() {
            color if color.red() == 255 => 0xFFFFFF,
            _ => 0x000000,
        };
        palette
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            assert!(!Theme { preference: Preference::Light, system_dark, ..Default::default() }.dark());
            assert!(Theme { preference: Preference::Dark, system_dark, ..Default::default() }.dark());
            assert_eq!(Theme { preference: Preference::System, system_dark, ..Default::default() }.dark(), system_dark);
        }
        for p in [Preference::Light, Preference::Dark, Preference::System] {
            assert_eq!(Preference::parse(p.key()), Some(p));
        }
        assert_eq!(Preference::parse("unknown"), None);
    }

    #[test]
    fn accent_palettes_keep_actions_readable() {
        let contrast = |a, b| {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        };
        for accent in [Accent::Teal, Accent::Blue, Accent::Green, Accent::Amber, Accent::Violet, Accent::Rose] {
            for dark in [false, true] {
                let p = Theme { preference: if dark { Preference::Dark } else { Preference::Light }, system_dark: dark, accent }.palette();
                for (fg, bg) in [(p.accent, p.accent_light), (p.on_accent, p.accent)] {
                    assert!(contrast(fg, bg) >= 4.5, "low accent contrast {:06x} on {:06x} ({accent:?}, dark={dark})", fg, bg);
                }
            }
        }
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
