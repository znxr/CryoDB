use super::{Message, State, TableAction};
use crate::constants::ICON_CHAT_AI;
use crate::model::connection::DatabaseDriver;
use crate::ui::presentation::Presentation;
use crate::ui::styles::{compact_button_style, panel_border_style};
use iced::widget::{button, container, mouse_area, row, space, stack};
use iced::{Center, Element, Fill, Length, Size, alignment};

impl State {
    pub(crate) fn table_context_menu_overlay<'a>(
        &'a self,
        presentation: Presentation<'a>,
        bounds: Size,
        driver: DatabaseDriver,
        chat_available: bool,
    ) -> Element<'a, Message> {
        let Some(state) = &self.table_context_menu else {
            return container(space::horizontal()).into();
        };

        let supports_actions = self.table_supports_table_actions(driver, &state.table);

        let alter_button = button(presentation.label_text("Alter Table..."))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::Alter));
        let add_column_button = button(presentation.label_text("Add Column..."))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::AddColumn));
        let truncate_button = button(presentation.label_text("Truncate"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::Truncate));
        let drop_button = button(presentation.label_text("Drop"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::Drop));
        let duplicate_button = button(presentation.label_text("Duplicate Table"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::Duplicate));
        let rename_button = button(presentation.label_text("Rename"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::Rename));
        let move_to_folder_button = button(presentation.label_text("Move to Folder"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::TableContextAction(TableAction::MoveToFolder));
        let ask_ai_button = button(
            row![
                presentation
                    .icon_text(ICON_CHAT_AI)
                    .size(presentation.label_text_size()),
                presentation.label_text("Ask AI about this table"),
            ]
            .spacing(presentation.scale_u16(6))
            .align_y(Center),
        )
        .padding(presentation.button_padding_tight())
        .width(Fill)
        .style(compact_button_style)
        .on_press(Message::TableContextAskAi);

        let unsupported_note =
            container(presentation.label_text("This PostgreSQL object is read-only."))
                .padding(presentation.button_padding_tight())
                .width(Fill)
                .style(panel_border_style);

        let mut menu = iced::widget::column![].spacing(presentation.scale_u16(2));
        let mut item_count = 0usize;
        if supports_actions {
            menu = menu
                .push(alter_button)
                .push(add_column_button)
                .push(truncate_button)
                .push(drop_button)
                .push(duplicate_button)
                .push(rename_button);
            item_count += 6;
        } else {
            menu = menu.push(unsupported_note);
            item_count += 1;
        }

        menu = menu.push(move_to_folder_button);
        item_count += 1;

        if chat_available {
            menu = menu.push(ask_ai_button);
            item_count += 1;
        }

        let menu_width = presentation.scale_f32(240.0);
        let menu_height = presentation.context_menu_height(item_count);
        let menu = menu.width(Length::Fixed(menu_width));

        let panel = container(menu)
            .padding(presentation.scale_u16(4))
            .style(panel_border_style);
        let offset_x = presentation.scale_f32(4.0);
        let offset_y = presentation.scale_f32(12.0);
        let padding = presentation.context_menu_padding(
            state.position,
            bounds,
            menu_width,
            menu_height,
            offset_x,
            offset_y,
        );

        let menu_layer = container(panel)
            .padding(padding)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseTableContextMenu)
            .on_right_press(Message::CloseTableContextMenu);

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }
    pub(crate) fn folder_context_menu_overlay<'a>(
        &'a self,
        presentation: Presentation<'a>,
        bounds: Size,
    ) -> Element<'a, Message> {
        let Some(state) = &self.folder_context_menu else {
            return container(space::horizontal()).into();
        };
        let is_pinned = self
            .table_folders
            .iter()
            .find(|folder| folder.name == state.folder)
            .is_some_and(|folder| folder.pin_to_top);
        let pin_label = if is_pinned { "Remove pin" } else { "Pin" };

        let dissolve_button = button(presentation.label_text("Move Contents Up"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::DissolveFolderGroup);
        let pin_button = button(presentation.label_text(pin_label))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::ToggleFolderPinFromMenu);
        let rename_button = button(presentation.label_text("Rename"))
            .padding(presentation.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::RenameFolderFromMenu);

        let menu_width = presentation.scale_f32(240.0);
        let menu_height = presentation.context_menu_height(3);
        let menu = iced::widget::column![dissolve_button, pin_button, rename_button]
            .spacing(presentation.scale_u16(2))
            .width(Length::Fixed(menu_width));

        let panel = container(menu)
            .padding(presentation.scale_u16(4))
            .style(panel_border_style);
        let padding = presentation.context_menu_padding(
            state.position,
            bounds,
            menu_width,
            menu_height,
            presentation.scale_f32(4.0),
            presentation.scale_f32(12.0),
        );
        let menu_layer = container(panel)
            .padding(padding)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseFolderContextMenu)
            .on_right_press(Message::CloseFolderContextMenu);

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }
}
