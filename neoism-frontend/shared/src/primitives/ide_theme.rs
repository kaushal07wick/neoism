//! Per-theme palette shared across chrome panels and the syntax
//! highlighter. Mirrors `nvim_runtime/lua/rio/theme.lua` so chrome and
//! editor paint with the same colors.
//!
//! Lifted from `frontends/neoism/src/chrome/primitives/theme.rs` so
//! native and web reach the same source of truth.

use neoism_terminal_core::colors::ColorRgb;
use sugarloaf::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdeThemeName {
    PastelDark,
    NvChadOne,
    TokyoNight,
    CatppuccinMocha,
}

impl IdeThemeName {
    /// Every selectable IDE theme, in display order. Single source of
    /// truth for theme pickers (the Cmd+P themes mode and the hamburger
    /// → Themes action) so web and desktop offer the same list without
    /// re-hardcoding it per host.
    pub const ALL: [IdeThemeName; 4] = [
        IdeThemeName::PastelDark,
        IdeThemeName::NvChadOne,
        IdeThemeName::TokyoNight,
        IdeThemeName::CatppuccinMocha,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IdeThemeName::PastelDark => "pastel_dark",
            IdeThemeName::NvChadOne => "nvchad_one",
            IdeThemeName::TokyoNight => "tokyo_night",
            IdeThemeName::CatppuccinMocha => "catppuccin_mocha",
        }
    }

    pub fn from_str(name: &str) -> Self {
        match name {
            "nvchad_one" => IdeThemeName::NvChadOne,
            "tokyo_night" => IdeThemeName::TokyoNight,
            "catppuccin_mocha" => IdeThemeName::CatppuccinMocha,
            _ => IdeThemeName::PastelDark,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IdeTheme {
    pub name: IdeThemeName,
    pub bg: u32,
    pub fg: u32,
    pub surface: u32,
    pub hover: u32,
    pub border: u32,
    pub muted: u32,
    pub dim: u32,
    pub accent: u32,
    pub folder: u32,
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
    pub white: u32,
    pub black: u32,
    pub syn_comment: u32,
    pub syn_string: u32,
    pub syn_number: u32,
    pub syn_keyword: u32,
    #[allow(dead_code)]
    pub syn_statement: u32,
    pub syn_func: u32,
    pub syn_type: u32,
    #[allow(dead_code)]
    pub syn_property: u32,
    #[allow(dead_code)]
    pub syn_constructor: u32,
    #[allow(dead_code)]
    pub syn_special: u32,
}

impl Default for IdeTheme {
    fn default() -> Self {
        Self::pastel_dark()
    }
}

impl IdeTheme {
    pub fn by_name(name: &str) -> Self {
        match IdeThemeName::from_str(name) {
            IdeThemeName::PastelDark => Self::pastel_dark(),
            IdeThemeName::NvChadOne => Self::nvchad_one(),
            IdeThemeName::TokyoNight => Self::tokyo_night(),
            IdeThemeName::CatppuccinMocha => Self::catppuccin_mocha(),
        }
    }

    pub fn pastel_dark() -> Self {
        Self {
            name: IdeThemeName::PastelDark,
            bg: 0x000000,
            fg: 0xe8e8e8,
            surface: 0x1a1a1a,
            hover: 0x1f1f1f,
            border: 0x1c1c1c,
            muted: 0x5a5a5a,
            dim: 0xb0b0b0,
            accent: 0xe8e8e8,
            folder: 0x7ebae4,
            red: 0xef8891,
            green: 0x9fe8c3,
            yellow: 0xfbdf90,
            blue: 0x99aee5,
            magenta: 0xc2a2e3,
            cyan: 0xb5c3ea,
            white: 0xb5bcc9,
            black: 0x000000,
            syn_comment: 0x7a7a7a,
            syn_string: 0x9fe8c3,
            syn_number: 0xeda685,
            syn_keyword: 0xc2a2e3,
            syn_statement: 0xc2a2e3,
            syn_func: 0x99aee5,
            syn_type: 0xfbdf90,
            syn_property: 0x99aee5,
            syn_constructor: 0xb5c3ea,
            syn_special: 0xef8891,
        }
    }

    pub fn nvchad_one() -> Self {
        Self {
            name: IdeThemeName::NvChadOne,
            bg: 0x1e222a,
            fg: 0xabb2bf,
            surface: 0x282c34,
            hover: 0x353b45,
            border: 0x31353d,
            muted: 0x565c64,
            dim: 0x6f737b,
            accent: 0x61afef,
            folder: 0x61afef,
            red: 0xe06c75,
            green: 0x98c379,
            yellow: 0xe7c787,
            blue: 0x61afef,
            magenta: 0xc678dd,
            cyan: 0x56b6c2,
            white: 0xabb2bf,
            black: 0x1e222a,
            syn_comment: 0x565c64,
            syn_string: 0x98c379,
            syn_number: 0xd19a66,
            syn_keyword: 0xc678dd,
            syn_statement: 0xe06c75,
            syn_func: 0x61afef,
            syn_type: 0xe5c07b,
            syn_property: 0xe06c75,
            syn_constructor: 0x56b6c2,
            syn_special: 0xbe5046,
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: IdeThemeName::TokyoNight,
            bg: 0x1a1b26,
            fg: 0xc0caf5,
            surface: 0x24283b,
            hover: 0x292e42,
            border: 0x3b4261,
            muted: 0x565f89,
            dim: 0xa9b1d6,
            accent: 0x7aa2f7,
            folder: 0x7aa2f7,
            red: 0xf7768e,
            green: 0x9ece6a,
            yellow: 0xe0af68,
            blue: 0x7aa2f7,
            magenta: 0xbb9af7,
            cyan: 0x7dcfff,
            white: 0xc0caf5,
            black: 0x11121d,
            syn_comment: 0x565f89,
            syn_string: 0x9ece6a,
            syn_number: 0xff9e64,
            syn_keyword: 0x7aa2f7,
            syn_statement: 0xbb9af7,
            syn_func: 0x7aa2f7,
            syn_type: 0x2ac3de,
            syn_property: 0x73daca,
            syn_constructor: 0x7dcfff,
            syn_special: 0xe0af68,
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: IdeThemeName::CatppuccinMocha,
            bg: 0x1e1e2e,
            fg: 0xcdd6f4,
            surface: 0x313244,
            hover: 0x45475a,
            border: 0x585b70,
            muted: 0x6c7086,
            dim: 0xa6adc8,
            accent: 0xcba6f7,
            folder: 0x89b4fa,
            red: 0xf38ba8,
            green: 0xa6e3a1,
            yellow: 0xf9e2af,
            blue: 0x89b4fa,
            magenta: 0xcba6f7,
            cyan: 0x89dceb,
            white: 0xcdd6f4,
            black: 0x11111b,
            syn_comment: 0x6c7086,
            syn_string: 0xa6e3a1,
            syn_number: 0xfab387,
            syn_keyword: 0x89b4fa,
            syn_statement: 0xcba6f7,
            syn_func: 0xf9e2af,
            syn_type: 0x94e2d5,
            syn_property: 0x89dceb,
            syn_constructor: 0x89dceb,
            syn_special: 0xf5c2e7,
        }
    }

    pub fn f32(self, color: u32) -> [f32; 4] {
        let r = ((color >> 16) & 0xff) as f32 / 255.0;
        let g = ((color >> 8) & 0xff) as f32 / 255.0;
        let b = (color & 0xff) as f32 / 255.0;
        [r, g, b, 1.0]
    }

    pub fn f32_alpha(self, color: u32, alpha: f32) -> [f32; 4] {
        let mut out = self.f32(color);
        out[3] = alpha;
        out
    }

    pub fn u8(self, color: u32) -> [u8; 4] {
        [
            ((color >> 16) & 0xff) as u8,
            ((color >> 8) & 0xff) as u8,
            (color & 0xff) as u8,
            255,
        ]
    }

    pub fn u8_alpha(self, color: u32, alpha: f32) -> [u8; 4] {
        let mut out = self.u8(color);
        out[3] = (255.0 * alpha.clamp(0.0, 1.0)) as u8;
        out
    }

    pub fn rgb(self, color: u32) -> ColorRgb {
        ColorRgb {
            r: ((color >> 16) & 0xff) as u8,
            g: ((color >> 8) & 0xff) as u8,
            b: (color & 0xff) as u8,
        }
    }

    pub fn sugar(self, color: u32) -> Color {
        Color {
            r: ((color >> 16) & 0xff) as f64 / 255.0,
            g: ((color >> 8) & 0xff) as f64 / 255.0,
            b: (color & 0xff) as f64 / 255.0,
            a: 1.0,
        }
    }

    pub fn sugar_alpha(self, color: u32, alpha: f64) -> Color {
        Color {
            r: ((color >> 16) & 0xff) as f64 / 255.0,
            g: ((color >> 8) & 0xff) as f64 / 255.0,
            b: (color & 0xff) as f64 / 255.0,
            a: alpha.clamp(0.0, 1.0),
        }
    }
}
