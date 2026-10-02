use crate::app::core::App;
use crate::app::message::Message;
use crate::app::types::LayoutMode;
use crate::app::view::history_input;
use crate::constants::{
    ICON_DIAGRAM_CODE, ICON_FOLDER_CLOSED, ICON_FOLDER_OPEN, ICON_INFO_LINE,
    ICON_MODERN_FOLDER_CLOSED, ICON_MODERN_FOLDER_OPEN, ICON_MODERN_TABLE, ICON_MORE_OPTIONS,
    ICON_PIN, ICON_POSTGRES_SCHEMA, ICON_POSTGRES_TABLE, ICON_REFRESH_LINE, ICON_RELATIONS,
    ICON_TRIGGER, MODERN_QUERY_STRIP_HEIGHT, SIDEBAR_LIST_RIGHT_PADDING, SIDEBAR_SCROLLBAR_GUTTER,
    SIDEBAR_SCROLLBAR_WIDTH, SIDEBAR_TRIGGER_INDENT,
};
use crate::model::connection::DatabaseDriver;
use crate::model::table::{
    PostgresObjectKind, SidebarRelationEntry, SidebarTriggerEntry, TableModalState,
};
use crate::ui::ids::{sidebar_tables_scroll_id, table_search_input_id};
use crate::ui::styles::{
    compact_button_style, compact_input_style, compact_tab_active_button_style, panel_border_style,
    panel_style, sidebar_folder_button_style, sidebar_table_button_style,
};
use crate::ui::widgets::header_dropdown::HeaderDropdown;
use crate::utils::text::truncate_with_ellipsis;
use iced::widget::{
    button, container, mouse_area, responsive, row, scrollable, space, stack, text,
};
use iced::{Center, Element, Fill, Font, Length, Padding, Point, Size, alignment, mouse};

impl App {
    pub(crate) fn table_ddl_button(&self) -> Element<'_, Message> {
        let available = self.current_table_for_info().is_some();
        let button = button(self.icon_text(ICON_DIAGRAM_CODE))
            .padding(self.button_padding_icon())
            .style(compact_button_style);
        let button: Element<'_, Message> = if available {
            button
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::OpenTableDdl,
                ))
                .into()
        } else {
            button.into()
        };
        self.with_tooltip(button, "Show table DDL")
    }

    pub(crate) fn table_ddl_overlay(&self) -> Element<'_, Message> {
        self.workspace
            .explorer
            .table_ddl_overlay(self.presentation())
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn table_info_button(&self) -> Element<'_, Message> {
        let has_table = self.current_table_for_info().is_some();
        let icon = if self.workspace.explorer.table_info_loading {
            self.loading_spinner_icon()
        } else {
            ICON_INFO_LINE
        };
        let button = button(self.icon_text(icon))
            .padding(self.button_padding_icon())
            .style(compact_button_style);
        let button: Element<'_, Message> =
            if has_table && !self.workspace.explorer.table_info_loading {
                button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::OpenTableInfoSidebar,
                    ))
                    .into()
            } else {
                button.into()
            };
        self.with_shortcut_tooltip(
            button,
            "Table info",
            &self.settings.values.open_table_info_sidebar_shortcut,
        )
    }

    pub(crate) fn table_info_sidebar(&self) -> Element<'_, Message> {
        self.workspace
            .explorer
            .table_info_sidebar(self.presentation(), self.shell.window_size.width)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn sidebar(&self, layout: LayoutMode) -> Element<'_, Message> {
        let filter = self
            .workspace
            .explorer
            .table_search
            .trim()
            .to_ascii_lowercase();
        let postgres_object_matches =
            if self.connections.current.driver == DatabaseDriver::PostgreSql {
                self.workspace
                    .explorer
                    .postgres_sidebar_objects
                    .iter()
                    .any(|object| {
                        if !self
                            .workspace
                            .explorer
                            .sidebar_postgres_kind_visible(object.kind)
                        {
                            return false;
                        }
                        if filter.is_empty() {
                            return true;
                        }
                        let search_blob = format!(
                            "{} {} {} {}",
                            object.schema,
                            object.name,
                            object.kind.searchable_label(),
                            object.qualified_name()
                        )
                        .to_ascii_lowercase();
                        search_blob.contains(&filter)
                    })
            } else {
                false
            };
        let filtered_tables: Vec<&String> = self
            .workspace
            .explorer
            .tables
            .iter()
            .filter(|table| {
                if !self.sidebar_table_visible(table) {
                    return false;
                }
                if filter.is_empty() {
                    return true;
                }
                self.table_search_terms(table)
                    .to_ascii_lowercase()
                    .contains(&filter)
            })
            .collect();
        let show_sidebar_relations = self.workspace.explorer.sidebar_filter_relationships
            && (!filter.is_empty() || !self.workspace.explorer.sidebar_filter_tables);
        let filtered_sidebar_relations: Vec<&SidebarRelationEntry> = if show_sidebar_relations {
            self.workspace
                .explorer
                .sidebar_relations
                .iter()
                .filter(|entry| {
                    if filter.is_empty() {
                        return true;
                    }
                    let search_blob = format!(
                        "{} {} {} {} relationship",
                        entry.table,
                        entry.relation.column,
                        entry.relation.referenced_table,
                        entry.relation.referenced_column
                    )
                    .to_ascii_lowercase();
                    search_blob.contains(&filter)
                })
                .collect()
        } else {
            Vec::new()
        };
        let show_sidebar_triggers = self.workspace.explorer.sidebar_filter_triggers
            && (!filter.is_empty() || !self.workspace.explorer.sidebar_filter_tables);
        let filtered_sidebar_triggers: Vec<&SidebarTriggerEntry> = if show_sidebar_triggers {
            self.workspace
                .explorer
                .sidebar_triggers
                .iter()
                .filter(|entry| {
                    if filter.is_empty() {
                        return true;
                    }
                    let search_blob = format!(
                        "{} {} {} {} trigger",
                        entry.table, entry.trigger.name, entry.trigger.timing, entry.trigger.event,
                    )
                    .to_ascii_lowercase();
                    search_blob.contains(&filter)
                })
                .collect()
        } else {
            Vec::new()
        };
        let has_sidebar_objects = !self.workspace.explorer.tables.is_empty()
            || (self.connections.current.driver == DatabaseDriver::PostgreSql
                && !self.workspace.explorer.postgres_sidebar_objects.is_empty());
        let list: Element<'_, Message> = if self.connections.loading_tables {
            iced::widget::column![self.label_text(
                if self.connections.current.driver == DatabaseDriver::PostgreSql {
                    "Loading objects..."
                } else {
                    "Loading tables..."
                }
            )]
            .spacing(self.scale_u16(5))
            .into()
        } else if !has_sidebar_objects {
            iced::widget::column![self.label_text(
                if self.connections.current.driver == DatabaseDriver::PostgreSql {
                    "No objects loaded."
                } else {
                    "No tables loaded."
                }
            )]
            .spacing(self.scale_u16(5))
            .into()
        } else if filtered_tables.is_empty()
            && !postgres_object_matches
            && filtered_sidebar_relations.is_empty()
            && filtered_sidebar_triggers.is_empty()
        {
            let empty_label = if filter.is_empty() && self.sidebar_filter_active() {
                "No sidebar items in selected filters."
            } else if self.connections.current.driver == DatabaseDriver::PostgreSql {
                "No matching objects."
            } else {
                "No matching tables."
            };
            iced::widget::column![self.label_text(empty_label)]
                .spacing(self.scale_u16(5))
                .into()
        } else {
            let mut tables = iced::widget::column![].spacing(self.sidebar_list_spacing());
            let folder_indent = self.scale_f32(SIDEBAR_TRIGGER_INDENT);
            let child_max_chars = self.sidebar_child_max_chars(layout);
            if self.workspace.explorer.folder_generation_running {
                let banner = container(
                    row![
                        self.icon_text(self.loading_spinner_icon())
                            .size(self.sidebar_icon_size()),
                        self.label_text("Generating folders..."),
                    ]
                    .spacing(self.scale_u16(4))
                    .align_y(Center),
                )
                .padding(self.scale_u16(6))
                .width(Fill)
                .style(panel_border_style);
                tables = tables.push(banner);
            }

            for folder in &self.workspace.explorer.table_folders {
                let folder_tables = filtered_tables
                    .iter()
                    .copied()
                    .filter(|table| self.table_assigned_to_folder(table, &folder.name))
                    .collect::<Vec<_>>();

                let folder_matches =
                    !filter.is_empty() && folder.name.to_ascii_lowercase().contains(&filter);
                if folder_tables.is_empty()
                    && !folder_matches
                    && (!filter.is_empty() || self.sidebar_object_filter_active())
                {
                    continue;
                }

                let table_count = self
                    .workspace
                    .explorer
                    .tables
                    .iter()
                    .filter(|table| self.table_assigned_to_folder(table, &folder.name))
                    .count();

                let folder_label = if self.modern() {
                    self.tracked_caption(&folder.name)
                } else {
                    format!("{} ({})", folder.name, table_count)
                };
                let folder_count: Element<'_, Message> = if self.modern() {
                    self.muted_label_text(table_count.to_string()).into()
                } else {
                    space::horizontal().width(Length::Shrink).into()
                };
                let icon = match (self.modern(), folder.is_open) {
                    (true, true) => ICON_MODERN_FOLDER_OPEN,
                    (true, false) => ICON_MODERN_FOLDER_CLOSED,
                    (false, true) => ICON_FOLDER_OPEN,
                    (false, false) => ICON_FOLDER_CLOSED,
                };
                let drop_target = self.workspace.explorer.sidebar_drag_active
                    && self
                        .workspace
                        .explorer
                        .sidebar_drop_folder
                        .as_deref()
                        .is_some_and(|name| name == folder.name.as_str());
                let folder_name = folder.name.clone();
                let pin_icon: Element<'_, Message> = if folder.pin_to_top {
                    self.icon_text(ICON_PIN)
                        .size(self.sidebar_icon_size())
                        .into()
                } else {
                    space::horizontal().width(Length::Shrink).into()
                };
                let folder_icon = self.icon_text(icon).size(self.sidebar_icon_size());
                let folder_icon = if self.modern() {
                    folder_icon.color(crate::ui::theme::tokens(&self.theme()).ghost)
                } else {
                    folder_icon
                };
                let folder_button = button(
                    container(
                        row![
                            folder_icon,
                            self.label_text(folder_label).font(if self.modern() {
                                Font {
                                    weight: iced::font::Weight::Bold,
                                    ..self.ui_font()
                                }
                            } else {
                                self.ui_font()
                            }),
                            space::horizontal(),
                            folder_count,
                            pin_icon,
                        ]
                        .spacing(self.scale_u16(4))
                        .align_y(Center),
                    )
                    .width(Fill),
                )
                .padding(self.sidebar_button_padding_tight())
                .width(Fill)
                .style(move |theme, status| {
                    sidebar_folder_button_style(drop_target, folder.pin_to_top, theme, status)
                })
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::ToggleFolderOpen(
                            folder_name.clone(),
                        ),
                    ),
                ));

                let folder_name = folder.name.clone();
                let folder_button = mouse_area(folder_button)
                    .on_right_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::FolderContextMenuRequested {
                            folder: folder_name,
                        },
                    )))
                    .on_release(Message::Workspace(crate::app::features::workspace::Message::Explorer(crate::app::features::workspace::explorer::Message::SidebarDragReleased)));

                tables = tables.push(folder_button);

                if folder.is_open {
                    for table in folder_tables {
                        let entry = self.sidebar_table_entry(table, layout, child_max_chars);
                        tables = tables.push(
                            container(entry)
                                .padding(iced::padding::left(folder_indent))
                                .width(Fill),
                        );
                    }
                }
            }

            let root_tables = filtered_tables
                .iter()
                .copied()
                .filter(|table| !self.table_has_folder_assignment(table))
                .collect::<Vec<_>>();

            if self.connections.current.driver == DatabaseDriver::PostgreSql
                && !self.workspace.explorer.postgres_sidebar_objects.is_empty()
            {
                let mut shown_root_keys = std::collections::HashSet::new();
                let mut schemas = self
                    .workspace
                    .explorer
                    .postgres_sidebar_objects
                    .iter()
                    .map(|object| object.schema.clone())
                    .collect::<Vec<_>>();
                schemas.sort_by(|left, right| {
                    left.to_ascii_lowercase()
                        .cmp(&right.to_ascii_lowercase())
                        .then_with(|| left.cmp(right))
                });
                schemas.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

                for schema in schemas {
                    let mut schema_sections =
                        iced::widget::column![].spacing(self.sidebar_list_spacing());
                    let mut schema_count = 0usize;

                    for kind in PostgresObjectKind::ordered() {
                        if !self.workspace.explorer.sidebar_postgres_kind_visible(kind) {
                            continue;
                        }

                        let mut kind_entries =
                            iced::widget::column![].spacing(self.sidebar_list_spacing());
                        let mut kind_count = 0usize;

                        if kind.is_selectable_table() {
                            for table_name in &root_tables {
                                let key = self.table_key(table_name);
                                if key.is_empty() {
                                    continue;
                                }

                                let Some((table_schema, _)) = key.split_once('.') else {
                                    continue;
                                };
                                if !table_schema.eq_ignore_ascii_case(&schema) {
                                    continue;
                                }

                                let table_kind = self
                                    .postgres_object_kind_for_table(table_name)
                                    .unwrap_or(PostgresObjectKind::Table);
                                if table_kind != kind {
                                    continue;
                                }

                                let search_blob = format!(
                                    "{} {} {}",
                                    self.table_search_terms(table_name),
                                    schema,
                                    kind.searchable_label()
                                )
                                .to_ascii_lowercase();
                                if !filter.is_empty() && !search_blob.contains(&filter) {
                                    continue;
                                }

                                if !shown_root_keys.insert(key) {
                                    continue;
                                }

                                kind_entries = kind_entries.push(self.sidebar_tree_wrap(
                                    2,
                                    self.sidebar_table_entry(table_name, layout, child_max_chars),
                                ));
                                kind_count += 1;
                            }
                        } else {
                            for object in &self.workspace.explorer.postgres_sidebar_objects {
                                if !object.schema.eq_ignore_ascii_case(&schema)
                                    || object.kind != kind
                                {
                                    continue;
                                }

                                let search_blob = format!(
                                    "{} {} {} {}",
                                    object.schema,
                                    object.name,
                                    object.kind.searchable_label(),
                                    object.qualified_name()
                                )
                                .to_ascii_lowercase();
                                if !filter.is_empty() && !search_blob.contains(&filter) {
                                    continue;
                                }

                                let name = object.name.clone();
                                let is_object_selected = self.postgres_object_is_selected(object);
                                let (display, name_truncated) =
                                    truncate_with_ellipsis(&name, child_max_chars);
                                let object_icon = App::postgres_object_icon(object.kind);
                                let object_label = container(
                                    row![
                                        self.icon_text(object_icon).size(self.sidebar_icon_size()),
                                        self.label_text(display)
                                            .wrapping(text::Wrapping::None)
                                            .width(Fill),
                                    ]
                                    .spacing(self.scale_u16(4))
                                    .align_y(Center),
                                )
                                .width(Fill)
                                .clip(true);

                                let object_button = button(object_label)
                                    .width(Fill)
                                    .padding(self.button_padding_tight())
                                    .style(move |theme, status| {
                                        if is_object_selected {
                                            compact_tab_active_button_style(theme, status)
                                        } else {
                                            compact_button_style(theme, status)
                                        }
                                    })
                                    .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                                        crate::app::features::workspace::explorer::Message::SidebarPostgresObjectPressed(
                                            object.clone(),
                                        ),
                                    )));

                                let object_entry: Element<'_, Message> = if name_truncated {
                                    self.with_tooltip(object_button, name)
                                } else {
                                    object_button.into()
                                };

                                let object_entry: Element<'_, Message> = if object.kind
                                    == PostgresObjectKind::Role
                                {
                                    mouse_area(object_entry)
                                            .on_right_press(
                                                Message::Workspace(crate::app::features::workspace::Message::Explorer(crate::app::features::workspace::explorer::Message::SidebarPostgresObjectContextMenuRequested(
                                                    object.clone(),
                                                ))),
                                            )
                                            .into()
                                } else {
                                    object_entry
                                };

                                kind_entries =
                                    kind_entries.push(self.sidebar_tree_wrap(2, object_entry));
                                kind_count += 1;
                            }
                        }

                        if kind_count == 0 {
                            continue;
                        }

                        schema_count += kind_count;
                        let kind_icon = App::postgres_object_icon(kind);
                        let kind_is_open = self
                            .workspace
                            .explorer
                            .is_postgres_schema_kind_open(&schema, kind);
                        let kind_toggle_label = if kind_is_open { "[-]" } else { "[+]" };
                        let schema_name = schema.clone();

                        let kind_button = button(
                            container(
                                row![
                                    self.label_text(kind_toggle_label),
                                    self.icon_text(kind_icon).size(self.sidebar_icon_size()),
                                    self.label_text(format!(
                                        "{} ({})",
                                        kind.section_label(),
                                        kind_count
                                    )),
                                ]
                                .spacing(self.scale_u16(4))
                                .align_y(Center),
                            )
                            .width(Fill),
                        )
                        .padding(self.sidebar_button_padding_tight())
                        .width(Fill)
                        .style(compact_button_style)
                        .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(crate::app::features::workspace::explorer::Message::TogglePostgresSchemaKindOpen {
                            schema: schema_name,
                            kind,
                        })));

                        schema_sections =
                            schema_sections.push(self.sidebar_tree_wrap(1, kind_button.into()));
                        if kind_is_open {
                            schema_sections = schema_sections.push(kind_entries);
                        }
                    }

                    if schema_count == 0 {
                        continue;
                    }

                    let schema_is_open = self.workspace.explorer.is_postgres_schema_open(&schema);
                    let schema_name = schema.clone();

                    let schema_header = button(
                        container(
                            row![
                                self.icon_text(ICON_POSTGRES_SCHEMA)
                                    .size(self.sidebar_icon_size()),
                                self.label_text(format!("{} ({})", schema, schema_count)),
                            ]
                            .spacing(self.scale_u16(4))
                            .align_y(Center),
                        )
                        .width(Fill),
                    )
                    .padding(self.sidebar_button_padding_tight())
                    .width(Fill)
                    .style(compact_button_style)
                    .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(crate::app::features::workspace::explorer::Message::TogglePostgresSchemaOpen(schema_name))));

                    let schema_header = container(schema_header)
                        .width(Fill)
                        .style(panel_border_style);

                    tables = tables.push(schema_header);
                    if schema_is_open {
                        tables = tables.push(schema_sections);
                    }
                }

                for table in root_tables {
                    let key = self.table_key(table);
                    if key.is_empty() || shown_root_keys.contains(&key) {
                        continue;
                    }
                    tables = tables.push(self.sidebar_table_entry(table, layout, child_max_chars));
                }
            } else {
                for table in root_tables {
                    tables = tables.push(self.sidebar_table_entry(table, layout, child_max_chars));
                }
            }

            if self.workspace.explorer.sidebar_filter_relationships
                && !filtered_sidebar_relations.is_empty()
            {
                let relation_count = filtered_sidebar_relations.len();
                let mut relation_entries = iced::widget::column![].spacing(self.scale_u16(2));
                for relation_entry in filtered_sidebar_relations {
                    relation_entries = relation_entries
                        .push(self.sidebar_relation_entry(relation_entry, child_max_chars));
                }

                let relations_header = row![
                    self.icon_text(ICON_RELATIONS)
                        .size(self.sidebar_icon_size()),
                    self.label_text(format!(
                        "{} ({relation_count})",
                        crate::i18n::tr("Relationships")
                    )),
                ]
                .spacing(self.scale_u16(3))
                .align_y(Center);

                let relations_section = container(
                    iced::widget::column![relations_header, relation_entries]
                        .spacing(self.scale_u16(3)),
                )
                .padding(self.scale_u16(4))
                .width(Fill)
                .style(panel_border_style);

                tables = tables.push(relations_section);
            }

            if self.workspace.explorer.sidebar_filter_triggers
                && !filtered_sidebar_triggers.is_empty()
            {
                let trigger_count = filtered_sidebar_triggers.len();
                let mut trigger_entries = iced::widget::column![].spacing(self.scale_u16(2));
                for trigger_entry in filtered_sidebar_triggers {
                    trigger_entries = trigger_entries
                        .push(self.sidebar_trigger_entry(trigger_entry, child_max_chars));
                }

                let triggers_header = row![
                    self.icon_text(ICON_TRIGGER).size(self.sidebar_icon_size()),
                    self.label_text(format!("{} ({trigger_count})", crate::i18n::tr("Triggers"))),
                ]
                .spacing(self.scale_u16(3))
                .align_y(Center);

                let triggers_section = container(
                    iced::widget::column![triggers_header, trigger_entries]
                        .spacing(self.scale_u16(3)),
                )
                .padding(self.scale_u16(4))
                .width(Fill)
                .style(panel_border_style);

                tables = tables.push(triggers_section);
            }

            tables.into()
        };

        let search_input = history_input(
            table_search_input_id(),
            if self.connections.current.driver == DatabaseDriver::PostgreSql {
                "Search objects"
            } else {
                "Search tables"
            },
            &self.workspace.explorer.table_search,
            |search| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::TableSearchChanged(search),
                ))
            },
        )
        .padding(self.input_padding())
        .size(self.input_text_size())
        .font(self.ui_font())
        .style(compact_input_style)
        .width(Fill);

        let sidebar_filter_dropdown = self
            .workspace
            .explorer
            .filter_view(self.connections.current.driver, self.presentation())
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            });

        let refresh_button: Element<'_, Message> = {
            let is_refreshing = self.connections.loading_tables
                || (self.settings.values.multiple_connections_layout
                    && self.connections.loading_databases);
            let icon = if is_refreshing {
                self.loading_spinner_icon()
            } else {
                ICON_REFRESH_LINE
            };
            let refresh_content: Element<'_, Message> = if self.modern() {
                container(self.icon_text(icon))
                    .height(Fill)
                    .align_y(Center)
                    .into()
            } else {
                self.icon_text(icon).into()
            };
            let button = button(refresh_content)
                .padding(self.button_padding_icon())
                .height(if self.modern() { Fill } else { Length::Shrink })
                .style(compact_button_style);
            let button: Element<'_, Message> = if is_refreshing {
                button.into()
            } else if self.settings.values.multiple_connections_layout {
                button
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::RefreshDatabaseAndTables,
                    ))
                    .into()
            } else {
                button
                    .on_press(Message::Connections(
                        crate::app::features::connections::Message::RefreshTables,
                    ))
                    .into()
            };
            let refresh_label = if self.settings.values.multiple_connections_layout {
                "Refresh"
            } else if self.connections.current.driver == DatabaseDriver::PostgreSql {
                "Refresh objects"
            } else {
                "Refresh tables"
            };

            self.with_tooltip(button, refresh_label)
        };

        let sidebar_tools_button: Element<'_, Message> = {
            let tools_content: Element<'_, Message> = if self.modern() {
                container(self.icon_text(ICON_MORE_OPTIONS))
                    .height(Fill)
                    .align_y(Center)
                    .into()
            } else {
                self.icon_text(ICON_MORE_OPTIONS).into()
            };
            let button = button(tools_content)
                .padding(self.button_padding_icon())
                .height(if self.modern() { Fill } else { Length::Shrink })
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::ToggleSidebarTools,
                    ),
                ));

            mouse_area(button)
                .on_enter(Message::Shell(
                    crate::app::shell::Message::SetHoveredTooltip("Sidebar tools".to_string()),
                ))
                .on_exit(Message::Shell(
                    crate::app::shell::Message::ClearHoveredTooltip,
                ))
                .interaction(mouse::Interaction::Pointer)
                .into()
        };

        let sidebar_tools_panel: Element<'_, Message> =
            if self.workspace.explorer.sidebar_tools_open {
                let make_folder = button(
                    container(
                        self.button_text("Make Folder")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.sidebar_button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::OpenMakeFolderModal,
                    ),
                ));

                let collapse_folders = button(
                    container(
                        self.button_text("Collapse All Folders")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::CollapseAllFolders,
                    ),
                ));

                let expand_folders = button(
                    container(
                        self.button_text("Expand All Folders")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style)
                .on_press(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::ExpandAllFolders,
                    ),
                ));

                let generate_folders_button = button(
                    container(
                        self.button_text("Group Folders (AI)")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
                let generate_folders_button: Element<'_, Message> =
                    if !self.settings.values.ai_enabled {
                        space::horizontal().into()
                    } else if self.workspace.explorer.folder_generation_running {
                        generate_folders_button.into()
                    } else {
                        generate_folders_button
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Explorer(
                            crate::app::features::workspace::explorer::Message::GenerateFolders,
                        ),
                    ))
                    .into()
                    };

                let import_folders_button = button(
                    container(
                        self.button_text("Import Folders")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
                let import_folders_button: Element<'_, Message> =
                    if self.workspace.explorer.folder_generation_running {
                        import_folders_button.into()
                    } else {
                        import_folders_button
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Explorer(
                                crate::app::features::workspace::explorer::Message::ImportFolders,
                            ),
                        ))
                        .into()
                    };

                let export_folders_button = button(
                    container(
                        self.button_text("Export Folders")
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                )
                .padding(self.button_padding_tight())
                .width(Fill)
                .style(compact_button_style);
                let export_folders_button: Element<'_, Message> =
                    if self.workspace.explorer.folder_generation_running {
                        export_folders_button.into()
                    } else {
                        export_folders_button
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Explorer(
                                crate::app::features::workspace::explorer::Message::ExportFolders,
                            ),
                        ))
                        .into()
                    };

                container(
                    iced::widget::column![
                        make_folder,
                        collapse_folders,
                        expand_folders,
                        generate_folders_button,
                        import_folders_button,
                        export_folders_button
                    ]
                    .spacing(self.scale_u16(4)),
                )
                .padding(self.scale_u16(6))
                .width(Length::Fixed(self.scale_f32(220.0)))
                .style(panel_border_style)
                .into()
            } else {
                space::horizontal().into()
            };

        let sidebar_tools_dropdown = HeaderDropdown::new(
            sidebar_tools_button,
            sidebar_tools_panel,
            self.workspace.explorer.sidebar_tools_open,
            Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::CloseSidebarTools,
            )),
        );

        let history = self.history_view(layout);

        let table_list = scrollable(
            container(list)
                .padding(iced::padding::right(
                    self.scale_f32(SIDEBAR_LIST_RIGHT_PADDING),
                ))
                .width(Fill),
        )
        .id(sidebar_tables_scroll_id())
        .height(Fill)
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::new()
                .width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                .scroller_width(self.scale_f32(SIDEBAR_SCROLLBAR_WIDTH))
                .spacing(self.scale_f32(SIDEBAR_SCROLLBAR_GUTTER)),
        ));
        let table_list = mouse_area(table_list)
            .on_move(|position| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarCursorMoved(
                        position,
                    ),
                ))
            })
            .on_release(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarDragReleased,
                ),
            ));
        let table_list: Element<'_, Message> = stack![
            table_list,
            responsive(move |size| self.sidebar_context_menu_overlay(size))
        ]
        .into();

        let sidebar_title: Element<'_, Message> =
            if self.settings.values.multiple_connections_layout {
                self.database_dropdown(self.scale_f32(190.0), true)
            } else {
                self.label_text(
                    if self.connections.current.driver == DatabaseDriver::PostgreSql {
                        let total = if self.workspace.explorer.postgres_sidebar_objects.is_empty() {
                            self.workspace.explorer.tables.len()
                        } else {
                            self.workspace.explorer.postgres_sidebar_objects.len()
                        };
                        format!("{} ({total})", crate::i18n::tr("Objects"))
                    } else {
                        format!(
                            "{} ({})",
                            crate::i18n::tr("Tables"),
                            self.workspace.explorer.tables.len()
                        )
                    },
                )
                .into()
            };

        let mut content = if self.modern() {
            iced::widget::column![
                container(
                    row![
                        container(self.modern_sidebar_identity())
                            .width(Fill)
                            .height(Fill),
                        refresh_button,
                        sidebar_tools_dropdown,
                    ]
                    .spacing(0)
                    .height(Fill),
                )
                .height(Length::Fixed(self.scale_f32(MODERN_QUERY_STRIP_HEIGHT)))
                .width(Fill),
                self.horizontal_hairline(),
                self.modern_filter_row(sidebar_filter_dropdown),
                self.horizontal_hairline(),
                table_list,
            ]
            .spacing(0)
            .height(Fill)
        } else {
            iced::widget::column![
                row![
                    sidebar_title,
                    space::horizontal(),
                    refresh_button,
                    sidebar_tools_dropdown,
                ]
                .spacing(self.scale_u16(4))
                .align_y(Center),
                container(
                    row![search_input, sidebar_filter_dropdown]
                        .spacing(self.scale_u16(4))
                        .align_y(Center),
                )
                .padding(iced::padding::right(
                    self.scale_f32(SIDEBAR_LIST_RIGHT_PADDING),
                ))
                .width(Fill),
                table_list,
            ]
            .spacing(self.scale_u16(6))
            .height(Fill)
        };

        if let Some(error) = &self.connections.table_error {
            content =
                content.push(self.label_text(format!("{}: {}", crate::i18n::tr("Error"), error)));
        }

        content = content.push(history);

        let width = match layout {
            LayoutMode::Wide => Fill,
            LayoutMode::Compact => Fill,
        };

        let height = Fill;

        container(content)
            .padding(if self.modern() {
                0.0
            } else {
                self.scale_u16(6)
            })
            .width(width)
            .height(height)
            .style(panel_style)
            .into()
    }

    pub(crate) fn sidebar_table_entry(
        &self,
        table: &str,
        layout: LayoutMode,
        child_max_chars: usize,
    ) -> Element<'_, Message> {
        let is_selected = self
            .workspace
            .selected_table
            .as_deref()
            .is_some_and(|selected| selected == table);

        let max_chars = self.sidebar_table_max_chars(layout);
        let (display, truncated) = truncate_with_ellipsis(table, max_chars);

        let table_label_content: Element<'_, Message> =
            if self.connections.current.driver == DatabaseDriver::PostgreSql {
                let icon = self
                    .postgres_table_icon_for(table)
                    .unwrap_or(ICON_POSTGRES_TABLE);
                row![
                    self.icon_text(icon).size(self.sidebar_icon_size()),
                    self.button_text(display)
                        .wrapping(text::Wrapping::None)
                        .width(Fill),
                ]
                .spacing(self.scale_u16(4))
                .align_y(Center)
                .into()
            } else if self.modern() {
                let tokens = crate::ui::theme::tokens(&self.theme());
                row![
                    self.icon_text(ICON_MODERN_TABLE)
                        .size(self.sidebar_icon_size())
                        .color(if is_selected {
                            tokens.accent
                        } else {
                            tokens.ghost
                        }),
                    container(
                        self.button_text(display)
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true),
                    match self.compact_row_count(table) {
                        Some(count) => Element::from(
                            container(
                                text(count)
                                    .font(self.ui_font())
                                    .size(self.scale_f32(11.0))
                                    .color(tokens.ghost)
                                    .wrapping(text::Wrapping::None),
                            )
                            .align_x(iced::alignment::Horizontal::Right)
                            .width(Length::Fixed(self.scale_f32(38.0))),
                        ),
                        None => space::horizontal().width(Length::Fixed(0.0)).into(),
                    },
                ]
                .spacing(self.scale_u16(6))
                .align_y(Center)
                .into()
            } else {
                self.button_text(display)
                    .wrapping(text::Wrapping::None)
                    .width(Fill)
                    .into()
            };

        let table_label = container(table_label_content).width(Fill).clip(true);

        let table_button = button(table_label)
            .width(Fill)
            .padding(self.sidebar_button_padding())
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarTablePressed(
                        table.to_string(),
                    ),
                ),
            ))
            .style(move |theme, status| sidebar_table_button_style(is_selected, theme, status));

        let entry: Element<'_, Message> = if truncated {
            self.with_tooltip(table_button, table.to_string())
        } else {
            table_button.into()
        };

        let entry: Element<'_, Message> = if self.modern() {
            row![self.inset_bar(is_selected, true), entry]
                .spacing(0)
                .align_y(Center)
                .into()
        } else {
            entry
        };

        let table_name = table.to_string();
        let drag_table_name = table.to_string();
        let entry = mouse_area(entry)
            .on_right_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::TableContextMenuRequested {
                        table: table_name,
                    },
                ),
            ))
            .on_move(move |_| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarTableDragMoved {
                        table: drag_table_name.clone(),
                    },
                ))
            })
            .on_release(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarDragReleased,
                ),
            ));

        let mut entry_column = iced::widget::column![entry].spacing(self.sidebar_list_spacing());

        let show_triggers = is_selected
            && self.workspace.explorer.sidebar_filter_triggers
            && self
                .workspace
                .explorer
                .triggers_table
                .as_deref()
                .is_some_and(|name| name == table)
            && !self.workspace.explorer.table_triggers.is_empty();

        let show_relations = is_selected
            && self.workspace.explorer.sidebar_filter_relationships
            && self
                .workspace
                .explorer
                .relations_table
                .as_deref()
                .is_some_and(|name| name == table)
            && (self.connections.current.driver == DatabaseDriver::PostgreSql
                || !self.workspace.explorer.table_relations.is_empty());

        if show_triggers || show_relations {
            let mut sections = iced::widget::column![].spacing(self.scale_u16(4));

            if show_triggers {
                let mut triggers = iced::widget::column![].spacing(self.scale_u16(2));

                for trigger in &self.workspace.explorer.table_triggers {
                    let is_trigger_selected = self
                        .workspace
                        .explorer
                        .selected_trigger
                        .as_deref()
                        .is_some_and(|selected| selected == trigger.name.as_str());
                    let (display, truncated) =
                        truncate_with_ellipsis(&trigger.name, child_max_chars);

                    let trigger_label = container(
                        self.label_text(display)
                            .wrapping(text::Wrapping::None)
                            .width(Fill),
                    )
                    .width(Fill)
                    .clip(true);

                    let trigger_button = button(trigger_label)
                        .width(Fill)
                        .padding(self.sidebar_button_padding_tight())
                        .style(move |theme, status| {
                            if is_trigger_selected {
                                compact_tab_active_button_style(theme, status)
                            } else {
                                compact_button_style(theme, status)
                            }
                        })
                        .on_press(Message::Workspace(
                            crate::app::features::workspace::Message::Explorer(
                                crate::app::features::workspace::explorer::Message::TriggerSelected(
                                    trigger.name.clone(),
                                ),
                            ),
                        ));

                    let trigger_entry: Element<'_, Message> = if truncated {
                        self.with_tooltip(trigger_button, trigger.name.clone())
                    } else {
                        trigger_button.into()
                    };

                    triggers = triggers.push(trigger_entry);
                }

                let triggers_header = row![
                    self.icon_text(ICON_TRIGGER).size(self.sidebar_icon_size()),
                    self.label_text("Triggers")
                ]
                .spacing(self.scale_u16(3))
                .align_y(Center);

                let triggers_section = container(
                    iced::widget::column![triggers_header, triggers].spacing(self.scale_u16(3)),
                )
                .padding(self.scale_u16(4))
                .width(Fill)
                .style(panel_border_style);

                sections = sections.push(triggers_section);
            }

            if show_relations {
                let relations_active = self.is_relations_view_active();
                let relations_label = row![
                    self.icon_text(ICON_RELATIONS)
                        .size(self.sidebar_icon_size()),
                    self.label_text("Relations")
                ]
                .spacing(self.scale_u16(3))
                .align_y(Center);

                let relations_button = button(container(relations_label).width(Fill))
                    .width(Fill)
                    .padding(self.sidebar_button_padding_tight())
                    .style(move |theme, status| {
                        if relations_active {
                            compact_tab_active_button_style(theme, status)
                        } else {
                            compact_button_style(theme, status)
                        }
                    })
                    .on_press(Message::Workspace(
                        crate::app::features::workspace::Message::Query(
                            crate::app::features::workspace::query::Message::RelationsSelected(
                                table.to_string(),
                            ),
                        ),
                    ));

                sections = sections.push(relations_button);
            }

            entry_column = entry_column.push(
                container(sections)
                    .padding(iced::padding::left(self.scale_f32(SIDEBAR_TRIGGER_INDENT)))
                    .width(Fill),
            );
        }

        entry_column.into()
    }

    pub(crate) fn sidebar_relation_entry(
        &self,
        entry: &SidebarRelationEntry,
        child_max_chars: usize,
    ) -> Element<'_, Message> {
        let column = if entry.relation.column.trim().is_empty() {
            String::from("?")
        } else {
            entry.relation.column.clone()
        };
        let referenced_column = if entry.relation.referenced_column.trim().is_empty() {
            String::from("?")
        } else {
            entry.relation.referenced_column.clone()
        };
        let label_text = format!(
            "{}.{} -> {}.{}",
            entry.table, column, entry.relation.referenced_table, referenced_column
        );
        let is_selected = self.is_relations_view_active()
            && self.workspace.selected_table.as_deref() == Some(entry.table.as_str());
        let (display, truncated) = truncate_with_ellipsis(&label_text, child_max_chars);

        let label = container(
            row![
                self.icon_text(ICON_RELATIONS)
                    .size(self.sidebar_icon_size()),
                self.label_text(display)
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            ]
            .spacing(self.scale_u16(3))
            .align_y(Center),
        )
        .width(Fill)
        .clip(true);

        let button = button(label)
            .width(Fill)
            .padding(self.sidebar_button_padding_tight())
            .style(move |theme, status| {
                if is_selected {
                    compact_tab_active_button_style(theme, status)
                } else {
                    compact_button_style(theme, status)
                }
            })
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarRelationPressed {
                        table: entry.table.clone(),
                    },
                ),
            ));

        if truncated {
            self.with_tooltip(button, label_text)
        } else {
            button.into()
        }
    }

    pub(crate) fn sidebar_trigger_entry(
        &self,
        entry: &SidebarTriggerEntry,
        child_max_chars: usize,
    ) -> Element<'_, Message> {
        let label_text = format!("{} ({})", entry.trigger.name, entry.table);
        let is_selected = self.workspace.selected_table.as_deref() == Some(entry.table.as_str())
            && self.workspace.explorer.selected_trigger.as_deref()
                == Some(entry.trigger.name.as_str());
        let (display, truncated) = truncate_with_ellipsis(&label_text, child_max_chars);

        let label = container(
            row![
                self.icon_text(ICON_TRIGGER).size(self.sidebar_icon_size()),
                self.label_text(display)
                    .wrapping(text::Wrapping::None)
                    .width(Fill),
            ]
            .spacing(self.scale_u16(3))
            .align_y(Center),
        )
        .width(Fill)
        .clip(true);

        let button = button(label)
            .width(Fill)
            .padding(self.button_padding_tight())
            .style(move |theme, status| {
                if is_selected {
                    compact_tab_active_button_style(theme, status)
                } else {
                    compact_button_style(theme, status)
                }
            })
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::SidebarTriggerPressed {
                        table: entry.table.clone(),
                        trigger: entry.trigger.clone(),
                    },
                ),
            ));

        if truncated {
            self.with_tooltip(button, label_text)
        } else {
            button.into()
        }
    }

    pub(crate) fn context_menu_height(&self, items: usize) -> f32 {
        self.presentation().context_menu_height(items)
    }

    pub(crate) fn sidebar_tree_wrap<'a>(
        &'a self,
        depth: usize,
        content: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if depth == 0 {
            return content;
        }

        let indent = self.scale_f32(SIDEBAR_TRIGGER_INDENT) * depth as f32;
        container(content)
            .padding(iced::padding::left(indent))
            .width(Fill)
            .into()
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
        self.presentation().context_menu_padding(
            position,
            bounds,
            menu_width,
            menu_height,
            offset_x,
            offset_y,
        )
    }

    pub(crate) fn sidebar_context_menu_overlay(&self, bounds: Size) -> Element<'_, Message> {
        if self.workspace.explorer.folder_context_menu.is_some() {
            self.folder_context_menu_overlay(bounds)
        } else if self
            .workspace
            .explorer
            .postgres_object_context_menu
            .is_some()
        {
            self.postgres_object_context_menu_overlay(bounds)
        } else if self.workspace.explorer.table_context_menu.is_some() {
            self.table_context_menu_overlay(bounds)
        } else {
            container(space::horizontal()).into()
        }
    }

    pub(crate) fn folder_context_menu_overlay(&self, bounds: Size) -> Element<'_, Message> {
        self.workspace
            .explorer
            .folder_context_menu_overlay(self.presentation(), bounds)
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn postgres_object_context_menu_overlay(
        &self,
        bounds: Size,
    ) -> Element<'_, Message> {
        let Some(state) = &self.workspace.explorer.postgres_object_context_menu else {
            return container(space::horizontal()).into();
        };

        if state.object.kind != PostgresObjectKind::Role {
            return container(space::horizontal()).into();
        }

        let create_button = button(self.label_text("Create"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::PostgresRoleContextCreate,
                ),
            ));
        let drop_button = button(self.label_text("Drop"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(
                crate::app::features::workspace::Message::Explorer(
                    crate::app::features::workspace::explorer::Message::PostgresRoleContextDrop,
                ),
            ));
        let create_script_button = button(self.label_text("Create Script"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::PostgresRoleContextCreateScript,
            )));
        let properties_button = button(self.label_text("Properties"))
            .padding(self.button_padding_tight())
            .width(Fill)
            .style(compact_button_style)
            .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::PostgresRoleContextProperties,
            )));

        let menu_width = self.scale_f32(240.0);
        let menu_height = self.context_menu_height(4);
        let menu = iced::widget::column![
            create_button,
            drop_button,
            create_script_button,
            properties_button
        ]
        .spacing(self.scale_u16(2))
        .width(Length::Fixed(menu_width));

        let panel = container(menu)
            .padding(self.scale_u16(4))
            .style(panel_border_style);
        let padding = self.context_menu_padding(
            state.position,
            bounds,
            menu_width,
            menu_height,
            self.scale_f32(4.0),
            self.scale_f32(12.0),
        );
        let menu_layer = container(panel)
            .padding(padding)
            .width(Fill)
            .height(Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top);
        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::ClosePostgresObjectContextMenu,
            )))
            .on_right_press(Message::Workspace(crate::app::features::workspace::Message::Explorer(
                crate::app::features::workspace::explorer::Message::ClosePostgresObjectContextMenu,
            )));

        stack![backdrop, menu_layer].width(Fill).height(Fill).into()
    }

    pub(crate) fn table_context_menu_overlay(&self, bounds: Size) -> Element<'_, Message> {
        self.workspace
            .explorer
            .table_context_menu_overlay(
                self.presentation(),
                bounds,
                self.connections.current.driver,
                self.chat_available(),
            )
            .map(|value| {
                Message::Workspace(crate::app::features::workspace::Message::Explorer(value))
            })
    }

    pub(crate) fn postgres_role_modal_view(&self) -> Element<'_, Message> {
        crate::app::features::workspace::explorer::roles::view::View {
            state: &self.workspace.explorer,
            presentation: self.presentation(),
            layout_mode: self.shell.layout_mode,
            theme_choice: self.settings.theme_choice,
            spinner: self.loading_spinner_icon(),
            is_generating_ai: self.ai.is_generating_ai,
        }
        .postgres_role_modal_view()
        .map(|value| Message::Workspace(crate::app::features::workspace::Message::Explorer(value)))
    }

    pub(crate) fn table_modal_view(&self) -> Element<'_, Message> {
        let columns = match self.workspace.explorer.table_modal.as_ref() {
            Some(TableModalState::AddColumn { table, .. }) => self.cached_table_columns(table),
            _ => &[],
        };
        crate::app::features::workspace::explorer::forms::View {
            state: &self.workspace.explorer,
            presentation: self.presentation(),
            driver: self.connections.current.driver,
            columns,
            layout_mode: self.shell.layout_mode,
            window_size: self.shell.window_size,
        }
        .table_modal_view()
        .map(|value| Message::Workspace(crate::app::features::workspace::Message::Explorer(value)))
    }
}
