use super::{Message, State};
use crate::constants::{ICON_CHECK_FILL, ICON_CLOSE_LINE};
use crate::model::add_column::{
    AddColumnDraft, AddColumnField, AddColumnFlag, ColumnDefaultKind, ColumnPosition,
};
use crate::model::add_column::{data_type_options, type_takes_length};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::PickerOption;
use crate::model::table::{TableCommand, TableModalState};
use crate::ui::ids::text_field_id;
use crate::ui::presentation::{LayoutMode, Presentation};
use crate::ui::styles::{
    alert_style, compact_button_style, compact_input_style, compact_pick_list_menu_style,
    compact_pick_list_style, compact_primary_button_style, modal_backdrop_style,
    panel_border_style, panel_style,
};
use crate::utils::helpers::sql_quote_table_reference;
use iced::Padding;
use iced::widget::{
    Id, TextInput, button, checkbox, container, mouse_area, pick_list, row, scrollable, space,
    stack, text,
};
use iced::{Center, Element, Fill, Length, Size, Theme, mouse};

pub(crate) struct View<'a> {
    pub(crate) state: &'a State,
    pub(crate) presentation: Presentation<'a>,
    pub(crate) driver: DatabaseDriver,
    pub(crate) columns: &'a [String],
    pub(crate) layout_mode: LayoutMode,
    pub(crate) window_size: Size,
}

pub(super) fn history_input<'a>(
    id: Id,
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> Message + Send + Sync + 'static,
) -> TextInput<'a, Message> {
    crate::ui::widgets::history_input::history_input(
        id,
        placeholder,
        value,
        on_input,
        Message::TextEdited,
    )
}

impl<'a> View<'a> {
    fn add_column_form(&self, table: &'a str, draft: &'a AddColumnDraft) -> Element<'a, Message> {
        let driver = self.driver;
        let is_mysql = matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb);

        let mut step_number = 0u8;
        let mut step = move |title: &str, hint: &str| {
            step_number += 1;
            iced::widget::column![
                self.presentation
                    .label_text(format!("{step_number}. {title}")),
                self.presentation
                    .label_text(hint.to_string())
                    .wrapping(text::Wrapping::Word),
            ]
            .spacing(self.presentation.scale_u16(2))
        };

        let text_field = |field: AddColumnField, id: &str, placeholder: &str, value: &str| {
            history_input(text_field_id(id), placeholder, value, move |value| {
                Message::AddColumnFieldChanged { field, value }
            })
            .padding(self.presentation.input_padding())
            .size(self.presentation.input_text_size())
            .font(self.presentation.ui_font())
            .style(compact_input_style)
            .width(Fill)
        };

        let mut form = iced::widget::column![
            self.presentation
                .label_text(format!("{}: {table}", crate::i18n::tr("Table"))),
            step("Name the column", "Use the name you will query by."),
            text_field(
                AddColumnField::Name,
                "add-column-name",
                "e.g. nickname",
                &draft.name
            ),
            step(
                "Pick a type",
                "Choose one of this driver's types, or type your own.",
            ),
        ]
        .spacing(self.presentation.scale_u16(8))
        .width(Fill);

        let type_options = data_type_options(driver);
        let selected_type = type_options
            .iter()
            .copied()
            .find(|option| option.eq_ignore_ascii_case(draft.data_type.trim()));
        let type_picker = pick_list(type_options, selected_type, |value: &str| {
            Message::AddColumnFieldChanged {
                field: AddColumnField::DataType,
                value: value.to_string(),
            }
        })
        .placeholder("Custom type")
        .padding(self.presentation.input_padding())
        .text_size(self.presentation.input_text_size())
        .font(self.presentation.ui_font())
        .handle(self.presentation.pick_list_handle())
        .style(compact_pick_list_style)
        .menu_style(compact_pick_list_menu_style)
        .width(Fill);

        let type_row: Element<'_, Message> = if type_takes_length(draft.data_type.trim()) {
            row![
                type_picker,
                text_field(
                    AddColumnField::Length,
                    "add-column-length",
                    "length, e.g. 255",
                    &draft.length
                )
                .width(Length::FillPortion(1)),
            ]
            .spacing(self.presentation.scale_u16(6))
            .align_y(Center)
            .into()
        } else {
            type_picker.into()
        };

        form = form
            .push(type_row)
            .push(self.presentation.label_text("Or write the type yourself"))
            .push(text_field(
                AddColumnField::DataType,
                "add-column-type",
                "e.g. ENUM('a','b') or numeric(10,2)",
                &draft.data_type,
            ));

        form = form
            .push(step(
                "Decide what an existing row gets",
                "Existing rows need a value: keep the column nullable, or give it a default.",
            ))
            .push(
                checkbox(draft.nullable)
                    .label(crate::i18n::tr("Allow NULL"))
                    .on_toggle(|enabled| Message::AddColumnFlagChanged {
                        flag: AddColumnFlag::Nullable,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
            )
            .push(
                pick_list(
                    ColumnDefaultKind::ALL,
                    Some(draft.default_kind),
                    Message::AddColumnDefaultKindSelected,
                )
                .padding(self.presentation.input_padding())
                .text_size(self.presentation.input_text_size())
                .font(self.presentation.ui_font())
                .handle(self.presentation.pick_list_handle())
                .style(compact_pick_list_style)
                .menu_style(compact_pick_list_menu_style)
                .width(Fill),
            );

        if matches!(
            draft.default_kind,
            ColumnDefaultKind::Value | ColumnDefaultKind::Expression
        ) {
            let placeholder = if draft.default_kind == ColumnDefaultKind::Expression {
                "e.g. now()"
            } else {
                "e.g. guest — quoting is handled for you"
            };
            form = form.push(text_field(
                AddColumnField::DefaultValue,
                "add-column-default",
                placeholder,
                &draft.default_value,
            ));
        }

        let mut has_constraints = false;
        let mut constraints = iced::widget::column![]
            .spacing(self.presentation.scale_u16(6))
            .width(Fill);
        if driver != DatabaseDriver::Sqlite {
            has_constraints = true;
            constraints = constraints.push(
                checkbox(draft.unique)
                    .label(crate::i18n::tr("Unique"))
                    .on_toggle(|enabled| Message::AddColumnFlagChanged {
                        flag: AddColumnFlag::Unique,
                        enabled,
                    })
                    .text_size(self.presentation.label_text_size())
                    .font(self.presentation.ui_font()),
            );
        }
        if is_mysql {
            has_constraints = true;
            constraints = constraints
                .push(
                    checkbox(draft.auto_increment)
                        .label(crate::i18n::tr("AUTO_INCREMENT"))
                        .on_toggle(|enabled| Message::AddColumnFlagChanged {
                            flag: AddColumnFlag::AutoIncrement,
                            enabled,
                        })
                        .text_size(self.presentation.label_text_size())
                        .font(self.presentation.ui_font()),
                )
                .push(text_field(
                    AddColumnField::Comment,
                    "add-column-comment",
                    "comment (optional)",
                    &draft.comment,
                ));
        }
        if has_constraints {
            form = form
                .push(step(
                    "Add constraints",
                    "Optional — a unique column also needs the existing rows to be unique.",
                ))
                .push(constraints);
        }

        if ColumnPosition::supported_by(driver) {
            form = form
                .push(step(
                    "Choose where it goes",
                    "MySQL and MariaDB can place the column; other drivers append it.",
                ))
                .push(
                    pick_list(
                        ColumnPosition::ALL,
                        Some(draft.position),
                        Message::AddColumnPositionSelected,
                    )
                    .padding(self.presentation.input_padding())
                    .text_size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .handle(self.presentation.pick_list_handle())
                    .style(compact_pick_list_style)
                    .menu_style(compact_pick_list_menu_style)
                    .width(Fill),
                );

            if draft.position == ColumnPosition::After {
                let columns = self.columns;
                if columns.is_empty() {
                    form = form.push(text_field(
                        AddColumnField::AfterColumn,
                        "add-column-after",
                        "existing column name",
                        &draft.after_column,
                    ));
                } else {
                    let options: Vec<PickerOption<String>> = columns
                        .iter()
                        .map(|column| PickerOption::new(column.clone(), column.clone()))
                        .collect();
                    let selected = options
                        .iter()
                        .find(|option| option.value == draft.after_column)
                        .cloned();
                    form = form.push(
                        pick_list(options, selected, |option: PickerOption<String>| {
                            Message::AddColumnFieldChanged {
                                field: AddColumnField::AfterColumn,
                                value: option.value,
                            }
                        })
                        .placeholder("Select a column")
                        .padding(self.presentation.input_padding())
                        .text_size(self.presentation.input_text_size())
                        .font(self.presentation.ui_font())
                        .handle(self.presentation.pick_list_handle())
                        .style(compact_pick_list_style)
                        .menu_style(compact_pick_list_menu_style)
                        .width(Fill),
                    );
                }
            }
        }

        let (preview, preview_style): (String, fn(&Theme) -> container::Style) =
            match draft.definition(driver) {
                Ok(definition) => (
                    format!(
                        "ALTER TABLE {} ADD COLUMN {definition}",
                        sql_quote_table_reference(driver, table)
                    ),
                    panel_border_style,
                ),
                Err(reason) => (reason, alert_style),
            };

        form = form.push(self.presentation.label_text("Statement")).push(
            container(
                self.presentation
                    .label_text(preview)
                    .wrapping(text::Wrapping::WordOrGlyph)
                    .font(crate::ui::presentation::font_for_choice(
                        &self.presentation.settings.editor_font,
                    )),
            )
            .padding(self.presentation.scale_u16(8))
            .width(Fill)
            .style(preview_style),
        );

        scrollable(form.padding(Padding {
            top: 0.0,
            right: self.presentation.scale_f32(8.0),
            bottom: 0.0,
            left: 0.0,
        }))
        .height(Fill)
        .width(Fill)
        .into()
    }
    pub(crate) fn table_modal_view(&self) -> Element<'a, Message> {
        let Some(modal_state) = self.state.table_modal.as_ref() else {
            return space::horizontal().into();
        };

        let layout = self.layout_mode;
        let modal_width = self.presentation.scale_f32(520.0);

        let modal_height = if matches!(modal_state, TableModalState::AddColumn { .. }) {
            self.presentation
                .scale_f32(600.0)
                .min(self.window_size.height * 0.9)
        } else {
            self.presentation.scale_f32(320.0)
        };
        let modal_padding = self.presentation.scale_u16(12);

        let close_button = button(
            self.presentation
                .action_label("Close", ICON_CLOSE_LINE, layout),
        )
        .padding(self.presentation.button_padding())
        .style(compact_button_style)
        .on_press(Message::CloseTableModal);

        let (title, body, action_label): (String, Element<'_, Message>, Option<String>) =
            match modal_state {
                TableModalState::MakeFolder {
                    name,
                    open_by_default,
                    pin_to_top,
                    source_folder,
                } => {
                    let name_input = history_input(
                        text_field_id("table-modal-folder-name"),
                        "Folder name",
                        name,
                        Message::TableModalPrimaryChanged,
                    )
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill);
                    let body = iced::widget::column![
                        self.presentation.label_text("Folder name"),
                        name_input,
                        checkbox(*open_by_default)
                            .label(crate::i18n::tr("Open by default"))
                            .on_toggle(Message::TableModalOpenByDefaultToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                        checkbox(*pin_to_top)
                            .label(crate::i18n::tr("Pin folder to top"))
                            .on_toggle(Message::TableModalPinToTopToggled)
                            .text_size(self.presentation.label_text_size())
                            .font(self.presentation.ui_font()),
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    let (title, action) = if source_folder.is_some() {
                        (String::from("Rename Folder"), String::from("Save folder"))
                    } else {
                        (String::from("Make a Folder"), String::from("Create folder"))
                    };
                    (title, body, Some(action))
                }
                TableModalState::AlterTable { table, clause } => {
                    let clause_input = history_input(
                        text_field_id("table-modal-alter-clause"),
                        "ALTER clause",
                        clause,
                        Message::TableModalPrimaryChanged,
                    )
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill);
                    let body = iced::widget::column![
                        self.presentation
                            .label_text(format!("{}: {table}", crate::i18n::tr("Table"))),
                        self.presentation.label_text("ALTER TABLE clause"),
                        clause_input,
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Alter Table"),
                        body,
                        Some(String::from("Run ALTER")),
                    )
                }
                TableModalState::AddColumn { table, draft } => (
                    String::from("Add Column"),
                    self.add_column_form(table, draft),
                    Some(String::from("Add column")),
                ),
                TableModalState::DuplicateTable { table, new_name } => {
                    let new_name_input = history_input(
                        text_field_id("table-modal-duplicate-name"),
                        "New table name",
                        new_name,
                        Message::TableModalPrimaryChanged,
                    )
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill);
                    let body = iced::widget::column![
                        self.presentation
                            .label_text(format!("{}: {table}", crate::i18n::tr("Source table"))),
                        self.presentation.label_text("Duplicate to"),
                        new_name_input,
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Duplicate Table"),
                        body,
                        Some(String::from("Duplicate")),
                    )
                }
                TableModalState::RenameTable { table, new_name } => {
                    let new_name_input = history_input(
                        text_field_id("table-modal-rename"),
                        "New table name",
                        new_name,
                        Message::TableModalPrimaryChanged,
                    )
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill);
                    let body = iced::widget::column![
                        self.presentation
                            .label_text(format!("{}: {table}", crate::i18n::tr("Current table"))),
                        self.presentation.label_text("Rename to"),
                        new_name_input,
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Rename Table"),
                        body,
                        Some(String::from("Rename")),
                    )
                }
                TableModalState::RenameQueryTab { name, .. } => {
                    let name_input = history_input(
                        text_field_id("query-tab-rename"),
                        "Tab name",
                        name,
                        Message::TableModalPrimaryChanged,
                    )
                    .on_submit(Message::SubmitTableModal)
                    .padding(self.presentation.input_padding())
                    .size(self.presentation.input_text_size())
                    .font(self.presentation.ui_font())
                    .style(compact_input_style)
                    .width(Fill);
                    let body = iced::widget::column![
                        self.presentation.label_text("Rename to"),
                        name_input
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Rename Tab"),
                        body,
                        Some(String::from("Rename")),
                    )
                }
                TableModalState::MoveToFolder { table, folder } => {
                    let options = self.state.folder_picker_options();
                    let selected = options
                        .iter()
                        .find(|option| option.value == *folder)
                        .cloned();
                    let folder_picker =
                        pick_list(options, selected, Message::TableModalFolderSelected)
                            .padding(self.presentation.input_padding())
                            .text_size(self.presentation.input_text_size())
                            .font(self.presentation.ui_font())
                            .handle(self.presentation.pick_list_handle())
                            .style(compact_pick_list_style)
                            .menu_style(compact_pick_list_menu_style)
                            .width(Fill);
                    let body = iced::widget::column![
                        self.presentation
                            .label_text(format!("{}: {table}", crate::i18n::tr("Table"))),
                        self.presentation.label_text("Target folder"),
                        folder_picker,
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Move to Folder"),
                        body,
                        Some(String::from("Move")),
                    )
                }
                TableModalState::ConfirmFoldersImport {
                    path,
                    groups,
                    existing_groups,
                    existing_tables,
                } => {
                    let replacement_warning = format!(
                        "This will replace {} existing folder{} and {} mapped table{} in the current database.",
                        existing_groups,
                        if *existing_groups == 1 { "" } else { "s" },
                        existing_tables,
                        if *existing_tables == 1 { "" } else { "s" },
                    );
                    let body = iced::widget::column![
                        self.presentation
                            .label_text("Import folders from this backup file?")
                            .wrapping(text::Wrapping::Word),
                        self.presentation
                            .label_text(replacement_warning)
                            .wrapping(text::Wrapping::Word),
                        self.presentation.label_text(format!(
                            "{}: {}",
                            crate::i18n::tr("Incoming groups"),
                            groups.len()
                        )),
                        self.presentation
                            .label_text(format!("{}: {}", crate::i18n::tr("File"), path.display()))
                            .wrapping(text::Wrapping::Word),
                    ]
                    .spacing(self.presentation.scale_u16(8))
                    .width(Fill)
                    .into();
                    (
                        String::from("Import Folders"),
                        body,
                        Some(String::from("Import")),
                    )
                }
                TableModalState::Confirm {
                    title,
                    summary,
                    command,
                } => {
                    let action = match command {
                        TableCommand::Truncate { .. } => "Truncate",
                        TableCommand::Drop { .. } => "Drop",
                        _ => "Confirm",
                    };
                    let body = iced::widget::column![self.presentation.label_text(summary.clone())]
                        .spacing(self.presentation.scale_u16(8))
                        .width(Fill)
                        .into();
                    (title.clone(), body, Some(action.to_string()))
                }
            };

        let action_button: Element<'_, Message> = if let Some(action_label) = action_label {
            button(
                self.presentation
                    .action_label(&action_label, ICON_CHECK_FILL, layout),
            )
            .padding(self.presentation.button_padding())
            .style(compact_primary_button_style)
            .on_press(Message::SubmitTableModal)
            .into()
        } else {
            space::horizontal().into()
        };

        let header = row![
            self.presentation.heading_text(title),
            space::horizontal(),
            close_button,
        ]
        .spacing(self.presentation.scale_u16(6))
        .align_y(Center);

        let content = iced::widget::column![
            header,
            body,
            row![space::horizontal(), action_button]
                .spacing(self.presentation.scale_u16(6))
                .align_y(Center),
        ]
        .spacing(self.presentation.scale_u16(10))
        .width(Fill)
        .height(Fill);

        let modal = container(content)
            .padding(modal_padding)
            .width(Length::Fixed(modal_width))
            .height(Length::Fixed(modal_height))
            .style(panel_style);

        let dim = self.presentation.settings.modal_backdrop_dim;
        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(move |theme| modal_backdrop_style(theme, dim)),
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
}
