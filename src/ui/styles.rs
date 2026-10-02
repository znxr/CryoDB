use crate::ToastLevel;
use crate::ui::changelog::ChangeKind;
use crate::ui::theme::{
    carbon_button_text_color, carbon_danger_text_color, carbon_primary_text_color,
    is_carbon_ash_theme, is_carbon_theme, is_dark_theme, is_modern, tokens, ui_radius,
};
use iced::border::Radius;
use iced::widget::overlay::menu;
use iced::widget::{button, container, pick_list, scrollable, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

fn flat_container(background: Color, text_color: Color, border_color: Color) -> container::Style {
    container::Style {
        background: Some(background.into()),
        text_color: Some(text_color),
        border: Border {
            width: 1.0,
            radius: 0.0.into(),
            color: border_color,
        },
        ..container::Style::default()
    }
}

fn flat_button(background: Color, text_color: Color, border_color: Color) -> button::Style {
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            width: if border_color == Color::TRANSPARENT {
                0.0
            } else {
                1.0
            },
            radius: 0.0.into(),
            color: border_color,
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub(crate) fn panel_style(theme: &Theme) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let mut style = flat_container(t.chrome, t.body, t.line);
        style.border.width = 0.0;
        return style;
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(palette.background.weak.text),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn modal_backdrop_style(theme: &Theme, dim: f32) -> container::Style {
    let palette = theme.extended_palette();
    let mut overlay = if is_modern() {
        if is_dark_theme(theme) {
            Color::BLACK
        } else {
            tokens(theme).ghost
        }
    } else {
        palette.background.strong.color
    };
    overlay.a = dim.clamp(0.20, 1.0);

    container::Style {
        background: Some(overlay.into()),
        text_color: Some(palette.background.strong.text),
        border: Border {
            width: 0.0,
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        ..container::Style::default()
    }
}

pub(crate) fn panel_border_style(theme: &Theme) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let mut style = flat_container(t.chrome2, t.body, t.line);
        style.border.width = 0.0;
        return style;
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weakest.color.into()),
        text_color: Some(palette.background.weakest.text),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn tooltip_container_style(theme: &Theme) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        return flat_container(t.chrome2, t.body, t.line);
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.base.color.into()),
        text_color: Some(palette.background.base.text),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    }
}

fn button_base_style(background: Color, text_color: Color, border_color: Color) -> button::Style {
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: border_color,
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

fn disabled_button_style(style: button::Style) -> button::Style {
    button::Style {
        background: style
            .background
            .map(|background| background.scale_alpha(0.5)),
        text_color: style.text_color.scale_alpha(0.5),
        ..style
    }
}

pub(crate) fn compact_button_style(theme: &Theme, status: button::Status) -> button::Style {
    if is_modern() {
        return compact_button_style_with_text_color(theme, status, tokens(theme).body);
    }

    let palette = theme.extended_palette();
    let text_color = carbon_button_text_color(theme, palette.background.weakest.text);
    compact_button_style_with_text_color(theme, status, text_color)
}

fn selection_overlay_color(theme: &Theme, alpha: f32) -> Color {
    let palette = theme.extended_palette();
    let mut color = palette.primary.base.color;
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn table_selection_overlay_color(theme: &Theme, alpha: f32) -> Color {
    let mut color = if is_dark_theme(theme) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn highlighted_text_color(theme: &Theme) -> Color {
    if is_dark_theme(theme) {
        Color::WHITE
    } else {
        Color::from_rgb8(0x12, 0x14, 0x18)
    }
}

fn table_result_text_color(theme: &Theme, fallback: Color) -> Color {
    if is_carbon_ash_theme(theme) {
        Color::from_rgb8(0x1f, 0x22, 0x26)
    } else if is_carbon_theme(theme) {
        Color::from_rgb8(0xd0, 0xd0, 0xd0)
    } else {
        fallback
    }
}

pub(crate) fn compact_tab_active_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    sidebar_table_button_style(true, theme, status)
}

pub(crate) fn settings_modal_tab_button_style(
    selected: bool,
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    if selected {
        let palette = theme.extended_palette();
        compact_button_style_with_text_color(theme, status, palette.primary.base.color)
    } else {
        compact_button_style(theme, status)
    }
}

pub(crate) fn compact_button_style_with_text_color(
    theme: &Theme,
    status: button::Status,
    text_color: Color,
) -> button::Style {
    if is_modern() {
        let t = tokens(theme);
        let base = flat_button(Color::TRANSPARENT, text_color, Color::TRANSPARENT);

        return match status {
            button::Status::Active => base,
            button::Status::Hovered => button::Style {
                background: Some(Background::Color(t.hover)),
                ..base
            },
            button::Status::Pressed => button::Style {
                background: Some(Background::Color(t.sel)),
                ..base
            },
            button::Status::Disabled => disabled_button_style(base),
        };
    }

    let palette = theme.extended_palette();
    let base = button_base_style(
        palette.background.weakest.color,
        text_color,
        palette.background.strong.color,
    );

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(palette.background.weaker.color)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(palette.background.weak.color)),
            ..base
        },
        button::Status::Disabled => disabled_button_style(base),
    }
}

pub(crate) fn compact_primary_button_style(theme: &Theme, status: button::Status) -> button::Style {
    if is_modern() {
        let t = tokens(theme);
        let base = flat_button(Color::TRANSPARENT, t.accent, t.accent);

        return match status {
            button::Status::Active => base,
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(t.sel)),
                ..base
            },
            button::Status::Disabled => disabled_button_style(base),
        };
    }

    let palette = theme.extended_palette();
    let text_color = carbon_primary_text_color(theme, palette.primary.base.text);
    let base = button_base_style(
        palette.primary.base.color,
        text_color,
        palette.primary.strong.color,
    );

    match status {
        button::Status::Active | button::Status::Pressed => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(palette.primary.strong.color)),
            ..base
        },
        button::Status::Disabled => disabled_button_style(base),
    }
}

pub(crate) fn primary_foreground_color(theme: &Theme) -> Color {
    if is_modern() {
        return tokens(theme).accent;
    }
    carbon_primary_text_color(theme, theme.extended_palette().primary.base.text)
}

pub(crate) fn ai_fix_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let (base_bg, hover_bg, pressed_bg, text_color) = if is_dark_theme(theme) {
        (
            Color::from_rgb(1.0, 1.0, 1.0),
            Color::from_rgb(0.93, 0.93, 0.93),
            Color::from_rgb(0.86, 0.86, 0.86),
            Color::from_rgb(0.0, 0.0, 0.0),
        )
    } else {
        (
            Color::from_rgb(0.0, 0.0, 0.0),
            Color::from_rgb(0.13, 0.13, 0.13),
            Color::from_rgb(0.2, 0.2, 0.2),
            Color::from_rgb(1.0, 1.0, 1.0),
        )
    };
    let mut base = button_base_style(base_bg, text_color, Color::TRANSPARENT);
    base.border.width = 0.0;

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(hover_bg)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(pressed_bg)),
            ..base
        },
        button::Status::Disabled => disabled_button_style(base),
    }
}

pub(crate) fn compact_secondary_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    if is_modern() {
        return compact_button_style_with_text_color(theme, status, tokens(theme).muted);
    }

    let palette = theme.extended_palette();
    let text_color = carbon_button_text_color(theme, palette.secondary.base.text);
    let base = button_base_style(
        palette.secondary.base.color,
        text_color,
        palette.secondary.strong.color,
    );

    match status {
        button::Status::Active | button::Status::Pressed => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(palette.secondary.strong.color)),
            ..base
        },
        button::Status::Disabled => disabled_button_style(base),
    }
}

pub(crate) fn disconnect_button_style(theme: &Theme, status: button::Status) -> button::Style {
    if is_modern() {
        return compact_button_style_with_text_color(theme, status, tokens(theme).danger);
    }

    let palette = theme.extended_palette();
    let text_color = carbon_danger_text_color(theme, palette.danger.base.color);
    compact_button_style_with_text_color(theme, status, text_color)
}

pub(crate) fn compact_input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);

    if is_modern() {
        let t = tokens(theme);
        style.background = Background::Color(t.bg);
        style.value = t.fg;
        style.placeholder = t.ghost;
        style.selection = t.sel;
        style.border = Border {
            width: 1.0,
            radius: 0.0.into(),
            color: if matches!(status, text_input::Status::Focused { .. }) {
                t.accent
            } else {
                t.line
            },
        };
        return style;
    }

    style.border.radius = ui_radius().into();
    style
}

pub(crate) fn compact_pick_list_style(
    theme: &Theme,
    status: pick_list::Status,
) -> pick_list::Style {
    let button_status = match status {
        pick_list::Status::Active => button::Status::Active,
        pick_list::Status::Hovered => button::Status::Hovered,
        pick_list::Status::Opened { is_hovered } => {
            if is_hovered {
                button::Status::Hovered
            } else {
                button::Status::Pressed
            }
        }
    };
    let button_style = compact_button_style(theme, button_status);
    let background = button_style
        .background
        .unwrap_or(Background::Color(Color::TRANSPARENT));

    pick_list::Style {
        text_color: button_style.text_color,
        placeholder_color: button_style.text_color.scale_alpha(0.7),
        handle_color: button_style.text_color,
        background,
        border: button_style.border,
    }
}

pub(crate) fn compact_pick_list_menu_style(theme: &Theme) -> menu::Style {
    if is_modern() {
        let t = tokens(theme);
        return menu::Style {
            background: t.chrome2.into(),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: t.line,
            },
            text_color: t.body,
            selected_text_color: t.fg,
            selected_background: t.sel.into(),
            shadow: Shadow::default(),
        };
    }

    let palette = theme.extended_palette();
    let text_color = carbon_button_text_color(theme, palette.background.weakest.text);

    menu::Style {
        background: palette.background.weakest.color.into(),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: palette.background.strong.color,
        },
        text_color,
        selected_text_color: text_color,
        selected_background: palette.background.weaker.color.into(),
        shadow: Shadow::default(),
    }
}

pub(crate) fn table_header_style(theme: &Theme, radius: Radius) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let mut style = flat_container(t.chrome, t.fg, t.line);
        style.border.width = 0.0;
        return style;
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(palette.background.weak.text),
        border: Border {
            width: 1.0,
            radius,
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn table_row_header_style(
    theme: &Theme,
    selected: bool,
    radius: Radius,
) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let mut style = flat_container(
            if selected { t.sel } else { t.bg },
            if selected { t.fg } else { t.ghost },
            t.line_weak,
        );
        style.border.width = 0.0;
        return style;
    }

    let palette = theme.extended_palette();

    let mut style = container::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: Some(table_result_text_color(theme, palette.background.weak.text)),
        border: Border {
            width: 1.0,
            radius,
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    };

    if selected {
        style.background = Some(table_selection_overlay_color(theme, 0.20).into());
    }

    style
}

pub(crate) fn table_cell_style(
    theme: &Theme,
    focused: bool,
    row_selected: bool,
    radius: Radius,
) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let background = if focused {
            t.sel
        } else if row_selected {
            t.row
        } else {
            t.bg
        };
        let mut style = flat_container(
            background,
            if focused { t.fg } else { t.body },
            if focused {
                t.accent
            } else {
                Color::TRANSPARENT
            },
        );
        style.border.width = if focused { 2.0 } else { 0.0 };
        return style;
    }

    let palette = theme.extended_palette();

    let mut style = container::Style {
        background: Some(palette.background.weakest.color.into()),
        text_color: Some(table_result_text_color(
            theme,
            palette.background.weakest.text,
        )),
        border: Border {
            width: 1.0,
            radius,
            color: palette.background.strong.color,
        },
        ..container::Style::default()
    };

    if row_selected {
        style.background = Some(table_selection_overlay_color(theme, 0.20).into());
    }

    if focused {
        style.border.width = 1.5;
        style.border.color = Color::WHITE;
    }

    style
}

pub(crate) fn table_input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border {
        width: 0.0,
        radius: ui_radius().into(),
        color: Color::TRANSPARENT,
    };

    if is_modern() {
        let t = tokens(theme);
        style.value = t.fg;
        style.placeholder = t.ghost;
        style.selection = t.sel;
    }

    style
}

pub(crate) fn borderless_query_editor_style(
    theme: &Theme,
    _status: text_editor::Status,
) -> text_editor::Style {
    if is_modern() {
        let t = tokens(theme);
        return text_editor::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                width: 0.0,
                radius: 0.0.into(),
                color: Color::TRANSPARENT,
            },
            placeholder: t.ghost,
            value: t.body,
            selection: t.sel,
        };
    }

    let palette = theme.extended_palette();

    text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            width: 0.0,
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        placeholder: palette.secondary.base.color,
        value: palette.background.base.text,
        selection: palette.primary.weak.color,
    }
}

pub(crate) fn alert_style(theme: &Theme) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        return flat_container(t.chrome, t.danger, t.danger);
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.danger.weak.color.into()),
        text_color: Some(palette.danger.weak.text),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: palette.danger.strong.color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn update_banner_style(theme: &Theme) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        return flat_container(t.sel, t.fg, t.accent);
    }

    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.primary.weak.color.into()),
        text_color: Some(palette.primary.weak.text),
        border: Border {
            width: 1.0,
            radius: 0.0.into(),
            color: palette.primary.strong.color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn toast_button_style(
    level: ToastLevel,
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    if is_modern() {
        let t = tokens(theme);
        let accent = toast_level_accent_color(level);
        let base = flat_button(t.chrome2, accent, accent);

        return match status {
            button::Status::Active => base,
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(t.hover)),
                ..base
            },
            button::Status::Disabled => disabled_button_style(base),
        };
    }

    let dark = is_dark_theme(theme);
    let (base_background, hover_background, pressed_background) = (
        Color::from_rgba(0.0, 0.0, 0.0, 0.98),
        Color::from_rgba(0.0, 0.0, 0.0, 0.99),
        Color::from_rgba(0.0, 0.0, 0.0, 1.0),
    );

    let text_color = toast_level_accent_color(level);

    let mut border_color = toast_level_accent_color(level);
    border_color.a = if dark { 0.74 } else { 0.64 };

    let mut base = button_base_style(base_background, text_color, border_color);
    base.shadow = Shadow {
        color: if dark {
            Color::from_rgba(0.0, 0.0, 0.0, 0.52)
        } else {
            Color::from_rgba(0.0, 0.0, 0.0, 0.34)
        },
        offset: Vector::new(0.0, 8.0),
        blur_radius: 28.0,
    };

    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(hover_background)),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(Background::Color(pressed_background)),
            ..base
        },
        button::Status::Disabled => disabled_button_style(base),
    }
}

pub(crate) fn changelog_kind_color(kind: ChangeKind, theme: &Theme) -> Color {
    if is_dark_theme(theme) {
        match kind {
            ChangeKind::Added => Color::from_rgb8(0x6F, 0xD0, 0x8C),
            ChangeKind::Changed => Color::from_rgb8(0x63, 0xB3, 0xED),
            ChangeKind::Fixed => Color::from_rgb8(0xE9, 0xB4, 0x4C),
            ChangeKind::Removed => Color::from_rgb8(0xF0, 0x8A, 0x8A),
            ChangeKind::Other => theme.extended_palette().background.base.text,
        }
    } else {
        match kind {
            ChangeKind::Added => Color::from_rgb8(0x14, 0x66, 0x3A),
            ChangeKind::Changed => Color::from_rgb8(0x14, 0x50, 0x8F),
            ChangeKind::Fixed => Color::from_rgb8(0x7A, 0x4E, 0x00),
            ChangeKind::Removed => Color::from_rgb8(0x8E, 0x2B, 0x2B),
            ChangeKind::Other => theme.extended_palette().background.base.text,
        }
    }
}

pub(crate) fn toast_level_accent_color(level: ToastLevel) -> Color {
    match level {
        ToastLevel::Info => Color::from_rgb8(0x3D, 0x9E, 0xFF),
        ToastLevel::Success => Color::from_rgb8(0x22, 0xC5, 0x5E),
        ToastLevel::Error => Color::from_rgb8(0xEF, 0x44, 0x44),
    }
}

pub(crate) fn modern_scrollable_style(
    theme: &Theme,
    status: scrollable::Status,
) -> scrollable::Style {
    let t = tokens(theme);
    let hovered = match status {
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
            ..
        } => is_vertical_scrollbar_hovered || is_horizontal_scrollbar_hovered,
        scrollable::Status::Dragged { .. } => true,
        _ => false,
    };

    let rail = scrollable::Rail {
        background: None,
        border: Border {
            width: 0.0,
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        scroller: scrollable::Scroller {
            background: Background::Color(if hovered { t.ghost } else { t.line }),
            border: Border {
                width: 0.0,
                radius: 0.0.into(),
                color: Color::TRANSPARENT,
            },
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::default(theme, status).auto_scroll,
    }
}

pub(crate) fn resize_handle_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    let mut color = if is_modern() {
        tokens(theme).line
    } else {
        palette.background.strong.color
    };
    if !is_modern() {
        color.a = 0.5;
    }

    container::Style {
        background: Some(color.into()),
        border: Border {
            width: 0.0,
            radius: ui_radius().into(),
            color: Color::TRANSPARENT,
        },
        ..container::Style::default()
    }
}

pub(crate) fn tab_chip_style(theme: &Theme, active: bool) -> container::Style {
    if is_modern() {
        let t = tokens(theme);
        let mut style = flat_container(
            if active { t.bg } else { t.chrome },
            if active { t.fg } else { t.muted },
            t.line,
        );
        style.border.width = 0.0;
        return style;
    }

    let palette = theme.extended_palette();
    let background = if active {
        selection_overlay_color(theme, 0.10)
    } else {
        palette.background.weakest.color
    };
    let border_color = if active {
        selection_overlay_color(theme, 0.32)
    } else {
        palette.background.strong.color
    };
    let text_color = if active {
        highlighted_text_color(theme)
    } else {
        palette.background.base.text
    };

    container::Style {
        background: Some(background.into()),
        text_color: Some(text_color),
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: border_color,
        },
        ..container::Style::default()
    }
}

pub(crate) fn tab_chip_close_button_style(
    theme: &Theme,
    active: bool,
    status: button::Status,
) -> button::Style {
    let palette = theme.extended_palette();
    let text_color = if is_modern() {
        let t = tokens(theme);
        if active { t.fg } else { t.muted }
    } else if active {
        highlighted_text_color(theme)
    } else {
        palette.background.base.text
    };
    let mut style = button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color,
        border: Border {
            width: 0.0,
            radius: 0.0.into(),
            color: Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
        snap: false,
    };

    match status {
        button::Status::Hovered => style.text_color = text_color.scale_alpha(0.8),
        button::Status::Pressed => style.text_color = text_color.scale_alpha(0.65),
        button::Status::Disabled => style.text_color = text_color.scale_alpha(0.5),
        button::Status::Active => {}
    }

    style
}

pub(crate) fn sidebar_table_button_style(
    selected: bool,
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    if is_modern() {
        let t = tokens(theme);
        let base = flat_button(
            if selected { t.sel } else { Color::TRANSPARENT },
            if selected { t.fg } else { t.body },
            Color::TRANSPARENT,
        );

        return match status {
            button::Status::Active => base,
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(Background::Color(if selected { t.sel } else { t.hover })),
                ..base
            },
            button::Status::Disabled => disabled_button_style(base),
        };
    }

    let palette = theme.extended_palette();

    let base_background = if selected {
        selection_overlay_color(theme, 0.10)
    } else {
        palette.background.weakest.color
    };

    let hover_background = if selected {
        selection_overlay_color(theme, 0.14)
    } else {
        palette.background.weaker.color
    };

    let active_background = if selected {
        selection_overlay_color(theme, 0.18)
    } else {
        palette.background.weak.color
    };

    let mut style = button::Style {
        background: Some(Background::Color(base_background)),
        text_color: if selected {
            highlighted_text_color(theme)
        } else {
            palette.background.base.text
        },
        border: Border {
            width: 1.0,
            radius: ui_radius().into(),
            color: if selected {
                selection_overlay_color(theme, 0.32)
            } else {
                palette.background.strong.color
            },
        },
        shadow: Shadow::default(),
        snap: false,
    };

    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(hover_background));
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(active_background));
        }
        button::Status::Disabled => {
            style.background = Some(Background::Color(palette.background.weakest.color));
            style.text_color = palette.background.weak.text;
        }
        button::Status::Active => {}
    }

    style
}

pub(crate) fn sidebar_folder_button_style(
    drop_target: bool,
    pinned: bool,
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    let palette = theme.extended_palette();
    let mut style = compact_button_style(theme, status);

    if pinned {
        style.text_color = if is_modern() {
            tokens(theme).accent
        } else {
            highlighted_text_color(theme)
        };
    }

    if drop_target {
        if is_modern() {
            style.background = Some(Background::Color(tokens(theme).sel));
            return style;
        }
        style.background = Some(Background::Color(selection_overlay_color(theme, 0.14)));
        style.border.color = selection_overlay_color(theme, 0.32);
        if matches!(status, button::Status::Pressed) {
            style.background = Some(Background::Color(selection_overlay_color(theme, 0.18)));
        }
    } else if matches!(status, button::Status::Disabled) {
        style.background = Some(Background::Color(palette.background.weakest.color));
    }

    style
}
