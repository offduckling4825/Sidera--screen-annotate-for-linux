//! Shared classroom palette. Ink colours and whiteboard paper are independent.
use tiny_skia::Color;

pub const PAPER: u32 = 0xFAFBF7;
pub const SURFACE: u32 = 0xF0F4EF;
pub const BORDER: u32 = 0xCBD7CE;
pub const TEXT: u32 = 0x263E3A;
pub const MUTED: u32 = 0x5C7069;
pub const DISABLED_SURFACE: u32 = 0xECEEEA;
pub const DISABLED_BORDER: u32 = 0xD7DAD5;
pub const DISABLED_INK: u32 = 0x9CA39E;
pub const ACCENT: u32 = 0x216F62;
pub const ACCENT_LIGHT: u32 = 0xDCEEE5;
pub const ACCENT_LINE: u32 = 0x8DB5A3;
pub const SUCCESS: u32 = 0x2F774C;
pub const DANGER: u32 = 0xA44234;
pub const DANGER_LIGHT: u32 = 0xF8E9E3;
pub const WARNING: u32 = 0x906017;
pub const WARNING_LIGHT: u32 = 0xFFF0D7;
pub const ERASER: u32 = 0xD5E5EB;
pub const WHITE: u32 = 0xFFFFFF;

pub fn alpha(rgb: u32, a: u8) -> Color {
    Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
}

pub fn c(rgb: u32) -> Color { alpha(rgb, 255) }
