use crate::model::settings::AppearanceColor;
pub use crate::model::settings::{ThemeChoice, ThemeVariant};
use iced::highlighter;
use iced::{Color, Theme, theme};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const OS_THEME_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub const CARBON_FROST_THEME_NAME: &str = "Carbon Frost";
pub const CARBON_FROST_NIGHT_THEME_NAME: &str = "Carbon Frost Night";
pub const CARBON_FROST_ASH_THEME_NAME: &str = "Carbon Frost Ash";

const CARBON_PRIMARY: Color = Color::from_rgb8(0x19, 0x19, 0x19);
const CARBON_SECONDARY: Color = Color::from_rgb8(0x25, 0x25, 0x25);
const CARBON_SURFACE_1: Color = Color::from_rgb8(0x2c, 0x2c, 0x2c);
const CARBON_SURFACE_2: Color = Color::from_rgb8(0x33, 0x33, 0x33);
const CARBON_BORDER: Color = Color::from_rgb8(0x3b, 0x3b, 0x3b);
const CARBON_BORDER_STRONG: Color = Color::from_rgb8(0x45, 0x45, 0x45);
const CARBON_BLUE: Color = Color::from_rgb8(0x34, 0x98, 0xdb);
const CARBON_DANGER: Color = Color::from_rgb8(0xff, 0x6b, 0x6b);
const CARBON_TEXT: Color = Color::from_rgb8(0xe6, 0xe6, 0xe6);
const CARBON_DANGER_TEXT: Color = Color::from_rgb8(0xff, 0x6b, 0x6b);

const CARBON_NIGHT_PRIMARY: Color = Color::from_rgb8(0x0f, 0x10, 0x12);
const CARBON_NIGHT_SECONDARY: Color = Color::from_rgb8(0x14, 0x16, 0x18);
const CARBON_NIGHT_SURFACE_1: Color = Color::from_rgb8(0x1b, 0x1d, 0x20);
const CARBON_NIGHT_SURFACE_2: Color = Color::from_rgb8(0x22, 0x24, 0x28);
const CARBON_NIGHT_BORDER: Color = Color::from_rgb8(0x2b, 0x2e, 0x33);
const CARBON_NIGHT_BORDER_STRONG: Color = Color::from_rgb8(0x36, 0x3a, 0x40);
const CARBON_NIGHT_BLUE: Color = Color::from_rgb8(0x4a, 0xa5, 0xeb);
const CARBON_NIGHT_DANGER: Color = Color::from_rgb8(0xff, 0x7a, 0x7a);

const CARBON_ASH_PRIMARY: Color = Color::from_rgb8(0xf4, 0xf4, 0xf3);
const CARBON_ASH_SECONDARY: Color = Color::from_rgb8(0xec, 0xec, 0xea);
const CARBON_ASH_SURFACE_1: Color = Color::from_rgb8(0xe4, 0xe4, 0xe1);
const CARBON_ASH_SURFACE_2: Color = Color::from_rgb8(0xdc, 0xdc, 0xd8);
const CARBON_ASH_BORDER: Color = Color::from_rgb8(0xc8, 0xc8, 0xc3);
const CARBON_ASH_BORDER_STRONG: Color = Color::from_rgb8(0xb2, 0xb2, 0xac);
const CARBON_ASH_BLUE: Color = Color::from_rgb8(0x1f, 0x6a, 0xe2);
const CARBON_ASH_DANGER: Color = Color::from_rgb8(0xd9, 0x56, 0x4e);
const CARBON_ASH_TEXT: Color = Color::from_rgb8(0x1f, 0x22, 0x26);
const CARBON_ASH_DANGER_TEXT: Color = Color::from_rgb8(0xb8, 0x3d, 0x36);

const AYU_DARK_BACKGROUND: Color = color_hex(0x0f1419);
const AYU_DARK_TEXT: Color = color_hex(0xe6e1cf);
const AYU_DARK_PRIMARY: Color = color_hex(0xf29718);
const AYU_DARK_SUCCESS: Color = color_hex(0xb8cc52);
const AYU_DARK_WARNING: Color = color_hex(0xffb454);
const AYU_DARK_DANGER: Color = color_hex(0xff3333);
const AYU_DARK_PANEL: Color = color_hex(0x14191f);
const AYU_DARK_LINE: Color = color_hex(0x151a1e);
const AYU_DARK_SELECTION: Color = color_hex(0x253340);
const AYU_DARK_BORDER: Color = color_hex(0x2d3640);

const AYU_MIRAGE_BACKGROUND: Color = color_hex(0x212733);
const AYU_MIRAGE_TEXT: Color = color_hex(0xd9d7ce);
const AYU_MIRAGE_PRIMARY: Color = color_hex(0xffcc66);
const AYU_MIRAGE_SUCCESS: Color = color_hex(0xbbe67e);
const AYU_MIRAGE_WARNING: Color = color_hex(0xffd57f);
const AYU_MIRAGE_DANGER: Color = color_hex(0xff3333);
const AYU_MIRAGE_PANEL: Color = color_hex(0x272d38);
const AYU_MIRAGE_LINE: Color = color_hex(0x242b38);
const AYU_MIRAGE_SELECTION: Color = color_hex(0x343f4c);
const AYU_MIRAGE_BORDER: Color = color_hex(0x3d4751);

const AYU_LIGHT_BACKGROUND: Color = color_hex(0xfafafa);
const AYU_LIGHT_TEXT: Color = color_hex(0x5c6773);
const AYU_LIGHT_PRIMARY: Color = color_hex(0xff6a00);
const AYU_LIGHT_SUCCESS: Color = color_hex(0x86b300);
const AYU_LIGHT_WARNING: Color = color_hex(0xf29718);
const AYU_LIGHT_DANGER: Color = color_hex(0xff3333);
const AYU_LIGHT_PANEL: Color = color_hex(0xffffff);
const AYU_LIGHT_LINE: Color = color_hex(0xf3f3f3);
const AYU_LIGHT_SELECTION: Color = color_hex(0xf0eee4);
const AYU_LIGHT_BORDER: Color = color_hex(0xd9d8d7);

const BASE24_GRUVBOX_DARK_BASE00: Color = color_hex(0x282828);
const BASE24_GRUVBOX_DARK_BASE01: Color = color_hex(0x3c3836);
const BASE24_GRUVBOX_DARK_BASE02: Color = color_hex(0x504945);
const BASE24_GRUVBOX_DARK_BASE03: Color = color_hex(0x665c54);
const BASE24_GRUVBOX_DARK_BASE05: Color = color_hex(0xebdbb2);
const BASE24_GRUVBOX_DARK_BASE08: Color = color_hex(0xcc241d);
const BASE24_GRUVBOX_DARK_BASE0A: Color = color_hex(0xd79921);
const BASE24_GRUVBOX_DARK_BASE0B: Color = color_hex(0x98971a);
const BASE24_GRUVBOX_DARK_BASE0D: Color = color_hex(0x458588);

const BASE24_GRUVBOX_LIGHT_BASE00: Color = color_hex(0xfbf1c7);
const BASE24_GRUVBOX_LIGHT_BASE01: Color = color_hex(0xebdbb2);
const BASE24_GRUVBOX_LIGHT_BASE02: Color = color_hex(0xd5c4a1);
const BASE24_GRUVBOX_LIGHT_BASE03: Color = color_hex(0xbdae93);
const BASE24_GRUVBOX_LIGHT_BASE05: Color = color_hex(0x3c3836);
const BASE24_GRUVBOX_LIGHT_BASE08: Color = color_hex(0xcc241d);
const BASE24_GRUVBOX_LIGHT_BASE0A: Color = color_hex(0xd79921);
const BASE24_GRUVBOX_LIGHT_BASE0B: Color = color_hex(0x98971a);
const BASE24_GRUVBOX_LIGHT_BASE0D: Color = color_hex(0x458588);

const BASE16_MOCHA_BACKGROUND: Color = Color::from_rgb8(0x3b, 0x32, 0x28);
const BASE16_MOCHA_TEXT: Color = Color::from_rgb8(0xd0, 0xc8, 0xc6);
const BASE16_MOCHA_PRIMARY: Color = Color::from_rgb8(0x8a, 0xb3, 0xb5);
const BASE16_MOCHA_SUCCESS: Color = Color::from_rgb8(0xbe, 0xb5, 0x5b);
const BASE16_MOCHA_WARNING: Color = Color::from_rgb8(0xf4, 0xbc, 0x87);
const BASE16_MOCHA_DANGER: Color = Color::from_rgb8(0xcb, 0x60, 0x77);

const BASE16_OCEAN_BACKGROUND: Color = Color::from_rgb8(0x2b, 0x30, 0x3b);
const BASE16_OCEAN_TEXT: Color = Color::from_rgb8(0xc0, 0xc5, 0xce);
const BASE16_OCEAN_PRIMARY: Color = Color::from_rgb8(0x8f, 0xa1, 0xb3);
const BASE16_OCEAN_SUCCESS: Color = Color::from_rgb8(0xa3, 0xbe, 0x8c);
const BASE16_OCEAN_WARNING: Color = Color::from_rgb8(0xeb, 0xcb, 0x8b);
const BASE16_OCEAN_DANGER: Color = Color::from_rgb8(0xbf, 0x61, 0x6a);

const BASE16_EIGHTIES_BACKGROUND: Color = Color::from_rgb8(0x2d, 0x2d, 0x2d);
const BASE16_EIGHTIES_TEXT: Color = Color::from_rgb8(0xd3, 0xd0, 0xc8);
const BASE16_EIGHTIES_PRIMARY: Color = Color::from_rgb8(0x66, 0x99, 0xcc);
const BASE16_EIGHTIES_SUCCESS: Color = Color::from_rgb8(0x99, 0xcc, 0x99);
const BASE16_EIGHTIES_WARNING: Color = Color::from_rgb8(0xff, 0xcc, 0x66);
const BASE16_EIGHTIES_DANGER: Color = Color::from_rgb8(0xf2, 0x77, 0x7a);

static MODERN_VARIANT: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_theme_variant(variant: ThemeVariant) {
    MODERN_VARIANT.store(variant == ThemeVariant::Modern, Ordering::Relaxed);
}

pub(crate) fn is_modern() -> bool {
    MODERN_VARIANT.load(Ordering::Relaxed)
}

pub(crate) fn ui_radius() -> f32 {
    if is_modern() { 0.0 } else { crate::UI_RADIUS }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tokens {
    pub bg: Color,
    pub chrome: Color,
    pub chrome2: Color,
    pub hover: Color,
    pub sel: Color,
    pub row: Color,
    pub line: Color,
    pub line_weak: Color,
    pub fg: Color,
    pub body: Color,
    pub muted: Color,
    pub ghost: Color,
    pub accent: Color,
    pub danger: Color,
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    Color::from_rgb(
        from.r + (to.r - from.r) * amount,
        from.g + (to.g - from.g) * amount,
        from.b + (to.b - from.b) * amount,
    )
}

pub(crate) fn tokens(theme: &Theme) -> Tokens {
    let palette = theme.palette();
    let extended = theme.extended_palette();
    let bg = extended.background.base.color;
    let fg = extended.background.base.text;
    let line = extended.background.strong.color;

    Tokens {
        bg,
        chrome: extended.background.weak.color,
        chrome2: extended.background.weakest.color,
        hover: extended.background.weaker.color,
        sel: mix(bg, extended.primary.base.color, 0.18),
        row: mix(bg, extended.primary.base.color, 0.08),
        line,
        line_weak: mix(bg, line, 0.45),
        fg,
        body: mix(fg, bg, 0.16),
        muted: mix(fg, bg, 0.34),
        ghost: mix(fg, bg, 0.68),
        accent: extended.primary.base.color,
        danger: palette.danger,
    }
}

impl ThemeChoice {
    pub fn highlighter(self) -> highlighter::Theme {
        match self {
            Self::SolarizedDark
            | Self::CarbonFrost
            | Self::CarbonFrostNight
            | Self::CarbonFrostAsh => highlighter::Theme::SolarizedDark,
            Self::CatppuccinLatte => highlighter::Theme::InspiredGitHub,
            Self::CatppuccinFrappe => highlighter::Theme::Base16Ocean,
            Self::CatppuccinMacchiato => highlighter::Theme::Base16Eighties,
            Self::CatppuccinMocha => highlighter::Theme::Base16Mocha,
            Self::TokyoNight | Self::TokyoNightStorm => highlighter::Theme::Base16Ocean,
            Self::TokyoNightLight => highlighter::Theme::InspiredGitHub,
            Self::AyuLight => highlighter::Theme::InspiredGitHub,
            Self::AyuMirage | Self::AyuDark => highlighter::Theme::Base16Mocha,
            Self::Base16Mocha => highlighter::Theme::Base16Mocha,
            Self::Base16Ocean => highlighter::Theme::Base16Ocean,
            Self::Base16Eighties => highlighter::Theme::Base16Eighties,
            Self::Base24GruvboxDark => highlighter::Theme::Base16Eighties,
            Self::Base24GruvboxLight => highlighter::Theme::InspiredGitHub,
            Self::InspiredGitHub => highlighter::Theme::InspiredGitHub,
        }
    }

    pub fn ui_theme(self) -> Theme {
        let theme = self.base_ui_theme();
        if matches!(
            self,
            Self::CarbonFrost | Self::CarbonFrostNight | Self::CarbonFrostAsh
        ) {
            theme
        } else {
            invert_surface_ramp(theme)
        }
    }

    fn base_ui_theme(self) -> Theme {
        match self {
            Self::CarbonFrost => carbon_theme(),
            Self::CarbonFrostNight => carbon_night_theme(),
            Self::CarbonFrostAsh => carbon_ash_theme(),
            Self::CatppuccinLatte => {
                catppuccin_theme("Catppuccin Latte", catppuccin::PALETTE.latte)
            }
            Self::CatppuccinFrappe => {
                catppuccin_theme("Catppuccin Frappe", catppuccin::PALETTE.frappe)
            }
            Self::CatppuccinMacchiato => {
                catppuccin_theme("Catppuccin Macchiato", catppuccin::PALETTE.macchiato)
            }
            Self::CatppuccinMocha => {
                catppuccin_theme("Catppuccin Mocha", catppuccin::PALETTE.mocha)
            }
            Self::TokyoNight => Theme::TokyoNight,
            Self::TokyoNightStorm => Theme::TokyoNightStorm,
            Self::TokyoNightLight => Theme::TokyoNightLight,
            Self::AyuLight => ayu_light_theme(),
            Self::AyuMirage => ayu_mirage_theme(),
            Self::AyuDark => ayu_dark_theme(),
            Self::SolarizedDark => Theme::SolarizedDark,
            Self::Base16Mocha => base16_mocha_theme(),
            Self::Base16Ocean => base16_ocean_theme(),
            Self::Base16Eighties => base16_eighties_theme(),
            Self::Base24GruvboxDark => base24_gruvbox_dark_theme(),
            Self::Base24GruvboxLight => base24_gruvbox_light_theme(),
            _ => {
                if self.highlighter().is_dark() {
                    Theme::Dark
                } else {
                    Theme::Light
                }
            }
        }
    }
}

pub fn is_carbon_theme(active_theme: &Theme) -> bool {
    matches!(
        iced::theme::Base::name(active_theme),
        CARBON_FROST_THEME_NAME | CARBON_FROST_NIGHT_THEME_NAME | CARBON_FROST_ASH_THEME_NAME
    )
}

pub fn is_carbon_ash_theme(active_theme: &Theme) -> bool {
    matches!(
        iced::theme::Base::name(active_theme),
        CARBON_FROST_ASH_THEME_NAME
    )
}

pub fn is_dark_theme(theme: &Theme) -> bool {
    let palette = theme.extended_palette();
    color_luminance(palette.background.base.color) < 0.5
}

pub fn detect_os_dark_mode() -> bool {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

    let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    match *cache {
        Some((checked_at, value)) => {
            if checked_at.elapsed() >= OS_THEME_POLL_INTERVAL {
                *cache = Some((Instant::now(), value));
                drop(cache);
                std::thread::spawn(|| {
                    let fresh = probe_os_dark_mode();
                    *CACHE.lock().unwrap_or_else(|error| error.into_inner()) =
                        Some((Instant::now(), fresh));
                });
            }
            value
        }
        None => {
            let value = probe_os_dark_mode();
            *cache = Some((Instant::now(), value));
            value
        }
    }
}

fn probe_os_dark_mode() -> bool {
    #[cfg(target_os = "linux")]
    {
        detect_gtk_dark_mode().unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        detect_macos_dark_mode().unwrap_or(false)
    }
    #[cfg(target_os = "windows")]
    {
        detect_windows_dark_mode().unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

#[cfg(target_os = "linux")]
fn detect_gtk_dark_mode() -> Option<bool> {
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .and_then(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.contains("prefer-dark") {
                Some(true)
            } else if stdout.contains("prefer-light") || stdout.contains("default") {
                Some(false)
            } else {
                None
            }
        })
}

#[cfg(target_os = "macos")]
fn detect_macos_dark_mode() -> Option<bool> {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()
        .map(|output| output.status.success())
}

#[cfg(target_os = "windows")]
fn detect_windows_dark_mode() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.contains("0x1") {
        Some(false)
    } else if stdout.contains("0x0") {
        Some(true)
    } else {
        None
    }
}

pub fn carbon_button_text_color(theme: &Theme, fallback: Color) -> Color {
    if is_carbon_theme(theme) {
        if is_carbon_ash_theme(theme) {
            CARBON_ASH_TEXT
        } else {
            CARBON_TEXT
        }
    } else {
        fallback
    }
}

pub fn carbon_primary_text_color(theme: &Theme, fallback: Color) -> Color {
    if is_carbon_theme(theme) {
        if is_carbon_ash_theme(theme) {
            Color::WHITE
        } else {
            CARBON_TEXT
        }
    } else {
        fallback
    }
}

pub fn carbon_danger_text_color(theme: &Theme, fallback: Color) -> Color {
    if is_carbon_theme(theme) {
        if is_carbon_ash_theme(theme) {
            CARBON_ASH_DANGER_TEXT
        } else {
            CARBON_DANGER_TEXT
        }
    } else {
        fallback
    }
}

fn invert_surface_ramp(theme: Theme) -> Theme {
    let palette = theme.palette();
    let base_extended = *theme.extended_palette();
    let name = theme.to_string();

    Theme::custom_with_fn(name, palette, move |_| {
        let mut extended = base_extended;
        extended.background.weak = base_extended.background.weakest;
        extended.background.weakest = base_extended.background.weaker;
        extended.background.weaker = base_extended.background.weak;
        extended.background.neutral = extended.background.weakest;
        extended
    })
}

fn color_luminance(color: Color) -> f32 {
    (0.2126 * color.r) + (0.7152 * color.g) + (0.0722 * color.b)
}

fn carbon_theme() -> Theme {
    let palette = theme::Palette {
        background: CARBON_PRIMARY,
        text: CARBON_TEXT,
        primary: CARBON_BLUE,
        success: CARBON_BLUE,
        warning: CARBON_BLUE,
        danger: CARBON_DANGER,
    };

    Theme::custom_with_fn(CARBON_FROST_THEME_NAME, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;

        extended.background.base = theme::palette::Pair::new(CARBON_PRIMARY, text);
        extended.background.weakest = theme::palette::Pair::new(CARBON_SURFACE_1, text);
        extended.background.weaker = theme::palette::Pair::new(CARBON_SURFACE_2, text);
        extended.background.weak = theme::palette::Pair::new(CARBON_SECONDARY, text);
        extended.background.neutral = theme::palette::Pair::new(CARBON_SURFACE_1, text);
        extended.background.strong = theme::palette::Pair::new(CARBON_BORDER, text);
        extended.background.stronger = theme::palette::Pair::new(CARBON_BORDER_STRONG, text);
        extended.background.strongest = theme::palette::Pair::new(CARBON_BORDER_STRONG, text);

        extended
    })
}

fn carbon_night_theme() -> Theme {
    let palette = theme::Palette {
        background: CARBON_NIGHT_PRIMARY,
        text: CARBON_TEXT,
        primary: CARBON_NIGHT_BLUE,
        success: CARBON_NIGHT_BLUE,
        warning: CARBON_NIGHT_BLUE,
        danger: CARBON_NIGHT_DANGER,
    };

    Theme::custom_with_fn(CARBON_FROST_NIGHT_THEME_NAME, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;

        extended.background.base = theme::palette::Pair::new(CARBON_NIGHT_PRIMARY, text);
        extended.background.weakest = theme::palette::Pair::new(CARBON_NIGHT_SURFACE_1, text);
        extended.background.weaker = theme::palette::Pair::new(CARBON_NIGHT_SURFACE_2, text);
        extended.background.weak = theme::palette::Pair::new(CARBON_NIGHT_SECONDARY, text);
        extended.background.neutral = theme::palette::Pair::new(CARBON_NIGHT_SURFACE_1, text);
        extended.background.strong = theme::palette::Pair::new(CARBON_NIGHT_BORDER, text);
        extended.background.stronger = theme::palette::Pair::new(CARBON_NIGHT_BORDER_STRONG, text);
        extended.background.strongest = theme::palette::Pair::new(CARBON_NIGHT_BORDER_STRONG, text);

        extended
    })
}

fn carbon_ash_theme() -> Theme {
    let palette = theme::Palette {
        background: CARBON_ASH_PRIMARY,
        text: CARBON_ASH_TEXT,
        primary: CARBON_ASH_BLUE,
        success: CARBON_ASH_BLUE,
        warning: CARBON_ASH_BLUE,
        danger: CARBON_ASH_DANGER,
    };

    Theme::custom_with_fn(CARBON_FROST_ASH_THEME_NAME, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;
        let white = Color::WHITE;

        extended.background.base = theme::palette::Pair::new(CARBON_ASH_PRIMARY, text);
        extended.background.weakest = theme::palette::Pair::new(CARBON_ASH_SURFACE_1, text);
        extended.background.weaker = theme::palette::Pair::new(CARBON_ASH_SURFACE_2, text);
        extended.background.weak = theme::palette::Pair::new(CARBON_ASH_SECONDARY, text);
        extended.background.neutral = theme::palette::Pair::new(CARBON_ASH_SURFACE_1, text);
        extended.background.strong = theme::palette::Pair::new(CARBON_ASH_BORDER, text);
        extended.background.stronger = theme::palette::Pair::new(CARBON_ASH_BORDER_STRONG, text);
        extended.background.strongest = theme::palette::Pair::new(CARBON_ASH_BORDER_STRONG, text);

        extended.primary.base = theme::palette::Pair::new(CARBON_ASH_BLUE, white);
        extended.primary.weak = theme::palette::Pair::new(CARBON_ASH_BLUE, white);
        extended.primary.strong = theme::palette::Pair::new(CARBON_ASH_BLUE, white);

        extended
    })
}

fn catppuccin_color(color: catppuccin::Color) -> Color {
    let rgb = color.rgb;
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

fn catppuccin_theme(name: &'static str, flavor: catppuccin::Flavor) -> Theme {
    let colors = flavor.colors;
    let palette = theme::Palette {
        background: catppuccin_color(colors.base),
        text: catppuccin_color(colors.text),
        primary: catppuccin_color(colors.blue),
        success: catppuccin_color(colors.green),
        warning: catppuccin_color(colors.yellow),
        danger: catppuccin_color(colors.red),
    };

    Theme::custom_with_fn(name, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;
        let surface0 = catppuccin_color(colors.surface0);
        let surface1 = catppuccin_color(colors.surface1);
        let surface2 = catppuccin_color(colors.surface2);

        extended.background.weakest = theme::palette::Pair::new(surface0, text);
        extended.background.weaker = theme::palette::Pair::new(surface1, text);
        extended.background.weak = theme::palette::Pair::new(surface2, text);

        extended
    })
}

#[allow(clippy::too_many_arguments)]
fn ayu_theme(
    name: &'static str,
    background: Color,
    text: Color,
    primary: Color,
    success: Color,
    warning: Color,
    danger: Color,
    panel: Color,
    line: Color,
    selection: Color,
    border: Color,
) -> Theme {
    let palette = theme::Palette {
        background,
        text,
        primary,
        success,
        warning,
        danger,
    };

    Theme::custom_with_fn(name, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;

        extended.background.weakest = theme::palette::Pair::new(panel, text);
        extended.background.weaker = theme::palette::Pair::new(line, text);
        extended.background.weak = theme::palette::Pair::new(selection, text);
        extended.background.strong = theme::palette::Pair::new(border, text);
        extended.background.stronger = theme::palette::Pair::new(border, text);
        extended.background.strongest = theme::palette::Pair::new(border, text);

        extended
    })
}

fn ayu_dark_theme() -> Theme {
    ayu_theme(
        "Ayu Dark",
        AYU_DARK_BACKGROUND,
        AYU_DARK_TEXT,
        AYU_DARK_PRIMARY,
        AYU_DARK_SUCCESS,
        AYU_DARK_WARNING,
        AYU_DARK_DANGER,
        AYU_DARK_PANEL,
        AYU_DARK_LINE,
        AYU_DARK_SELECTION,
        AYU_DARK_BORDER,
    )
}

fn ayu_mirage_theme() -> Theme {
    ayu_theme(
        "Ayu Mirage",
        AYU_MIRAGE_BACKGROUND,
        AYU_MIRAGE_TEXT,
        AYU_MIRAGE_PRIMARY,
        AYU_MIRAGE_SUCCESS,
        AYU_MIRAGE_WARNING,
        AYU_MIRAGE_DANGER,
        AYU_MIRAGE_PANEL,
        AYU_MIRAGE_LINE,
        AYU_MIRAGE_SELECTION,
        AYU_MIRAGE_BORDER,
    )
}

fn ayu_light_theme() -> Theme {
    ayu_theme(
        "Ayu Light",
        AYU_LIGHT_BACKGROUND,
        AYU_LIGHT_TEXT,
        AYU_LIGHT_PRIMARY,
        AYU_LIGHT_SUCCESS,
        AYU_LIGHT_WARNING,
        AYU_LIGHT_DANGER,
        AYU_LIGHT_PANEL,
        AYU_LIGHT_LINE,
        AYU_LIGHT_SELECTION,
        AYU_LIGHT_BORDER,
    )
}

#[allow(clippy::too_many_arguments)]
fn base24_theme(
    name: &'static str,
    background: Color,
    text: Color,
    primary: Color,
    success: Color,
    warning: Color,
    danger: Color,
    surface0: Color,
    surface1: Color,
    surface2: Color,
) -> Theme {
    let palette = theme::Palette {
        background,
        text,
        primary,
        success,
        warning,
        danger,
    };

    Theme::custom_with_fn(name, palette, |palette| {
        let mut extended = theme::palette::Extended::generate(palette);
        let text = palette.text;

        extended.background.weakest = theme::palette::Pair::new(surface0, text);
        extended.background.weaker = theme::palette::Pair::new(surface1, text);
        extended.background.weak = theme::palette::Pair::new(surface2, text);

        extended
    })
}

fn base24_gruvbox_dark_theme() -> Theme {
    base24_theme(
        "Base24 Gruvbox Dark",
        BASE24_GRUVBOX_DARK_BASE00,
        BASE24_GRUVBOX_DARK_BASE05,
        BASE24_GRUVBOX_DARK_BASE0D,
        BASE24_GRUVBOX_DARK_BASE0B,
        BASE24_GRUVBOX_DARK_BASE0A,
        BASE24_GRUVBOX_DARK_BASE08,
        BASE24_GRUVBOX_DARK_BASE01,
        BASE24_GRUVBOX_DARK_BASE02,
        BASE24_GRUVBOX_DARK_BASE03,
    )
}

fn base24_gruvbox_light_theme() -> Theme {
    base24_theme(
        "Base24 Gruvbox Light",
        BASE24_GRUVBOX_LIGHT_BASE00,
        BASE24_GRUVBOX_LIGHT_BASE05,
        BASE24_GRUVBOX_LIGHT_BASE0D,
        BASE24_GRUVBOX_LIGHT_BASE0B,
        BASE24_GRUVBOX_LIGHT_BASE0A,
        BASE24_GRUVBOX_LIGHT_BASE08,
        BASE24_GRUVBOX_LIGHT_BASE01,
        BASE24_GRUVBOX_LIGHT_BASE02,
        BASE24_GRUVBOX_LIGHT_BASE03,
    )
}

fn base16_theme(
    name: &'static str,
    background: Color,
    text: Color,
    primary: Color,
    success: Color,
    warning: Color,
    danger: Color,
) -> Theme {
    Theme::custom(
        name,
        theme::Palette {
            background,
            text,
            primary,
            success,
            warning,
            danger,
        },
    )
}

fn base16_mocha_theme() -> Theme {
    base16_theme(
        "Base16 Mocha",
        BASE16_MOCHA_BACKGROUND,
        BASE16_MOCHA_TEXT,
        BASE16_MOCHA_PRIMARY,
        BASE16_MOCHA_SUCCESS,
        BASE16_MOCHA_WARNING,
        BASE16_MOCHA_DANGER,
    )
}

fn base16_ocean_theme() -> Theme {
    base16_theme(
        "Base16 Ocean",
        BASE16_OCEAN_BACKGROUND,
        BASE16_OCEAN_TEXT,
        BASE16_OCEAN_PRIMARY,
        BASE16_OCEAN_SUCCESS,
        BASE16_OCEAN_WARNING,
        BASE16_OCEAN_DANGER,
    )
}

fn base16_eighties_theme() -> Theme {
    base16_theme(
        "Base16 Eighties",
        BASE16_EIGHTIES_BACKGROUND,
        BASE16_EIGHTIES_TEXT,
        BASE16_EIGHTIES_PRIMARY,
        BASE16_EIGHTIES_SUCCESS,
        BASE16_EIGHTIES_WARNING,
        BASE16_EIGHTIES_DANGER,
    )
}

const fn color_hex(hex: u32) -> Color {
    Color::from_rgb8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub(crate) fn appearance_color(value: AppearanceColor, fallback: Color) -> Color {
    match value {
        AppearanceColor::Default => fallback,
        AppearanceColor::Blue => color_hex(0x3d9eff),
        AppearanceColor::Cyan => color_hex(0x22d3ee),
        AppearanceColor::Teal => color_hex(0x14b8a6),
        AppearanceColor::Green => color_hex(0x22c55e),
        AppearanceColor::Amber => color_hex(0xf59e0b),
        AppearanceColor::Orange => color_hex(0xf97316),
        AppearanceColor::Red => color_hex(0xef4444),
        AppearanceColor::Purple => color_hex(0xa855f7),
        AppearanceColor::Pink => color_hex(0xec4899),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppearanceMode {
    Dark,
    Light,
    System,
}
