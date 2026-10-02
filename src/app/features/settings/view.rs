use super::types::{FontPickerTarget, SettingsModalTab, ShortcutCaptureTarget};
use super::{Message, State};
use crate::constants::{
    AI_CLI_PRESETS, AI_PROVIDERS, DENSITY_CHOICES, FONT_PICKER_MAX_VISIBLE, ICON_CLOSE_LINE,
    ICON_FOLDER_OPEN, ICON_KEY_COMBO_PLUS, MODAL_SCROLLBAR_GUTTER, RESULT_GRID_DENSITY_CHOICES,
    SIDEBAR_SCROLLBAR_GUTTER, SIDEBAR_SCROLLBAR_WIDTH,
};
use crate::i18n::Language;
use crate::model::settings::{
    AppearanceColor, FontChoice, PickerOption, ShortcutBinding, SystemThemeMode,
};
use crate::ui::ids::text_field_id;
use crate::ui::presentation::{LayoutMode, Presentation};
use crate::ui::styles::{
    alert_style, compact_button_style, compact_input_style, compact_pick_list_menu_style,
    compact_pick_list_style, compact_primary_button_style, disconnect_button_style,
    modal_backdrop_style, panel_border_style, panel_style, settings_modal_tab_button_style,
    tab_chip_style,
};
use crate::ui::theme::{ThemeChoice, ThemeVariant};
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use crate::ui::widgets::history_input::history_input;
use crate::utils::text::truncate_with_ellipsis;
use iced::widget::{
    button, checkbox, container, mouse_area, pick_list, row, scrollable, slider, space, stack, text,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Size, Theme, mouse};

pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) layout_mode: LayoutMode,
    pub(crate) window_size: Size,
    pub(crate) query_inline_suggestion_error: Option<&'a str>,
    pub(crate) diagrams: Vec<(String, String, String)>,
    pub(crate) update_status: String,
    pub(crate) update_busy: bool,
}

impl<'a> View<'a> {
    pub(crate) fn settings_modal(&self) -> Element<'a, Message> {
        let layout = self.layout_mode;
        let backdrop_dim = self.state.values.modal_backdrop_dim;
        let available_width = if self.window_size.width > 0.0 {
            self.window_size.width
        } else {
            self.presentation.scale_f32(1200.0)
        };
        let available_height = if self.window_size.height > 0.0 {
            self.window_size.height
        } else {
            self.presentation.scale_f32(800.0)
        };
        let modal_margin = self.presentation.scale_f32(32.0);
        let max_modal_width =
            (available_width - modal_margin).max(self.presentation.scale_f32(320.0));
        let max_modal_height =
            (available_height - modal_margin).max(self.presentation.scale_f32(360.0));
        let modal_width = (available_width * 0.88).clamp(
            self.presentation.scale_f32(640.0).min(max_modal_width),
            max_modal_width,
        );
        let modal_height = (available_height * 0.86).clamp(
            self.presentation.scale_f32(520.0).min(max_modal_height),
            max_modal_height,
        );
        let modal_padding = self.presentation.scale_u16(14);
        let modal_inner_width = (modal_width - (modal_padding * 2.0)).max(0.0);
        let nav_width = match layout {
            LayoutMode::Wide => self.presentation.scale_f32(218.0),
            LayoutMode::Compact => self.presentation.scale_f32(172.0),
        }
        .min((modal_inner_width * 0.38).max(self.presentation.scale_f32(132.0)));
        let content_inner_width =
            (modal_inner_width - nav_width - self.presentation.scale_f32(14.0)).max(0.0);

        let header = row![
            self.presentation.heading_text("Settings"),
            space::horizontal(),
            button(
                self.presentation
                    .action_label("Close", ICON_CLOSE_LINE, layout)
            )
            .padding(self.presentation.button_padding())
            .style(compact_button_style)
            .on_press(Message::CloseSettings),
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        let font_picker_width = self.presentation.scale_f32(170.0);
        let font_max_chars = self.presentation.pick_list_max_chars(font_picker_width);
        let font_dropdown = |target: FontPickerTarget, active_font: &FontChoice| {
            let font_label = active_font.to_string();
            let (font_display, font_truncated) =
                truncate_with_ellipsis(&font_label, font_max_chars);
            let is_open = self.state.font_picker_open && self.state.font_picker_target == target;

            let font_picker_button = {
                let label = container(
                    self.presentation
                        .button_text(font_display)
                        .wrapping(text::Wrapping::None)
                        .width(Fill),
                )
                .width(Fill)
                .clip(true);

                button(label)
                    .padding(self.presentation.input_padding())
                    .style(compact_button_style)
                    .width(Length::Fixed(font_picker_width))
                    .on_press(Message::ToggleFontPicker(target))
            };

            let font_picker_button: Element<'_, Message> = if font_truncated {
                self.with_tooltip(font_picker_button, font_label)
            } else {
                font_picker_button.into()
            };

            let font_panel: Element<'_, Message> = if is_open {
                let font_filter = self.state.font_search.trim().to_ascii_lowercase();
                let font_options: Vec<PickerOption<FontChoice>> = self
                    .state
                    .font_choices
                    .iter()
                    .cloned()
                    .filter_map(|font| {
                        let label = font.to_string();
                        if !font_filter.is_empty()
                            && !label.to_ascii_lowercase().contains(&font_filter)
                        {
                            return None;
                        }
                        let (display, _) = truncate_with_ellipsis(&label, font_max_chars);
                        Some(PickerOption::new(font, display))
                    })
                    .take(FONT_PICKER_MAX_VISIBLE)
                    .collect();

                let search_input = history_input(
                    text_field_id("font-search"),
                    "Search fonts",
                    &self.state.font_search,
                    Message::FontSearchChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .width(Fill);

                let mut list = iced::widget::column![].spacing(self.presentation.scale_u16(3));
                if font_options.is_empty() {
                    list = list.push(self.presentation.label_text("No matching fonts."));
                } else {
                    for option in &font_options {
                        let is_selected = option.value == *active_font;
                        let item_label = container(
                            self.presentation
                                .button_text(option.label.clone())
                                .wrapping(text::Wrapping::None)
                                .width(Fill),
                        )
                        .width(Fill)
                        .clip(true);

                        let entry = button(item_label)
                            .padding(self.presentation.button_padding_tight())
                            .width(Fill)
                            .style(if is_selected {
                                compact_primary_button_style
                            } else {
                                compact_button_style
                            })
                            .on_press(Message::FontSelected(target, option.value.clone()));

                        list = list.push(entry);
                    }
                    if font_options.len() == FONT_PICKER_MAX_VISIBLE {
                        list = list.push(
                            self.presentation
                                .label_text("Refine search to see more fonts."),
                        );
                    }
                }

                let list = scrollable(list)
                    .height(Length::Fixed(self.presentation.scale_f32(240.0)))
                    .width(Fill)
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::new()
                            .width(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                            .scroller_width(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                            .spacing(self.presentation.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
                    ));

                container(
                    iced::widget::column![search_input, list]
                        .spacing(self.presentation.scale_u16(4)),
                )
                .padding(self.presentation.scale_u16(6))
                .width(Length::Fixed(font_picker_width))
                .style(panel_border_style)
                .into()
            } else {
                space::horizontal().into()
            };

            HeaderDropdown::new(
                font_picker_button,
                font_panel,
                is_open,
                Message::ToggleFontPicker(target),
            )
        };

        let ui_font_dropdown = font_dropdown(FontPickerTarget::Ui, &self.state.values.font);
        let editor_font_dropdown =
            font_dropdown(FontPickerTarget::Editor, &self.state.values.editor_font);

        let font_size_picker = history_input(
            text_field_id("font-size"),
            "14",
            &self.state.values.font_size_input,
            Message::FontSizeChanged,
            Message::TextEdited,
        )
        .on_submit(Message::FontSizeApplied)
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style);

        let font_size_apply = button(self.presentation.button_text("Apply"))
            .padding(self.presentation.button_padding_tight())
            .style(compact_button_style)
            .on_press(Message::FontSizeApplied);

        let density_picker = pick_list(
            DENSITY_CHOICES,
            Some(self.state.values.ui_density),
            Message::UiDensitySelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let accent_picker = pick_list(
            AppearanceColor::ALL,
            Some(self.state.values.accent_color),
            Message::AccentColorSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let result_grid_density_picker = pick_list(
            RESULT_GRID_DENSITY_CHOICES,
            Some(self.state.values.result_grid_density),
            Message::ResultGridDensitySelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let color_swatch = |color: Color, size: f32| -> Element<'_, Message> {
            container(space::horizontal())
                .width(Length::Fixed(size))
                .height(Length::Fixed(size))
                .style(move |theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
                        background: Some(Background::Color(color)),
                        border: Border {
                            width: 1.0,
                            radius: (size * 0.25).into(),
                            color: palette.background.strong.color,
                        },
                        ..container::Style::default()
                    }
                })
                .into()
        };

        let theme_picker_width = self.presentation.scale_f32(360.0);
        let theme_max_chars = self.presentation.pick_list_max_chars(theme_picker_width);
        let active_theme = self.state.theme_choice.ui_theme();
        let active_palette = active_theme.palette();
        let theme_label = self.state.theme_choice.to_string();
        let (theme_display, _) = truncate_with_ellipsis(&theme_label, theme_max_chars);

        let theme_picker_button = button(
            container(
                row![
                    self.presentation
                        .button_text(theme_display)
                        .wrapping(text::Wrapping::None)
                        .width(Fill),
                    color_swatch(active_palette.primary, self.presentation.scale_f32(13.0)),
                    color_swatch(active_palette.background, self.presentation.scale_f32(13.0)),
                    color_swatch(active_palette.text, self.presentation.scale_f32(13.0)),
                ]
                .spacing(self.presentation.scale_u16(4))
                .align_y(Center),
            )
            .width(Fill)
            .clip(true),
        )
        .padding(self.presentation.input_padding())
        .style(compact_button_style)
        .width(Length::Fixed(theme_picker_width))
        .on_press(Message::ToggleSettingsThemePicker);

        let theme_panel: Element<'_, Message> = if self.state.settings_theme_picker_open {
            let search_input = history_input(
                text_field_id("settings-theme-search"),
                "Search themes",
                &self.state.settings_theme_search,
                Message::SettingsThemeSearchChanged,
                Message::TextEdited,
            )
            .padding(self.presentation.input_padding())
            .size(self.presentation.input_text_size())
            .font(self.presentation.ui_font())
            .style(compact_input_style)
            .width(Fill);

            let theme_filter = self.state.settings_theme_search.trim().to_ascii_lowercase();
            let mut list = iced::widget::column![].spacing(self.presentation.scale_u16(3));
            let mut option_count = 0usize;
            for option in ThemeChoice::ALL.iter().copied() {
                let label = option.to_string();
                if !theme_filter.is_empty() && !label.to_ascii_lowercase().contains(&theme_filter) {
                    continue;
                }

                let preview_theme = option.ui_theme();
                let palette = preview_theme.palette();
                let is_selected = option == self.state.theme_choice;
                let title = if is_selected {
                    format!("{label} ({})", crate::i18n::tr("active"))
                } else {
                    label
                };
                let (display, _) = truncate_with_ellipsis(&title, theme_max_chars);
                let entry = button(
                    container(
                        row![
                            self.presentation
                                .button_text(display)
                                .wrapping(text::Wrapping::None)
                                .width(Fill),
                            color_swatch(palette.primary, self.presentation.scale_f32(13.0)),
                            color_swatch(palette.background, self.presentation.scale_f32(13.0)),
                            color_swatch(palette.text, self.presentation.scale_f32(13.0)),
                        ]
                        .spacing(self.presentation.scale_u16(4))
                        .align_y(Center),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.presentation.button_padding_tight())
                .width(Fill)
                .style(if is_selected {
                    compact_primary_button_style
                } else {
                    compact_button_style
                })
                .on_press(Message::ThemeSelected(option));

                option_count += 1;
                list = list.push(entry);
            }

            if option_count == 0 {
                list = list.push(self.presentation.label_text("No matching themes."));
            }

            let list = scrollable(list)
                .height(Length::Fixed(self.presentation.scale_f32(260.0)))
                .width(Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new()
                        .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                ));

            container(
                iced::widget::column![search_input, list].spacing(self.presentation.scale_u16(4)),
            )
            .padding(self.presentation.scale_u16(6))
            .width(Length::Fixed(theme_picker_width))
            .style(panel_border_style)
            .into()
        } else {
            space::horizontal().into()
        };

        let theme_dropdown = HeaderDropdown::new(
            theme_picker_button.into(),
            theme_panel,
            self.state.settings_theme_picker_open,
            Message::ToggleSettingsThemePicker,
        );

        let backdrop_dim_slider = slider(
            0.20..=1.0,
            self.state.values.modal_backdrop_dim,
            Message::ModalBackdropDimChanged,
        )
        .step(0.01)
        .width(Fill);

        let large_sidebar_buttons_toggle = row![
            checkbox(self.state.values.large_sidebar_buttons)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::LargeSidebarButtonsToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation.muted_label_text("Larger sidebar buttons"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let compact_sidebar_toggle = row![
            checkbox(self.state.values.compact_sidebar)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::CompactSidebarToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Compact sidebar (denser table lists)"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let auto_expand_table_toggle = row![
            checkbox(self.state.values.auto_expand_selected_table)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::AutoExpandSelectedTableToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Auto-expand selected table folder"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let show_hidden_tables_toggle = row![
            checkbox(self.state.values.show_hidden_tables)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::ShowHiddenTablesToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Show hidden/system tables"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let multiple_connections_layout_toggle = row![
            checkbox(self.state.values.multiple_connections_layout)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::MultipleConnectionsLayoutToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Enable multiple connections layout"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let header_emphasis_toggle = row![
            checkbox(self.state.values.emphasize_column_headers)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::EmphasizeColumnHeadersToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Bold column header labels"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let auto_scroll_table_toggle = row![
            checkbox(self.state.values.auto_scroll_sidebar_to_selected_table)
                .label(crate::i18n::tr(""))
                .on_toggle(Message::AutoScrollSidebarToSelectedTableToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation
                .muted_label_text("Auto-scroll sidebar to selected table"),
        ]
        .spacing(self.presentation.scale_u16(5))
        .align_y(Center);

        let accent_color = crate::ui::theme::appearance_color(
            self.state.values.accent_color,
            self.state.theme().palette().primary,
        );
        let backdrop_percent = (self.state.values.modal_backdrop_dim * 100.0).round() as u32;

        let card_padding = self.presentation.scale_u16(8);
        let card_spacing = self.presentation.scale_u16(8);
        let field_spacing = self.presentation.scale_u16(4);
        macro_rules! settings_card {
            ($title:expr, $body:expr $(,)?) => {
                container(
                    iced::widget::column![self.presentation.title_text($title), $body]
                        .spacing(self.presentation.scale_u16(6))
                        .width(Fill),
                )
                .padding(card_padding)
                .width(Fill)
                .style(panel_border_style)
                .into()
            };
        }

        let accent_field = iced::widget::column![
            self.presentation.muted_label_text("Accent"),
            row![
                color_swatch(accent_color, self.presentation.scale_f32(16.0)),
                accent_picker.width(Length::Fixed(self.presentation.scale_f32(150.0))),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
        ]
        .spacing(field_spacing);

        let backdrop_field = iced::widget::column![
            self.presentation.muted_label_text("Backdrop Dim"),
            row![
                backdrop_dim_slider,
                container(
                    self.presentation
                        .muted_label_text(format!("{backdrop_percent}%"))
                )
                .width(Length::Fixed(self.presentation.scale_f32(44.0)))
                .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(self.presentation.scale_u16(8))
            .align_y(Center),
        ]
        .spacing(field_spacing)
        .width(Fill);

        let system_theme_mode_picker = pick_list(
            SystemThemeMode::ALL,
            Some(self.state.values.system_theme_mode),
            Message::SystemThemeModeSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let theme_variant_picker = pick_list(
            ThemeVariant::ALL,
            Some(self.state.values.theme_variant),
            Message::ThemeVariantSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let dark_theme_picker = pick_list(
            ThemeChoice::ALL,
            Some(self.state.values.dark_theme),
            Message::DarkThemeSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let light_theme_picker = pick_list(
            ThemeChoice::ALL,
            Some(self.state.values.light_theme),
            Message::LightThemeSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let language_card: Element<'_, Message> = settings_card!(
            "Language",
            iced::widget::column![
                self.presentation.muted_label_text("Interface Language"),
                pick_list(
                    Language::ALL,
                    Some(self.state.values.language),
                    Message::LanguageSelected,
                )
                .padding(self.presentation.input_padding())
                .text_size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .handle(self.presentation.pick_list_handle())
                .style(compact_pick_list_style)
                .menu_style(compact_pick_list_menu_style)
                .width(Length::Fixed(self.presentation.scale_f32(200.0))),
            ]
            .spacing(field_spacing)
            .width(Fill),
        );

        let is_system_mode = self.state.values.system_theme_mode == SystemThemeMode::System;

        let theme_card: Element<'_, Message> = if is_system_mode {
            settings_card!(
                "Theme & Color",
                iced::widget::column![
                    iced::widget::column![
                        self.presentation.muted_label_text("Theme Mode"),
                        system_theme_mode_picker
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Theme"),
                        theme_variant_picker
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Dark Color Palette"),
                        dark_theme_picker.width(Length::Fixed(self.presentation.scale_f32(200.0))),
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Light Color Palette"),
                        light_theme_picker.width(Length::Fixed(self.presentation.scale_f32(200.0))),
                    ]
                    .spacing(field_spacing),
                    row![accent_field, backdrop_field]
                        .spacing(self.presentation.scale_u16(10))
                        .align_y(Center),
                ]
                .spacing(card_spacing)
                .width(Fill),
            )
        } else {
            settings_card!(
                "Theme & Color",
                iced::widget::column![
                    iced::widget::column![
                        self.presentation.muted_label_text("Theme Mode"),
                        system_theme_mode_picker
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Theme"),
                        theme_variant_picker
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Color Palette"),
                        theme_dropdown
                    ]
                    .spacing(field_spacing),
                    row![accent_field, backdrop_field]
                        .spacing(self.presentation.scale_u16(10))
                        .align_y(Center),
                ]
                .spacing(card_spacing)
                .width(Fill),
            )
        };

        let typography_card: Element<'_, Message> = settings_card!(
            "Typography",
            iced::widget::column![
                row![
                    iced::widget::column![
                        self.presentation.muted_label_text("UI Font"),
                        ui_font_dropdown
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Editor Font"),
                        editor_font_dropdown
                    ]
                    .spacing(field_spacing),
                ]
                .spacing(self.presentation.scale_u16(10))
                .align_y(Center),
                iced::widget::column![
                    self.presentation.muted_label_text("Font Size"),
                    row![font_size_picker, font_size_apply,]
                        .spacing(self.presentation.scale_u16(6))
                        .align_y(Center),
                ]
                .spacing(field_spacing),
            ]
            .spacing(card_spacing)
            .width(Fill),
        );

        let density_card: Element<'_, Message> = settings_card!(
            "Density & Layout",
            iced::widget::column![
                row![
                    iced::widget::column![
                        self.presentation.muted_label_text("UI Density"),
                        density_picker.width(Length::Fixed(self.presentation.scale_f32(130.0))),
                    ]
                    .spacing(field_spacing),
                    iced::widget::column![
                        self.presentation.muted_label_text("Result Grid"),
                        result_grid_density_picker
                            .width(Length::Fixed(self.presentation.scale_f32(150.0))),
                    ]
                    .spacing(field_spacing),
                ]
                .spacing(self.presentation.scale_u16(10))
                .align_y(Center),
                iced::widget::column![
                    large_sidebar_buttons_toggle,
                    compact_sidebar_toggle,
                    header_emphasis_toggle,
                    auto_scroll_table_toggle,
                    auto_expand_table_toggle,
                    show_hidden_tables_toggle,
                    multiple_connections_layout_toggle,
                ]
                .spacing(self.presentation.scale_u16(5)),
            ]
            .spacing(card_spacing)
            .width(Fill),
        );

        let two_column_appearance = matches!(layout, LayoutMode::Wide)
            && content_inner_width >= self.presentation.scale_f32(760.0);
        let appearance: Element<'_, Message> = if two_column_appearance {
            let left = iced::widget::column![language_card, theme_card, typography_card]
                .spacing(card_spacing)
                .width(Fill);
            let right = iced::widget::column![density_card].width(Fill);
            row![left, right]
                .spacing(card_spacing)
                .align_y(iced::Alignment::Start)
                .width(Fill)
                .into()
        } else {
            iced::widget::column![language_card, theme_card, typography_card, density_card]
                .spacing(card_spacing)
                .width(Fill)
                .into()
        };

        let table_limit_input = history_input(
            text_field_id("table-query-limit"),
            "Rows per table page",
            &self.state.values.table_query_limit_input,
            Message::TableQueryLimitChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(160.0)));

        let history_limit_input = history_input(
            text_field_id("history-limit"),
            "History entries",
            &self.state.values.history_limit_input,
            Message::HistoryLimitChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(160.0)));

        let limits = iced::widget::column![
            row![
                self.presentation.label_text("Table query limit"),
                table_limit_input
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            row![
                self.presentation.label_text("History limit"),
                history_limit_input
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
        ]
        .spacing(self.presentation.scale_u16(6));

        let tabs_toggle = checkbox(self.state.values.tabs_enabled)
            .label(crate::i18n::tr("Enable table tabs"))
            .on_toggle(Message::TabsToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let inactive_memory_toggle = checkbox(self.state.values.release_inactive_tab_memory)
            .label(crate::i18n::tr("Unload inactive tab results to reduce RAM"))
            .on_toggle(Message::ReleaseInactiveTabMemoryToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let inactive_delay_input = history_input(
            text_field_id("inactive-tab-release-idle"),
            "Seconds",
            &self.state.values.inactive_tab_release_idle_secs_input,
            Message::InactiveTabReleaseIdleSecsChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(120.0)));

        let tabs_section = iced::widget::column![
            tabs_toggle,
            inactive_memory_toggle,
            row![
                self.presentation.label_text("Inactive unload delay"),
                inactive_delay_input
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
        ]
        .spacing(self.presentation.scale_u16(6));

        let schema_autocomplete_toggle = checkbox(self.state.values.schema_autocomplete_enabled)
            .label(crate::i18n::tr(
                "Enable schema autocomplete (tables/columns) in query editor",
            ))
            .on_toggle(Message::SchemaAutocompleteToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let sql_keyword_autocomplete_toggle =
            checkbox(self.state.values.sql_keyword_autocomplete_enabled)
                .label(crate::i18n::tr(
                    "Enable SQL keyword autocomplete in query editor",
                ))
                .on_toggle(Message::SqlKeywordAutocompleteToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font());

        let line_numbers_toggle = checkbox(self.state.values.query_editor_line_numbers_enabled)
            .label(crate::i18n::tr("Show line numbers in query editor"))
            .on_toggle(Message::QueryEditorLineNumbersToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let word_wrap_toggle = checkbox(self.state.values.query_editor_word_wrap_enabled)
            .label(crate::i18n::tr("Wrap long lines in query editor"))
            .on_toggle(Message::QueryEditorWordWrapToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let tab_size_input = history_input(
            text_field_id("tab-size"),
            "Tab size",
            &self.state.values.tab_size.to_string(),
            Message::TabSizeChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(120.0)));

        let insert_spaces_toggle = checkbox(self.state.values.insert_spaces)
            .label(crate::i18n::tr("Insert spaces instead of tabs"))
            .on_toggle(Message::InsertSpacesToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let inline_suggestion_delay_input = history_input(
            text_field_id("inline-suggestion-delay"),
            "Milliseconds",
            &self.state.values.query_inline_suggestion_delay_ms_input,
            Message::QueryInlineSuggestionDelayMsChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(140.0)));

        let schema_autocomplete_status = if self.state.values.schema_autocomplete_enabled {
            "Status: enabled."
        } else {
            "Status: disabled."
        };

        let query_editor_section = iced::widget::column![
            line_numbers_toggle,
            word_wrap_toggle,
            row![self.presentation.label_text("Tab size"), tab_size_input]
                .spacing(self.presentation.scale_u16(6))
                .align_y(Center),
            insert_spaces_toggle,
            row![
                self.presentation.label_text("Inline suggestion delay"),
                inline_suggestion_delay_input,
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            schema_autocomplete_toggle,
            sql_keyword_autocomplete_toggle,
            self.presentation.label_text(format!(
                "{}: {}",
                crate::i18n::tr("Shortcut"),
                self.state.values.autocomplete_tables_shortcut
            )),
            self.presentation.label_text(schema_autocomplete_status),
        ]
        .spacing(self.presentation.scale_u16(6));

        let shortcut_button =
            |label: &str, target: ShortcutCaptureTarget, binding: &ShortcutBinding| {
                let is_capturing = self.state.shortcut_capture_target == Some(target);
                let content: Element<'_, Message> = if is_capturing {
                    self.presentation
                        .label_text("Press to bind... (Esc to cancel)")
                        .into()
                } else {
                    binding
                        .to_string()
                        .split(" + ")
                        .enumerate()
                        .fold(
                            row![]
                                .spacing(self.presentation.scale_u16(4))
                                .align_y(Center),
                            |keys, (index, key)| {
                                let keys = if index == 0 {
                                    keys
                                } else {
                                    keys.push(
                                        self.presentation
                                            .icon_text(ICON_KEY_COMBO_PLUS)
                                            .size(self.presentation.label_text_size()),
                                    )
                                };
                                keys.push(
                                    container(
                                        text(key.to_string())
                                            .font(self.presentation.ui_font())
                                            .size(self.presentation.label_text_size()),
                                    )
                                    .padding([
                                        self.presentation.scale_u16(1),
                                        self.presentation.scale_u16(6),
                                    ])
                                    .style(|theme| tab_chip_style(theme, false)),
                                )
                            },
                        )
                        .into()
                };
                let button = button(content)
                    .padding(self.presentation.input_padding())
                    .style(move |theme, status| {
                        if is_capturing {
                            return compact_primary_button_style(theme, status);
                        }
                        let mut style = compact_button_style(theme, status);
                        if matches!(status, button::Status::Active) {
                            style.background = Some(Background::Color(Color::TRANSPARENT));
                            style.border.width = 0.0;
                        }
                        style
                    })
                    .width(Fill)
                    .on_press(Message::BeginShortcutCapture(target));

                let label = container(self.presentation.label_text(label))
                    .width(Length::FillPortion(3))
                    .align_y(Center);
                let bind = container(button)
                    .width(Length::FillPortion(2))
                    .align_y(Center);

                row![label, bind]
                    .padding([
                        self.presentation.scale_u16(2),
                        self.presentation.scale_u16(8),
                    ])
                    .spacing(self.presentation.scale_u16(8))
                    .align_y(Center)
                    .width(Fill)
            };

        let shortcut_entries = [
            (
                "Open table search",
                ShortcutCaptureTarget::OmniTable,
                &self.state.values.omni_table_shortcut,
            ),
            (
                "Open command palette",
                ShortcutCaptureTarget::OmniCommand,
                &self.state.values.omni_command_shortcut,
            ),
            (
                "Open command palette (alternate)",
                ShortcutCaptureTarget::OmniCommandAlt,
                &self.state.values.omni_command_alt_shortcut,
            ),
            (
                "Open settings modal",
                ShortcutCaptureTarget::OpenSettings,
                &self.state.values.open_settings_shortcut,
            ),
            (
                "Open database switcher",
                ShortcutCaptureTarget::SwitchDatabase,
                &self.state.values.switch_database_shortcut,
            ),
            (
                "Focus sidebar table search",
                ShortcutCaptureTarget::FocusTableSearch,
                &self.state.values.focus_table_search_shortcut,
            ),
            (
                "Toggle active tab pin",
                ShortcutCaptureTarget::ToggleTabPin,
                &self.state.values.toggle_tab_pin_shortcut,
            ),
            (
                "Toggle sidebar",
                ShortcutCaptureTarget::ToggleSidebar,
                &self.state.values.toggle_sidebar_shortcut,
            ),
            (
                "Toggle AI chat sidebar",
                ShortcutCaptureTarget::ToggleChatSidebar,
                &self.state.values.toggle_chat_sidebar_shortcut,
            ),
            (
                "Cycle application theme",
                ShortcutCaptureTarget::CycleTheme,
                &self.state.values.cycle_theme_shortcut,
            ),
            (
                "Execute current query",
                ShortcutCaptureTarget::RunQuery,
                &self.state.values.run_query_shortcut,
            ),
            (
                "Execute selected SQL",
                ShortcutCaptureTarget::RunSelection,
                &self.state.values.run_selection_shortcut,
            ),
            (
                "Trigger query autocomplete (schema)",
                ShortcutCaptureTarget::AutocompleteTables,
                &self.state.values.autocomplete_tables_shortcut,
            ),
            (
                "New query tab",
                ShortcutCaptureTarget::NewQuery,
                &self.state.values.new_query_shortcut,
            ),
            (
                "Close active tab",
                ShortcutCaptureTarget::CloseTab,
                &self.state.values.close_tab_shortcut,
            ),
            (
                "Next tab",
                ShortcutCaptureTarget::NextTab,
                &self.state.values.next_tab_shortcut,
            ),
            (
                "Previous tab",
                ShortcutCaptureTarget::PreviousTab,
                &self.state.values.previous_tab_shortcut,
            ),
            (
                "Next results page",
                ShortcutCaptureTarget::NextResultsPage,
                &self.state.values.next_results_page_shortcut,
            ),
            (
                "Previous results page",
                ShortcutCaptureTarget::PreviousResultsPage,
                &self.state.values.previous_results_page_shortcut,
            ),
            (
                "First results page",
                ShortcutCaptureTarget::FirstResultsPage,
                &self.state.values.first_results_page_shortcut,
            ),
            (
                "Last results page",
                ShortcutCaptureTarget::LastResultsPage,
                &self.state.values.last_results_page_shortcut,
            ),
            (
                "Open table info sidebar",
                ShortcutCaptureTarget::OpenTableInfoSidebar,
                &self.state.values.open_table_info_sidebar_shortcut,
            ),
        ];
        let shortcut_rows = shortcut_entries.into_iter().fold(
            iced::widget::column![].spacing(self.presentation.scale_u16(1)),
            |rows, (label, target, binding)| {
                rows.push(shortcut_button(label, target, binding))
                    .push(self.horizontal_hairline())
            },
        );
        let shortcut_header = row![
            self.presentation
                .muted_label_text(crate::i18n::tr("Action"))
                .width(Length::FillPortion(3)),
            self.presentation
                .muted_label_text(crate::i18n::tr("Shortcut"))
                .width(Length::FillPortion(2)),
        ]
        .padding([
            self.presentation.scale_u16(5),
            self.presentation.scale_u16(8),
        ])
        .spacing(self.presentation.scale_u16(8));
        let shortcuts = iced::widget::column![
            shortcut_header,
            self.horizontal_hairline(),
            scrollable(shortcut_rows).height(Fill).direction(
                iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new()
                        .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                )
            ),
        ]
        .height(Fill);

        let provider_picker = pick_list(
            AI_PROVIDERS,
            Some(self.state.values.ai_provider),
            Message::AiProviderSelected,
        )
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style);

        let ai_endpoint_input = history_input(
            text_field_id("ai-endpoint"),
            "Endpoint URL",
            &self.state.values.ai_endpoint,
            Message::AiEndpointChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let ai_model_input = history_input(
            text_field_id("ai-model"),
            "Model",
            &self.state.values.ai_model,
            Message::AiModelChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let ai_api_key_input = history_input(
            text_field_id("ai-api-key"),
            "API Key",
            &self.state.values.ai_api_key,
            Message::AiApiKeyChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .secure(true)
        .width(Fill);

        let ai_test_button = {
            let label = if self.state.ai_connection_testing {
                "Testing..."
            } else {
                "Test connection"
            };
            let button = button(self.presentation.button_text(label))
                .padding(self.presentation.button_padding_tight())
                .style(compact_primary_button_style);
            if self.state.ai_connection_testing {
                button
            } else {
                button.on_press(Message::TestAiProviderConnection)
            }
        };

        let ai_cli_command_input = history_input(
            text_field_id("ai-cli-command"),
            "Command, for example: claude -p",
            &self.state.values.ai_cli_command,
            Message::AiCliCommandChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let ai_credentials: Element<'_, Message> = if self.state.values.ai_provider
            == crate::ai::AiProvider::LocalCli
        {
            let mut presets = row![self.presentation.label_text("Presets")]
                .spacing(self.presentation.scale_u16(6))
                .align_y(Center);
            for (label, command) in AI_CLI_PRESETS {
                let selected = self.state.values.ai_cli_command.trim() == command;
                presets = presets.push(
                    button(self.presentation.button_text(label))
                        .padding(self.presentation.button_padding_tight())
                        .style(move |theme, status| {
                            if selected {
                                compact_primary_button_style(theme, status)
                            } else {
                                compact_button_style(theme, status)
                            }
                        })
                        .on_press(Message::AiCliPresetSelected(command)),
                );
            }

            iced::widget::column![
                presets,
                ai_cli_command_input,
                self.presentation.label_text(
                    "Runs a CLI you already signed in to. The conversation goes to its stdin, so the tool must accept a prompt there.",
                ),
            ]
            .spacing(self.presentation.scale_u16(6))
            .into()
        } else {
            iced::widget::column![ai_endpoint_input, ai_model_input, ai_api_key_input]
                .spacing(self.presentation.scale_u16(6))
                .into()
        };

        let ai_test_row: Element<'_, Message> = match self.state.ai_test_status.as_ref() {
            Some(status) => {
                let (label, message, style): (&str, &String, fn(&iced::Theme) -> container::Style) =
                    match status {
                        Ok(greeting) => ("Connected", greeting, panel_border_style),
                        Err(error) => ("Test failed", error, alert_style),
                    };

                iced::widget::column![
                    ai_test_button,
                    container(
                        iced::widget::column![
                            self.presentation.label_text(label),
                            scrollable(self.presentation.input_text(message.clone()))
                                .height(Length::Shrink)
                                .width(Fill),
                        ]
                        .spacing(self.presentation.scale_u16(4))
                    )
                    .padding(self.presentation.scale_u16(6))
                    .max_height(self.presentation.scale_f32(180.0))
                    .width(Fill)
                    .style(style),
                ]
                .spacing(self.presentation.scale_u16(6))
                .into()
            }
            None => ai_test_button.into(),
        };

        let ai_provider_section = iced::widget::column![
            iced::widget::column![
                self.presentation.label_text("Provider"),
                provider_picker.width(Length::Fixed(self.presentation.scale_f32(200.0))),
            ]
            .spacing(self.presentation.scale_u16(4)),
            ai_credentials,
            ai_test_row,
            checkbox(self.state.values.ai_send_temperature)
                .label(crate::i18n::tr("Send a temperature with every request"))
                .on_toggle(Message::AiSendTemperatureToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font()),
            self.presentation.label_text(
                "Leave off unless your model needs it: the newest reasoning models reject the temperature parameter.",
            ),
            self.presentation.label_text("Compatible with OpenAI, Anthropic, Ollama, a local CLI, and custom.",),
        ]
        .spacing(self.presentation.scale_u16(6));

        let ai_autocomplete_toggle = checkbox(self.state.values.ai_autocomplete_enabled)
            .label(crate::i18n::tr(
                "Enable AI inline autocomplete in query editor",
            ))
            .on_toggle(Message::AiAutocompleteToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let ai_autocomplete_config =
            crate::ai::AiRequestConfig::for_autocomplete(&self.state.values);
        let ai_autocomplete_status = if !self.state.values.ai_autocomplete_enabled {
            String::from("Status: disabled.")
        } else if !ai_autocomplete_config.supports_autocomplete()
            && !self.state.values.ai_autocomplete_allow_local_cli
        {
            String::from(
                "Status: inactive. Local CLI providers are too slow for inline completion.",
            )
        } else if ai_autocomplete_config.is_usable() {
            format!(
                "Status: active via {} (inline suggestions while typing).",
                ai_autocomplete_config.provider
            )
        } else {
            String::from("Status: inactive until endpoint and model are configured.")
        };

        let ai_autocomplete_local_cli_section: Element<'_, Message> = if ai_autocomplete_config
            .provider
            == crate::ai::AiProvider::LocalCli
        {
            iced::widget::column![
                    checkbox(self.state.values.ai_autocomplete_allow_local_cli)
                        .label(crate::i18n::tr("Use the local CLI for inline completion anyway"))
                        .on_toggle(Message::AiAutocompleteAllowLocalCliToggled)
                        .text_size(self.presentation.label_text_size())
                        .font(self.presentation.ui_font()),
                    self.presentation.label_text(
                        "Each suggestion starts a new CLI process, so expect seconds of delay and one billed call per suggestion.",
                    ),
                ]
                .spacing(self.presentation.scale_u16(4))
                .into()
        } else {
            space::vertical().height(Length::Shrink).into()
        };

        let ai_autocomplete_source_toggle =
            checkbox(self.state.values.ai_autocomplete_use_main_provider)
                .label(crate::i18n::tr(
                    "Use the provider configured in AI > Provider",
                ))
                .on_toggle(Message::AiAutocompleteUseMainProviderToggled)
                .text_size(self.presentation.label_text_size())
                .font(self.presentation.ui_font());

        let ai_autocomplete_provider_section: Element<'_, Message> =
            if self.state.values.ai_autocomplete_use_main_provider {
                self.presentation
                    .label_text(
                        "Autocomplete reuses the endpoint, model, and key from AI > Provider.",
                    )
                    .into()
            } else {
                let provider_picker = pick_list(
                    AI_PROVIDERS,
                    Some(self.state.values.ai_autocomplete_provider),
                    Message::AiAutocompleteProviderSelected,
                )
                .padding(self.presentation.input_padding())
                .text_size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .handle(self.presentation.pick_list_handle())
                .style(compact_pick_list_style)
                .menu_style(compact_pick_list_menu_style);

                let endpoint_input = history_input(
                    text_field_id("ai-autocomplete-endpoint"),
                    "Endpoint URL",
                    &self.state.values.ai_autocomplete_endpoint,
                    Message::AiAutocompleteEndpointChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .width(Fill);

                let model_input = history_input(
                    text_field_id("ai-autocomplete-model"),
                    "Model",
                    &self.state.values.ai_autocomplete_model,
                    Message::AiAutocompleteModelChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .width(Fill);

                let api_key_input = history_input(
                    text_field_id("ai-autocomplete-api-key"),
                    "API Key",
                    &self.state.values.ai_autocomplete_api_key,
                    Message::AiAutocompleteApiKeyChanged,
                    Message::TextEdited,
                )
                .padding(self.presentation.input_padding())
                .size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .style(compact_input_style)
                .secure(true)
                .width(Fill);

                let mut section = iced::widget::column![
                    iced::widget::column![
                        self.presentation.label_text("Autocomplete provider"),
                        provider_picker.width(Length::Fixed(self.presentation.scale_f32(200.0))),
                    ]
                    .spacing(self.presentation.scale_u16(4)),
                ]
                .spacing(self.presentation.scale_u16(6));

                section = section
                    .push(endpoint_input)
                    .push(model_input)
                    .push(api_key_input);

                section.into()
            };

        let ai_send_schema_toggle = checkbox(self.state.values.ai_send_schema_context)
            .label(crate::i18n::tr(
                "Send compact table and column context with AI requests",
            ))
            .on_toggle(Message::AiSendSchemaContextToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let ai_send_query_toggle = checkbox(self.state.values.ai_send_query_context)
            .label(crate::i18n::tr(
                "Send current query context with AI requests",
            ))
            .on_toggle(Message::AiSendQueryContextToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let ai_autocomplete_last_error: Element<'_, Message> =
            match self.query_inline_suggestion_error.as_ref() {
                Some(error) => self
                    .presentation
                    .label_text(format!("{}: {error}", crate::i18n::tr("Last error")))
                    .into(),
                None => space::vertical().height(Length::Shrink).into(),
            };

        let ai_autocomplete_section = iced::widget::column![
            ai_autocomplete_toggle,
            ai_autocomplete_last_error,
            self.presentation.label_text(ai_autocomplete_status),
            self.presentation.label_text(format!(
                "Stop typing for {} ms to get a suggestion, then press {} to accept it.",
                self.state.values.query_inline_suggestion_delay_ms,
                self.state.values.autocomplete_tables_shortcut
            )),
            ai_autocomplete_local_cli_section,
            ai_autocomplete_source_toggle,
            ai_autocomplete_provider_section,
            ai_send_schema_toggle,
            ai_send_query_toggle,
        ]
        .spacing(self.presentation.scale_u16(6));

        let connection_timeout_input = history_input(
            text_field_id("connection-timeout"),
            "Seconds",
            &self.state.values.connection_timeout_secs_input,
            Message::ConnectionTimeoutSecsChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(160.0)));

        let query_timeout_input = history_input(
            text_field_id("query-timeout"),
            "Seconds",
            &self.state.values.query_timeout_secs_input,
            Message::QueryTimeoutSecsChanged,
            Message::TextEdited,
        )
        .padding(self.presentation.input_padding())
        .size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .style(compact_input_style)
        .width(Length::Fixed(self.presentation.scale_f32(160.0)));

        let auto_reconnect_toggle = checkbox(self.state.values.auto_reconnect)
            .label(crate::i18n::tr("Auto-reconnect on connection loss"))
            .on_toggle(Message::AutoReconnectToggled)
            .text_size(self.presentation.label_text_size())
            .font(self.presentation.ui_font());

        let connections_section = iced::widget::column![
            row![
                self.presentation.label_text("Connection timeout"),
                connection_timeout_input
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            row![
                self.presentation.label_text("Query timeout"),
                query_timeout_input
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            auto_reconnect_toggle,
        ]
        .spacing(self.presentation.scale_u16(6));

        let saved_diagrams = if self.state.settings_modal_tab == SettingsModalTab::Diagrams {
            self.diagrams.clone()
        } else {
            Vec::new()
        };
        let diagrams_section = if self.state.settings_diagram_clear_confirmation {
            iced::widget::column![
                self.presentation
                    .label_text("Delete every saved diagram? This cannot be undone.")
            ]
        } else if saved_diagrams.is_empty() {
            iced::widget::column![self.presentation.muted_label_text("No saved diagrams.")]
        } else {
            let table_header = row![
                self.presentation
                    .muted_label_text(crate::i18n::tr("Connection"))
                    .width(Length::FillPortion(3)),
                self.presentation
                    .muted_label_text(crate::i18n::tr("Database"))
                    .width(Length::FillPortion(2)),
                self.presentation
                    .muted_label_text(crate::i18n::tr("Diagram name"))
                    .width(Length::FillPortion(3)),
            ]
            .padding([
                self.presentation.scale_u16(5),
                self.presentation.scale_u16(8),
            ])
            .spacing(self.presentation.scale_u16(8));
            let table_rows = saved_diagrams
                .iter()
                .take(self.state.settings_diagram_visible_rows)
                .fold(
                    iced::widget::column![].spacing(self.presentation.scale_u16(1)),
                    |rows, (connection, database, name)| {
                        rows.push(
                            row![
                                self.presentation
                                    .label_text(connection)
                                    .width(Length::FillPortion(3)),
                                self.presentation
                                    .label_text(database)
                                    .width(Length::FillPortion(2)),
                                self.presentation
                                    .label_text(name)
                                    .width(Length::FillPortion(3)),
                            ]
                            .padding([
                                self.presentation.scale_u16(5),
                                self.presentation.scale_u16(8),
                            ])
                            .spacing(self.presentation.scale_u16(8)),
                        )
                        .push(self.horizontal_hairline())
                    },
                );
            iced::widget::column![
                table_header,
                self.horizontal_hairline(),
                scrollable(table_rows)
                    .height(Fill)
                    .on_scroll(Message::SavedDiagramsScrolled)
                    .direction(iced::widget::scrollable::Direction::Vertical(
                        iced::widget::scrollable::Scrollbar::new()
                            .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
                    )),
            ]
            .height(Fill)
        };

        let nav_button = |title: &str, tab: SettingsModalTab| {
            let is_selected = self.state.settings_modal_tab == tab;
            let label = row![
                space::horizontal().width(Length::Fixed(self.presentation.scale_f32(10.0))),
                self.presentation
                    .button_text(title)
                    .size(self.presentation.button_text_size().saturating_sub(1)),
            ]
            .spacing(self.presentation.scale_u16(4))
            .align_y(Center);
            button(label)
                .padding(self.settings_nav_button_padding())
                .width(Fill)
                .style(move |theme, status| {
                    settings_modal_tab_button_style(is_selected, theme, status)
                })
                .on_press(Message::SettingsTabSelected(tab))
        };

        let nav_group = |label: &str| {
            container(
                row![
                    self.presentation
                        .icon_text(ICON_FOLDER_OPEN)
                        .size(self.presentation.label_text_size().saturating_sub(1)),
                    self.presentation
                        .label_text(label)
                        .size(self.presentation.label_text_size().saturating_sub(1)),
                ]
                .spacing(self.presentation.scale_u16(5))
                .align_y(Center),
            )
            .padding([
                self.presentation.scale_u16(6),
                self.presentation.scale_u16(6),
            ])
            .width(Fill)
        };

        let mut nav_items = iced::widget::column![].spacing(self.settings_nav_spacing());
        nav_items = nav_items
            .push(nav_group("General"))
            .push(nav_button("Appearance", SettingsModalTab::Appearance))
            .push(nav_button("Limits", SettingsModalTab::Limits))
            .push(nav_button("Tabs", SettingsModalTab::Tabs))
            .push(nav_button("Diagrams", SettingsModalTab::Diagrams))
            .push(nav_button("Query Editor", SettingsModalTab::QueryEditor))
            .push(nav_group("Shortcuts"))
            .push(nav_button("Keyboard", SettingsModalTab::Shortcuts))
            .push(nav_group("Connections"))
            .push(nav_button("Connections", SettingsModalTab::Connections));
        if self.state.values.ai_enabled {
            nav_items = nav_items
                .push(nav_group("AI"))
                .push(nav_button("Provider", SettingsModalTab::AiProvider))
                .push(nav_button("Autocomplete", SettingsModalTab::AiAutocomplete));
        }
        nav_items = nav_items
            .push(nav_group("About"))
            .push(nav_button("About CryoDB", SettingsModalTab::About));

        let nav_scroll = scrollable(nav_items).height(Fill).width(Fill).direction(
            iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
            ),
        );

        let nav = container(nav_scroll)
            .padding(self.presentation.scale_u16(10))
            .width(Length::Fixed(nav_width))
            .height(Fill)
            .style(panel_border_style);

        let sections: Element<'_, Message> = match self.state.settings_modal_tab {
            SettingsModalTab::Appearance => container(appearance)
                .width(Fill)
                .max_width(content_inner_width)
                .into(),
            SettingsModalTab::Limits => container(limits)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::Tabs => container(tabs_section)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::Diagrams => diagrams_section.width(Fill).height(Fill).into(),
            SettingsModalTab::QueryEditor => container(query_editor_section)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::Shortcuts => shortcuts.width(Fill).height(Fill).into(),
            SettingsModalTab::Connections => container(connections_section)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::AiProvider => container(ai_provider_section)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::AiAutocomplete => container(ai_autocomplete_section)
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
            SettingsModalTab::About => container(self.about_section())
                .padding(self.presentation.scale_u16(10))
                .width(Fill)
                .max_width(content_inner_width)
                .style(panel_border_style)
                .into(),
        };

        let scroll: Element<'_, Message> = if matches!(
            self.state.settings_modal_tab,
            SettingsModalTab::Diagrams | SettingsModalTab::Shortcuts
        ) {
            container(sections)
                .width(Fill)
                .height(Fill)
                .max_width(content_inner_width)
                .into()
        } else {
            scrollable(
                container(sections)
                    .width(Fill)
                    .max_width(content_inner_width),
            )
            .height(Fill)
            .width(Fill)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .spacing(self.presentation.scale_f32(MODAL_SCROLLBAR_GUTTER)),
            ))
            .into()
        };

        let (section_title, section_description) = match self.state.settings_modal_tab {
            SettingsModalTab::Appearance => {
                ("Appearance", "Accent, fonts, density, themes, and markers.")
            }
            SettingsModalTab::Limits => ("Limits", "Table page and query history limits."),
            SettingsModalTab::Tabs => ("Tabs", "Tab behavior and inactive memory release."),
            SettingsModalTab::Diagrams => (
                "Saved diagrams",
                "Review and remove diagrams saved across connections and databases.",
            ),
            SettingsModalTab::QueryEditor => (
                "Query Editor",
                "Editor display, tabs, and autocomplete behavior.",
            ),
            SettingsModalTab::Shortcuts => (
                "Keyboard",
                "Keyboard bindings and command palette behavior.",
            ),
            SettingsModalTab::Connections => {
                ("Connections", "Connection and query timeout settings.")
            }
            SettingsModalTab::AiProvider => ("Provider", "AI endpoint, model, and API key."),
            SettingsModalTab::AiAutocomplete => ("Autocomplete", "Inline AI suggestion behavior."),
            SettingsModalTab::About => ("About CryoDB", "Version and update channel."),
        };
        let reset_button: Element<'_, Message> = match self.state.settings_modal_tab {
            SettingsModalTab::Diagrams if self.state.settings_diagram_clear_confirmation => row![
                button(self.presentation.button_text("Cancel"))
                    .padding(self.presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::ClearSavedDiagramsCancelled),
                button(self.presentation.button_text("Delete all saved diagrams"))
                    .padding(self.presentation.button_padding_tight())
                    .style(disconnect_button_style)
                    .on_press(Message::ClearSavedDiagramsConfirmed),
            ]
            .spacing(self.presentation.scale_u16(6))
            .into(),
            SettingsModalTab::Diagrams if !saved_diagrams.is_empty() => {
                button(self.presentation.button_text("Delete all saved diagrams"))
                    .padding(self.presentation.button_padding_tight())
                    .style(disconnect_button_style)
                    .on_press(Message::ClearSavedDiagramsRequested)
                    .into()
            }
            _ => match self.state.settings_modal_tab.reset_label() {
                Some(reset_label) => button(self.presentation.button_text(reset_label))
                    .padding(self.presentation.button_padding_tight())
                    .style(disconnect_button_style)
                    .on_press(Message::ResetSettingsSection(self.state.settings_modal_tab))
                    .into(),
                None => space::horizontal().width(Length::Shrink).into(),
            },
        };
        let section_header = row![
            iced::widget::column![
                self.presentation.title_text(section_title),
                self.presentation.muted_label_text(section_description),
            ]
            .spacing(self.presentation.scale_u16(2)),
            space::horizontal(),
            reset_button,
        ]
        .spacing(self.presentation.scale_u16(8))
        .align_y(Center);
        let right_pane = container(
            iced::widget::column![section_header, scroll]
                .spacing(self.presentation.scale_u16(10))
                .height(Fill),
        )
        .padding(self.presentation.scale_u16(10))
        .width(Fill)
        .height(Fill)
        .style(panel_border_style);
        let body = row![nav, right_pane]
            .spacing(self.presentation.scale_u16(10))
            .height(Fill)
            .width(Fill);

        let mut content = iced::widget::column![header].spacing(self.presentation.scale_u16(10));
        if let Some(error) = &self.state.settings_store_error {
            content = content.push(
                self.presentation
                    .label_text(format!("{}: {error}", crate::i18n::tr("Settings error"))),
            );
        }
        let content = content.push(body).width(Fill).height(Fill);

        let modal = container(content)
            .padding(modal_padding)
            .width(Length::Fixed(modal_width))
            .height(Length::Fixed(modal_height))
            .style(panel_style);

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| modal_backdrop_style(theme, backdrop_dim)),
        )
        .on_press(Message::ModalBlocked)
        .on_scroll(|_| Message::ModalBlocked)
        .interaction(mouse::Interaction::Idle);

        let modal_layer = container(modal).width(Fill).height(Fill).center(Fill);

        stack![backdrop, modal_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }
    fn about_section(&self) -> Element<'a, Message> {
        let install = crate::update::detect_install();
        let install_label = match &install {
            crate::update::Install::AppImage(_) => "AppImage",
            crate::update::Install::MacApp(_) => "macOS app bundle",
            crate::update::Install::Windows => "Windows",
            crate::update::Install::Pacman => "pacman package",
            crate::update::Install::Debian => "Debian package",
            crate::update::Install::Other => "Unmanaged build",
        };
        let channel_label = if crate::update::update_channel_enabled() {
            crate::update::manifest_url()
        } else {
            String::from("Disabled for this build")
        };

        let status_label = self.update_status.clone();

        let whats_new_button = button(self.presentation.button_text("What's new"))
            .padding(self.presentation.button_padding())
            .style(compact_button_style)
            .on_press(Message::OpenChangelog);

        let check_button = button(self.presentation.button_text("Check for updates"))
            .padding(self.presentation.button_padding())
            .style(compact_primary_button_style);
        let check_button: Element<'_, Message> = if self.update_busy {
            check_button.into()
        } else {
            check_button.on_press(Message::CheckForUpdates).into()
        };

        let mut column = iced::widget::column![
            row![
                self.presentation.label_text("Version"),
                space::horizontal(),
                self.presentation
                    .label_text(crate::update::current_version()),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            row![
                self.presentation.label_text("Install"),
                space::horizontal(),
                self.presentation.label_text(install_label),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            row![
                self.presentation.label_text("Update channel"),
                space::horizontal(),
                self.presentation
                    .label_text(channel_label)
                    .wrapping(text::Wrapping::Glyph),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center),
            self.presentation.muted_label_text(status_label),
        ]
        .spacing(self.presentation.scale_u16(8))
        .width(Fill);

        if !install.self_updatable() && crate::update::update_channel_enabled() {
            column = column.push(self.presentation.muted_label_text(install.manual_hint()));
        }

        column
            .push(
                row![space::horizontal(), whats_new_button, check_button]
                    .spacing(self.presentation.scale_u16(6))
                    .align_y(Center),
            )
            .into()
    }

    pub(crate) fn settings_nav_button_padding(&self) -> [f32; 2] {
        if self.state.values.large_sidebar_buttons {
            self.presentation.scale_padding([8, 10])
        } else {
            [
                self.presentation.scale_u16(4),
                self.presentation.scale_u16(8),
            ]
        }
    }
    pub(crate) fn settings_nav_spacing(&self) -> f32 {
        if self.state.values.large_sidebar_buttons {
            self.presentation.scale_u16(7)
        } else {
            self.presentation.scale_u16(4)
        }
    }

    fn with_tooltip<'b>(
        &self,
        element: impl Into<Element<'b, Message>>,
        label: impl Into<String>,
    ) -> Element<'b, Message> {
        crate::ui::widgets::tooltip_area::tooltip_area(
            element,
            label,
            Message::Tooltip,
            Message::ClearTooltip,
        )
    }

    fn horizontal_hairline<'b>(&self) -> Element<'b, Message> {
        container(space::horizontal())
            .width(Fill)
            .height(Length::Fixed(1.0))
            .style(|theme: &Theme| container::Style {
                background: Some(crate::ui::theme::tokens(theme).line.into()),
                ..container::Style::default()
            })
            .into()
    }
}
