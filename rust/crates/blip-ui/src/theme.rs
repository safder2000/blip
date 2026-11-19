//! The light and dark themes, same colours as `THEMES` in `blip/kit.py`.

use blip_pet::sprites::Color;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color,
    pub surface: Color,
    pub line: Color,
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub on_accent: Color,
    pub hover: Color,
    pub ok: Color,
    pub warn: Color,
    pub danger: Color,
    pub off: Color,
    pub track: Color,
    pub tile: Color,
    pub knob: Color,
    pub frame: Color,
    pub shadow: Color,
    pub paper: Color,
    pub paper_ink: Color,
}

pub const DARK: Theme = Theme {
    dark: true,
    bg: 0x191919,
    surface: 0x1f1f1f,
    line: 0x303030,
    ink: 0xefefef,
    muted: 0x9c9c9c,
    accent: 0x8a9bef,
    accent_soft: 0x2e2e2e,
    on_accent: 0xffffff,
    hover: 0x272727,
    ok: 0x3ec58a,
    warn: 0xe8ad48,
    danger: 0xee6b6b,
    off: 0x5e5e5e,
    track: 0x3a3a3a,
    tile: 0x262626,
    knob: 0xffffff,
    frame: 0x404040,
    shadow: 0x0a0a0a,
    paper: 0x1f1f1f,
    paper_ink: 0xefefef,
};

pub const LIGHT: Theme = Theme {
    dark: false,
    bg: 0xf5f3fa,
    surface: 0xffffff,
    line: 0xe6e0f0,
    ink: 0x2b2340,
    muted: 0x7b7393,
    accent: 0x6b7fd7,
    accent_soft: 0xeef0fc,
    on_accent: 0xffffff,
    hover: 0xf2eff9,
    ok: 0x34b37a,
    warn: 0xeea320,
    danger: 0xe5484d,
    off: 0xb9b3c8,
    track: 0xd8d2e6,
    tile: 0xf7f5fb,
    knob: 0xffffff,
    frame: 0x2b2340,
    shadow: 0x2b2340,
    paper: 0xfffdf7,
    paper_ink: 0x2b2340,
};

/// "dark", "light" or "system" (the setting from config.json).
pub fn pick(setting: &str, system_dark: bool) -> Theme {
    match setting {
        "light" => LIGHT,
        "dark" => DARK,
        _ if system_dark => DARK,
        _ => LIGHT,
    }
}
