use super::{Message, State};
use crate::constants::ICON_FILTER_LINE;
use crate::constants::{
    ICON_CLOSE_LINE, SIDEBAR_CONTAINER_PADDING, TABLE_INFO_SIDEBAR_MAX_WIDTH,
    TABLE_INFO_SIDEBAR_MIN_WIDTH, TABLE_INFO_SIDEBAR_RATIO,
};
use crate::model::connection::DatabaseDriver;
use crate::ui::presentation::Presentation;
use crate::ui::styles::{compact_button_style, compact_primary_button_style, panel_border_style};
use crate::ui::styles::{modal_backdrop_style, panel_style};
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use crate::utils::format::format_bytes;
use iced::widget::{button, checkbox, container, mouse_area, space};
use iced::widget::{row, scrollable, stack};
use iced::{Center, Fill, alignment};
use iced::{Element, Length, mouse};

impl State {
    pub(crate) fn filter_view<'a>(
        &self,
        driver: DatabaseDriver,
        presentation: Presentation<'a>,
    ) -> Element<'a, Message> {
        let sidebar_filter_button: Element<'_, Message> = {
            let button = button(presentation.icon_text(ICON_FILTER_LINE))
                .padding(presentation.button_padding_icon())
                .style(
                    if Self::sidebar_filter_kinds_for_driver(driver)
                        .iter()
                        .any(|filter| !self.sidebar_filter_enabled(*filter))
                    {
                        compact_primary_button_style
                    } else {
                        compact_button_style
                    },
                )
                .on_press(Message::ToggleSidebarFilter);
            let filter_label = if driver == DatabaseDriver::PostgreSql {
                "Filter objects"
            } else {
                "Sidebar Filters"
            };

            mouse_area(button)
                .on_enter(Message::Tooltip(Some(filter_label.to_string())))
                .on_exit(Message::Tooltip(None))
                .interaction(mouse::Interaction::Pointer)
                .into()
        };

        let sidebar_filter_panel: Element<'_, Message> = if self.sidebar_filter_open {
            let mut toggles = iced::widget::column![].spacing(presentation.scale_u16(3));
            for filter_kind in Self::sidebar_filter_kinds_for_driver(driver) {
                let filter_kind = *filter_kind;
                let toggle = checkbox(self.sidebar_filter_enabled(filter_kind))
                    .label(filter_kind.label())
                    .on_toggle(move |enabled| Message::SidebarFilterToggled {
                        filter: filter_kind,
                        enabled,
                    })
                    .text_size(presentation.label_text_size())
                    .font(presentation.ui_font());
                toggles = toggles.push(toggle);
            }

            container(toggles)
                .padding(presentation.scale_u16(6))
                .width(Length::Fixed(presentation.scale_f32(220.0)))
                .style(panel_border_style)
                .into()
        } else {
            space::horizontal().into()
        };

        let sidebar_filter_dropdown = HeaderDropdown::new(
            sidebar_filter_button,
            sidebar_filter_panel,
            self.sidebar_filter_open,
            Message::CloseSidebarFilter,
        );

        sidebar_filter_dropdown.into()
    }
}

impl State {
    pub(crate) fn table_info_sidebar<'a>(
        &'a self,
        presentation: Presentation<'a>,
        window_width: f32,
    ) -> Element<'a, Message> {
        let base_width = if window_width > 0.0 {
            window_width
        } else {
            1280.0
        };
        let sidebar_width = (base_width * TABLE_INFO_SIDEBAR_RATIO).clamp(
            presentation.scale_f32(TABLE_INFO_SIDEBAR_MIN_WIDTH),
            presentation.scale_f32(TABLE_INFO_SIDEBAR_MAX_WIDTH),
        );

        let table_label = self
            .table_info_table
            .clone()
            .unwrap_or_else(|| String::from("None"));

        let close_button = button(presentation.icon_text(ICON_CLOSE_LINE))
            .padding(presentation.button_padding())
            .style(compact_button_style)
            .on_press(Message::CloseTableInfoSidebar);

        let header = row![
            presentation.heading_text("Table Info"),
            space::horizontal(),
            close_button
        ]
        .spacing(presentation.scale_u16(6))
        .align_y(Center);

        let mut details = iced::widget::column![presentation.label_text(format!(
            "{}: {}",
            crate::i18n::tr("Table"),
            table_label
        ))]
        .spacing(presentation.scale_u16(6));

        if self.table_info_loading {
            details = details.push(presentation.label_text("Loading table information..."));
        } else if let Some(error) = &self.table_info_error {
            details = details.push(presentation.label_text(format!(
                "{}: {}",
                crate::i18n::tr("Error"),
                error
            )));
        } else if let Some(info) = &self.table_info {
            let format_count = |value: Option<u64>| {
                value
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| String::from("-"))
            };
            let format_size =
                |value: Option<u64>| value.map(format_bytes).unwrap_or_else(|| String::from("-"));
            let format_text = |value: Option<&String>| {
                value
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| String::from("-"))
            };
            let total_size = info
                .data_size
                .unwrap_or(0)
                .saturating_add(info.index_size.unwrap_or(0));
            let total_size_text = if info.data_size.is_some() || info.index_size.is_some() {
                format_bytes(total_size)
            } else {
                String::from("-")
            };
            let rows = vec![
                ("Engine", format_text(info.engine.as_ref())),
                ("Rows", format_count(info.rows)),
                ("Charset", format_text(info.charset.as_ref())),
                ("Collation", format_text(info.collation.as_ref())),
                ("Auto Increment", format_count(info.auto_increment)),
                ("Row Format", format_text(info.row_format.as_ref())),
                ("Average Row Length", format_size(info.avg_row_length)),
                ("Data Size", format_size(info.data_size)),
                ("Index Size", format_size(info.index_size)),
                ("Free Size", format_size(info.free_size)),
                ("Total Size", total_size_text),
                ("Created", format_text(info.created_at.as_ref())),
                ("Updated", format_text(info.updated_at.as_ref())),
                ("Checked", format_text(info.checked_at.as_ref())),
                ("Comment", format_text(info.comment.as_ref())),
            ];
            for (label, value) in rows {
                details = details.push(
                    row![
                        presentation.label_text(label),
                        space::horizontal(),
                        presentation.label_text(value)
                    ]
                    .spacing(presentation.scale_u16(8))
                    .align_y(Center),
                );
            }
        } else {
            details = details.push(presentation.label_text("No table information."));
        }

        let body = scrollable(details).height(Fill).direction(
            iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::default(),
            ),
        );

        let panel =
            container(iced::widget::column![header, body].spacing(presentation.scale_u16(8)))
                .padding(presentation.scale_u16(10))
                .width(Length::Fixed(sidebar_width))
                .height(Fill)
                .style(panel_style);

        let sidebar_layer = container(panel)
            .padding(SIDEBAR_CONTAINER_PADDING)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Right)
            .align_y(alignment::Vertical::Top);

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| {
                    modal_backdrop_style(theme, presentation.settings.modal_backdrop_dim)
                }),
        )
        .on_press(Message::CloseTableInfoSidebar)
        .on_right_press(Message::CloseTableInfoSidebar);

        stack![backdrop, sidebar_layer]
            .width(Fill)
            .height(Fill)
            .into()
    }
}

impl State {
    pub(crate) fn table_ddl_overlay<'a>(
        &'a self,
        presentation: Presentation<'a>,
    ) -> Element<'a, Message> {
        let Some(state) = &self.table_ddl else {
            return space::horizontal().width(Length::Fixed(0.0)).into();
        };

        let body: Element<'_, Message> = match (&state.editor, &state.error) {
            (_, Some(error)) => presentation.label_text(error.clone()).into(),
            (Some(editor), _) => container(editor.view().map(Message::TableDdlAction))
                .height(Length::Fixed(presentation.scale_f32(420.0)))
                .width(Fill)
                .into(),
            (None, None) => presentation.label_text("Loading…").into(),
        };

        let close_button = button(presentation.icon_text(ICON_CLOSE_LINE))
            .padding(presentation.button_padding_icon())
            .style(compact_button_style)
            .on_press(Message::CloseTableDdl);

        let header = row![
            presentation.heading_text(state.table.clone()),
            space::horizontal(),
            close_button,
        ]
        .spacing(presentation.scale_u16(6))
        .align_y(Center);

        let mut actions = row![space::horizontal()].spacing(presentation.scale_u16(6));
        if state.sql.is_some() {
            actions = actions.push(
                button(presentation.button_text("Copy"))
                    .padding(presentation.button_padding_tight())
                    .style(compact_button_style)
                    .on_press(Message::CopyTableDdl),
            );
        }
        let panel = container(
            iced::widget::column![header, body, actions.align_y(Center)]
                .spacing(presentation.scale_u16(10))
                .width(Fill),
        )
        .padding(presentation.scale_u16(14))
        .width(Length::Fixed(presentation.scale_f32(760.0)))
        .style(panel_style);

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| {
                    modal_backdrop_style(theme, presentation.settings.modal_backdrop_dim)
                }),
        )
        .on_press(Message::CloseTableDdl)
        .on_scroll(|_| Message::ModalBlocked)
        .interaction(mouse::Interaction::Idle);

        stack![
            backdrop,
            container(panel).width(Fill).height(Fill).center(Fill)
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }
}
