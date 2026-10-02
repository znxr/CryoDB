use crate::constants::ICON_CLOSE_LINE;
use crate::utils::text::max_chars_for_width;
use iced::{Point, Size};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutMode {
    Wide,
    Compact,
}

use crate::constants::{FONT_SCALE_BASELINE, REMIX_ICON_FONT_FAMILY};
use crate::model::settings::{FontChoice, Settings, UiDensity};
use crate::ui::theme::ThemeVariant;
use iced::widget::container;
use iced::widget::{pick_list, text};
use iced::{Element, Padding};
use iced::{Font, Theme};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static FONT_NAME_CACHE: LazyLock<Mutex<HashMap<String, &'static str>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn font_for_choice(choice: &FontChoice) -> Font {
    match choice {
        FontChoice::Sans => Font::DEFAULT,
        FontChoice::Monospace => Font::MONOSPACE,
        FontChoice::System(name) => {
            let name = if let Ok(mut cache) = FONT_NAME_CACHE.lock() {
                if let Some(value) = cache.get(name) {
                    *value
                } else {
                    let value: &'static str = name.to_string().leak();
                    cache.insert(name.to_string(), value);
                    value
                }
            } else {
                name.to_string().leak()
            };
            Font::with_name(name)
        }
    }
}

pub(crate) struct Presentation<'a> {
    pub(crate) settings: &'a Settings,
}

impl Presentation<'_> {
    pub(crate) fn context_menu_height(&self, items: usize) -> f32 {
        let count = items.max(1) as f32;
        let button_height = self.button_text_size() as f32 + self.button_padding_tight()[0] * 2.0;
        let spacing = self.scale_f32(2.0) * (count - 1.0);
        let panel_padding = self.scale_u16(4) * 2.0;
        (button_height * count) + spacing + panel_padding
    }
    pub(crate) fn context_menu_padding(
        &self,
        position: Point,
        bounds: Size,
        menu_width: f32,
        menu_height: f32,
        offset_x: f32,
        offset_y: f32,
    ) -> Padding {
        let max_left = (bounds.width - menu_width).max(0.0);
        let max_top = (bounds.height - menu_height).max(0.0);
        let left = (position.x + offset_x).clamp(0.0, max_left);
        let down_top = position.y + offset_y;
        let up_top = position.y - offset_y - menu_height;
        let top = if down_top <= max_top {
            down_top
        } else if up_top >= 0.0 {
            up_top
        } else {
            max_top
        }
        .clamp(0.0, max_top);

        Padding {
            top,
            left,
            right: 0.0,
            bottom: 0.0,
        }
    }
    pub(crate) fn button_padding_icon(&self) -> [f32; 2] {
        if self.modern() {
            return self.scale_padding([4, 12]);
        }
        self.scale_padding([4, 6])
    }
    pub(crate) fn ui_font(&self) -> Font {
        font_for_choice(&self.settings.font)
    }

    pub(crate) fn icon_font(&self) -> Font {
        Font::with_name(REMIX_ICON_FONT_FAMILY)
    }

    pub(crate) fn font_scale(&self) -> f32 {
        self.settings.font_size as f32 / FONT_SCALE_BASELINE as f32
    }

    pub(crate) fn density_scale(&self) -> f32 {
        match self.settings.ui_density {
            UiDensity::Normal => 1.1,
            UiDensity::Compact => 0.85,
        }
    }

    pub(crate) fn ui_scale(&self) -> f32 {
        self.font_scale() * self.density_scale()
    }

    pub(crate) fn modern(&self) -> bool {
        self.settings.theme_variant == ThemeVariant::Modern
    }

    pub(crate) fn scale_u16(&self, value: u16) -> f32 {
        (value as f32) * self.ui_scale()
    }

    pub(crate) fn scale_f32(&self, value: f32) -> f32 {
        value * self.ui_scale()
    }

    pub(crate) fn button_text_size(&self) -> u32 {
        self.settings.font_size
    }

    pub(crate) fn icon_text_size(&self) -> u32 {
        match self.settings.ui_density {
            UiDensity::Normal => self.button_text_size().saturating_add(2),
            UiDensity::Compact => self.button_text_size(),
        }
    }

    pub(crate) fn input_text_size(&self) -> u32 {
        self.button_text_size()
    }

    pub(crate) fn label_text_size(&self) -> u32 {
        self.button_text_size().saturating_sub(1)
    }

    pub(crate) fn title_text_size(&self) -> u32 {
        self.button_text_size().saturating_add(2)
    }

    pub(crate) fn heading_text_size(&self) -> u32 {
        self.button_text_size().saturating_add(6)
    }

    pub(crate) fn scale_padding(&self, padding: [u16; 2]) -> [f32; 2] {
        [self.scale_u16(padding[0]), self.scale_u16(padding[1])]
    }

    pub(crate) fn button_padding(&self) -> [f32; 2] {
        if self.modern() {
            return self.scale_padding([7, 14]);
        }
        self.scale_padding([5, 8])
    }

    pub(crate) fn button_padding_tight(&self) -> [f32; 2] {
        self.scale_padding([4, 6])
    }

    pub(crate) fn input_padding(&self) -> [f32; 2] {
        if self.modern() {
            return self.scale_padding([7, 12]);
        }
        self.scale_padding([5, 8])
    }

    pub(crate) fn pick_list_handle(&self) -> pick_list::Handle<Font> {
        pick_list::Handle::default()
    }

    pub(crate) fn label_text<'a>(&self, content: impl Into<String>) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.label_text_size())
    }

    pub(crate) fn muted_label_text<'a>(
        &self,
        content: impl Into<String>,
    ) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.label_text_size())
            .style(|theme: &Theme| {
                let mut color = theme.extended_palette().background.weakest.text;
                color.a *= 0.68;
                iced::widget::text::Style { color: Some(color) }
            })
    }

    pub(crate) fn button_text<'a>(&self, content: impl Into<String>) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.button_text_size())
    }

    pub(crate) fn icon_text<'a>(&self, icon: char) -> iced::widget::Text<'a> {
        text(icon.to_string())
            .font(self.icon_font())
            .size(self.icon_text_size())
    }

    pub(crate) fn title_text<'a>(&self, content: impl Into<String>) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.title_text_size())
    }

    pub(crate) fn heading_text<'a>(&self, content: impl Into<String>) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.heading_text_size())
    }
    pub(crate) fn action_label<'a, M: 'a>(
        &self,
        label: &str,
        icon: char,
        layout: LayoutMode,
    ) -> Element<'a, M> {
        let icon_top_offset = if icon == ICON_CLOSE_LINE {
            self.scale_f32(2.0)
        } else {
            0.0
        };

        if matches!(layout, LayoutMode::Compact) {
            container(self.icon_text(icon))
                .padding(Padding {
                    top: icon_top_offset,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                })
                .align_y(iced::alignment::Vertical::Center)
                .into()
        } else {
            let icon = container(
                text(icon.to_string())
                    .font(self.icon_font())
                    .size(self.button_text_size()),
            )
            .padding(Padding {
                top: icon_top_offset,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            })
            .align_y(iced::alignment::Vertical::Center);
            let label =
                container(self.button_text(label)).align_y(iced::alignment::Vertical::Center);
            iced::widget::row![icon, label]
                .spacing(self.scale_u16(4))
                .align_y(iced::alignment::Vertical::Center)
                .into()
        }
    }
    pub(crate) fn pick_list_max_chars(&self, width: f32) -> usize {
        let border = self.scale_f32(2.0);
        let icon_space = self.scale_f32(24.0);
        let horizontal_padding = self.input_padding()[1] * 2.0;
        let available = (width - border - icon_space - horizontal_padding).max(20.0);
        max_chars_for_width(available / self.font_scale())
    }
    pub(crate) fn input_text<'a>(&self, content: impl Into<String>) -> iced::widget::Text<'a> {
        text(crate::i18n::tr(&content.into()))
            .font(self.ui_font())
            .size(self.input_text_size())
    }
}
