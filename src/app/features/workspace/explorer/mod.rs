use crate::app::types::ErrorModalState;
use crate::app::types::ToastLevel;
use crate::db::DatabasePool;
use crate::db::metadata::{fetch_sidebar_relations, fetch_sidebar_triggers, update_mysql_trigger};
use crate::model::add_column::{AddColumnField, AddColumnFlag, ColumnDefaultKind, ColumnPosition};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::PickerOption;
use crate::model::table::PostgresObjectKind;
use crate::model::table::TableCommand;
use crate::model::table::{
    PostgresRoleForm, PostgresRoleModalTab, PostgresRoleTextField, PostgresRoleToggleField,
};
use crate::ui::widgets::code_snippet;
use crate::ui::widgets::history_input::Edit;
pub(crate) use commands::TableAction;
use iced::Task;
use iced::widget::text_editor;
use iced_code_editor::Message as CodeEditorMessage;
use std::path::PathBuf;
mod cache;
mod commands;
mod filters;
mod folder_generation;
mod folder_io;
mod folders;
mod form_update;
pub(crate) mod forms;
mod generated_folders;
mod menus;
mod metadata;
pub(crate) mod roles;
mod schema;
mod view;
use crate::model::table::{
    FolderStore, GeneratedFolder, PostgresSidebarObject, RelationInfo, SidebarRelationEntry,
    SidebarTriggerEntry, TableFolder, TableInfoDetails, TableModalState, TriggerInfo,
};
pub(crate) use filters::SidebarFilterKind;
use iced::Point;
use roles::PostgresRoleModalState;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub(crate) struct TableContextMenuState {
    pub(crate) position: Point,
    pub(crate) table: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FolderContextMenuState {
    pub(crate) position: Point,
    pub(crate) folder: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PostgresObjectContextMenuState {
    pub(crate) position: Point,
    pub(crate) object: PostgresSidebarObject,
}

pub(crate) struct State {
    pub(crate) tables: Vec<String>,
    pub(crate) postgres_sidebar_objects: Vec<PostgresSidebarObject>,
    pub(crate) postgres_schema_open: HashMap<String, bool>,
    pub(crate) postgres_schema_kind_open: HashMap<String, bool>,
    pub(crate) table_row_counts: std::collections::HashMap<String, u64>,
    pub(crate) selected_postgres_object: Option<PostgresSidebarObject>,
    pub(crate) table_triggers: Vec<TriggerInfo>,
    pub(crate) sidebar_triggers: Vec<SidebarTriggerEntry>,
    pub(crate) table_triggers_cache: HashMap<String, Vec<TriggerInfo>>,
    pub(crate) triggers_table: Option<String>,
    pub(crate) selected_trigger: Option<String>,
    pub(crate) trigger_edit_name: String,
    pub(crate) trigger_edit_timing: String,
    pub(crate) trigger_edit_event: String,
    pub(crate) trigger_edit_definer_user: String,
    pub(crate) trigger_edit_definer_host: String,
    pub(crate) table_relations: Vec<RelationInfo>,
    pub(crate) sidebar_relations: Vec<SidebarRelationEntry>,
    pub(crate) table_relations_cache: HashMap<String, Vec<RelationInfo>>,
    pub(crate) relations_table: Option<String>,
    pub(crate) table_search: String,
    pub(crate) sidebar_filter_tables: bool,
    pub(crate) sidebar_filter_views: bool,
    pub(crate) sidebar_filter_materialized_views: bool,
    pub(crate) sidebar_filter_foreign_tables: bool,
    pub(crate) sidebar_filter_sequences: bool,
    pub(crate) sidebar_filter_extensions: bool,
    pub(crate) sidebar_filter_login_group_roles: bool,
    pub(crate) sidebar_filter_relationships: bool,
    pub(crate) sidebar_filter_triggers: bool,
    pub(crate) sidebar_table_cursor: Option<Point>,
    pub(crate) table_context_menu: Option<TableContextMenuState>,
    pub(crate) postgres_object_context_menu: Option<PostgresObjectContextMenuState>,
    pub(crate) folder_context_menu: Option<FolderContextMenuState>,
    pub(crate) sidebar_filter_open: bool,
    pub(crate) sidebar_tools_open: bool,
    pub(crate) table_folders: Vec<TableFolder>,
    pub(crate) table_folder_map: HashMap<String, String>,
    pub(crate) sidebar_drag_table: Option<String>,
    pub(crate) sidebar_drag_active: bool,
    pub(crate) sidebar_drop_folder: Option<String>,
    pub(crate) table_modal: Option<TableModalState>,
    pub(crate) postgres_role_modal: Option<PostgresRoleModalState>,
    pub(crate) table_action_running: bool,
    pub(crate) postgres_role_action_running: bool,
    pub(crate) folder_generation_running: bool,
    pub(crate) pending_generated_folders: Vec<GeneratedFolder>,
    pub(crate) folder_store: FolderStore,
    pub(crate) folder_store_error: Option<String>,
    pub(crate) table_ddl: Option<TableDdlState>,
    pub(crate) table_info_sidebar_open: bool,
    pub(crate) table_info_loading: bool,
    pub(crate) table_info_error: Option<String>,
    pub(crate) table_info_table: Option<String>,
    pub(crate) table_info: Option<TableInfoDetails>,
    pub(crate) table_info_cache: HashMap<String, (TableInfoDetails, std::time::Instant)>,
    pub(crate) table_info_cache_lru: VecDeque<String>,
}
impl State {
    pub(crate) fn new(folder_store: FolderStore, folder_store_error: Option<String>) -> Self {
        Self {
            tables: Vec::new(),
            postgres_sidebar_objects: Vec::new(),
            postgres_schema_open: HashMap::new(),
            postgres_schema_kind_open: HashMap::new(),
            table_row_counts: std::collections::HashMap::new(),
            selected_postgres_object: None,
            table_triggers: Vec::new(),
            sidebar_triggers: Vec::new(),
            table_triggers_cache: HashMap::new(),
            triggers_table: None,
            selected_trigger: None,
            trigger_edit_name: String::new(),
            trigger_edit_timing: String::new(),
            trigger_edit_event: String::new(),
            trigger_edit_definer_user: String::new(),
            trigger_edit_definer_host: String::new(),
            table_relations: Vec::new(),
            sidebar_relations: Vec::new(),
            table_relations_cache: HashMap::new(),
            relations_table: None,
            table_search: String::new(),
            sidebar_filter_tables: true,
            sidebar_filter_views: true,
            sidebar_filter_materialized_views: true,
            sidebar_filter_foreign_tables: true,
            sidebar_filter_sequences: true,
            sidebar_filter_extensions: true,
            sidebar_filter_login_group_roles: true,
            sidebar_filter_relationships: true,
            sidebar_filter_triggers: true,
            sidebar_table_cursor: None,
            table_context_menu: None,
            postgres_object_context_menu: None,
            folder_context_menu: None,
            sidebar_filter_open: false,
            sidebar_tools_open: false,
            table_folders: Vec::new(),
            table_folder_map: HashMap::new(),
            sidebar_drag_table: None,
            sidebar_drag_active: false,
            sidebar_drop_folder: None,
            table_modal: None,
            postgres_role_modal: None,
            table_action_running: false,
            postgres_role_action_running: false,
            folder_generation_running: false,
            pending_generated_folders: Vec::new(),
            folder_store,
            folder_store_error,
            table_ddl: None,
            table_info_sidebar_open: false,
            table_info_loading: false,
            table_info_error: None,
            table_info_table: None,
            table_info: None,
            table_info_cache: HashMap::new(),
            table_info_cache_lru: VecDeque::new(),
        }
    }
}

pub(crate) struct TableDdlState {
    pub(crate) table: String,
    pub(crate) sql: Option<String>,
    pub(crate) editor: Option<iced_code_editor::CodeEditor>,
    pub(crate) error: Option<String>,
}

impl TableDdlState {
    pub(crate) fn loading(table: String) -> Self {
        Self {
            table,
            sql: None,
            editor: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    SidebarTablePressed(String),
    SidebarPostgresObjectPressed(PostgresSidebarObject),
    SidebarTriggerPressed {
        table: String,
        trigger: TriggerInfo,
    },
    SidebarRelationPressed {
        table: String,
    },
    TriggerSelected(String),
    TriggerEditNameChanged(String),
    TriggerEditTimingChanged(String),
    TriggerEditEventChanged(String),
    TriggerEditDefinerUserChanged(String),
    TriggerEditDefinerHostChanged(String),
    UpdateTrigger,
    TriggerUpdated {
        table: String,
        result: Result<(), String>,
    },
    SidebarTableDragMoved {
        table: String,
    },
    SidebarDragReleased,
    SidebarCursorMoved(Point),
    TableContextMenuRequested {
        table: String,
    },
    FolderContextMenuRequested {
        folder: String,
    },
    ClosePostgresObjectContextMenu,
    SidebarPostgresObjectContextMenuRequested(PostgresSidebarObject),
    TableSearchChanged(String),

    PostgresRoleContextCreate,
    PostgresRoleContextDrop,
    PostgresRoleContextCreateScript,
    PostgresRoleContextProperties,
    PostgresRolePropertiesLoaded {
        role: String,
        result: Result<PostgresRoleForm, String>,
    },
    PostgresRoleScriptLoaded {
        role: String,
        result: Result<PostgresRoleForm, String>,
    },
    PostgresRoleDropFinished {
        role: String,
        result: Result<(), String>,
    },
    PostgresRoleModalTabSelected(PostgresRoleModalTab),
    PostgresRoleModalTextChanged {
        field: PostgresRoleTextField,
        value: String,
    },
    PostgresRoleModalToggleChanged {
        field: PostgresRoleToggleField,
        enabled: bool,
    },
    PostgresRoleModalSqlAction(text_editor::Action),
    PostgresRoleModalReset,
    PostgresRoleModalGenerateSqlWithAi,
    PostgresRoleModalSave,
    PostgresRoleModalSaved(Result<(), String>),
    ClosePostgresRoleModal,

    CloseTableContextMenu,
    CloseFolderContextMenu,
    TableContextAskAi,

    TableContextAction(TableAction),
    TableCommandFinished {
        command: TableCommand,
        result: Result<usize, String>,
    },

    SubmitTableModal,
    TextEdited(Edit<Message>),
    TableModalPrimaryChanged(String),
    AddColumnFieldChanged {
        field: AddColumnField,
        value: String,
    },
    AddColumnFlagChanged {
        flag: AddColumnFlag,
        enabled: bool,
    },
    AddColumnDefaultKindSelected(ColumnDefaultKind),
    AddColumnPositionSelected(ColumnPosition),
    TableModalFolderSelected(PickerOption<String>),
    TableModalOpenByDefaultToggled(bool),
    TableModalPinToTopToggled(bool),
    CloseTableModal,

    ModalBlocked,
    TableDdlLoaded {
        table: String,
        result: Result<String, String>,
    },
    CopyTableDdl,
    TableDdlAction(CodeEditorMessage),
    CloseTableDdl,

    CloseTableInfoSidebar,
    TableTriggersLoaded {
        table: String,
        result: Result<Vec<TriggerInfo>, String>,
    },
    TableRelationsLoaded {
        table: String,
        result: Result<Vec<RelationInfo>, String>,
    },
    TableInfoLoaded {
        table: String,
        result: Result<TableInfoDetails, String>,
    },

    PostgresSidebarObjectsLoaded {
        database: String,
        result: Result<Vec<PostgresSidebarObject>, String>,
    },

    TogglePostgresSchemaOpen(String),
    TogglePostgresSchemaKindOpen {
        schema: String,
        kind: PostgresObjectKind,
    },

    GenerateFolders,
    FolderGenerationFinished(Result<Vec<GeneratedFolder>, String>),
    ImportFolders,
    ExportFolders,
    FoldersImportFilePicked(Option<PathBuf>),
    FoldersExportFilePicked(Option<PathBuf>),

    DissolveFolderGroup,
    ToggleFolderPinFromMenu,
    RenameFolderFromMenu,
    OpenMakeFolderModal,

    ToggleFolderOpen(String),
    CollapseAllFolders,
    ExpandAllFolders,

    SidebarTriggersLoaded(Result<Vec<SidebarTriggerEntry>, String>),
    SidebarRelationsLoaded(Result<Vec<SidebarRelationEntry>, String>),

    Tooltip(Option<String>),
    ToggleSidebarTools,
    CloseSidebarTools,
    ToggleSidebarFilter,
    CloseSidebarFilter,
    SidebarFilterToggled {
        filter: SidebarFilterKind,
        enabled: bool,
    },
}

pub(crate) enum Output {
    SelectTable(String),
    OpenPostgresObject(PostgresSidebarObject),
    SelectTrigger {
        table: String,
        name: String,
    },
    SelectRelations(String),
    ShowTrigger {
        table: String,
        trigger: TriggerInfo,
    },
    TriggerUpdateFinished {
        table: String,
        result: Result<(), String>,
    },
    CloseTabContextMenu,
    Toast(ToastLevel, String),
    RoleChanged(String),
    RoleSqlStarted(String),
    RoleAi {
        prompt: String,
        sql: String,
    },
    RoleScript {
        notice: Option<String>,
        result: Result<String, String>,
    },

    AskAboutTable(String),
    LoadColumns(String),
    TableChanged {
        command: TableCommand,
        message: String,
    },
    RenameQueryTab {
        index: usize,
        name: String,
    },
    TextEdited(Edit<Message>),
    DdlCopied,
    ObjectsChanged,
    ApplyStatus {
        error: Option<String>,
        message: Option<String>,
    },
    Error(Option<ErrorModalState>),
    FolderFileSuccess(String),
    FolderRemoved(String),
    Tooltip(Option<String>),
}

impl State {
    fn close_sidebar_overlays(&mut self) {
        self.sidebar_tools_open = false;
        self.sidebar_filter_open = false;
        self.table_context_menu = None;
        self.postgres_object_context_menu = None;
        self.folder_context_menu = None;
    }

    pub(crate) fn can_update_selected_trigger(
        &self,
        driver: DatabaseDriver,
        query_running: bool,
        connected: bool,
        query: &str,
    ) -> bool {
        matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb)
            && !query_running
            && !self.table_action_running
            && connected
            && self.selected_trigger.is_some()
            && self.triggers_table.is_some()
            && !self.trigger_edit_name.trim().is_empty()
            && !self.trigger_edit_timing.trim().is_empty()
            && !self.trigger_edit_event.trim().is_empty()
            && !query.trim().is_empty()
    }

    pub(crate) fn update(
        &mut self,
        message: Message,
        context: Context<'_>,
    ) -> (Task<Message>, Option<Output>) {
        match message {
            Message::SidebarTablePressed(table) => {
                self.close_sidebar_overlays();
                self.table_modal = None;
                self.sidebar_drag_table = Some(table.clone());
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                (Task::none(), Some(Output::SelectTable(table)))
            }
            Message::SidebarPostgresObjectPressed(object) => {
                self.close_sidebar_overlays();
                self.table_modal = None;
                self.sidebar_drag_table = None;
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                (Task::none(), Some(Output::OpenPostgresObject(object)))
            }
            Message::SidebarTriggerPressed { table, trigger } => {
                if !context.is_connected || !self.tables.iter().any(|name| name == &table) {
                    return (Task::none(), None);
                }
                self.close_sidebar_overlays();
                self.table_modal = None;
                self.sidebar_drag_table = None;
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                self.open_folder_for_table(context.driver, context.folder_keys, &table);

                let mut triggers = self
                    .table_triggers_cache
                    .get(&table)
                    .cloned()
                    .unwrap_or_default();
                if !triggers.iter().any(|entry| entry.name == trigger.name) {
                    triggers.push(trigger.clone());
                }
                self.table_triggers = triggers.clone();
                self.table_triggers_cache.insert(table.clone(), triggers);
                self.triggers_table = Some(table.clone());
                self.update_sidebar_triggers_for_table(&table, self.table_triggers.clone());
                (
                    Task::none(),
                    Some(Output::SelectTrigger {
                        table,
                        name: trigger.name,
                    }),
                )
            }
            Message::SidebarRelationPressed { table } => {
                if !context.is_connected
                    || context.query_running
                    || !self.tables.iter().any(|name| name == &table)
                {
                    return (Task::none(), None);
                }
                self.close_sidebar_overlays();
                self.table_modal = None;
                self.sidebar_drag_table = None;
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                self.open_folder_for_table(context.driver, context.folder_keys, &table);
                self.selected_trigger = None;
                if let Some(relations) = self.table_relations_cache.get(&table).cloned() {
                    self.table_relations = relations;
                    self.relations_table = Some(table.clone());
                }
                (Task::none(), Some(Output::SelectRelations(table)))
            }
            Message::TriggerSelected(name) => {
                let Some(table) = context.selected_table else {
                    return (Task::none(), None);
                };
                let Some(trigger) = self
                    .table_triggers
                    .iter()
                    .find(|trigger| trigger.name == name)
                    .cloned()
                else {
                    return (Task::none(), None);
                };
                if matches!(
                    context.driver,
                    DatabaseDriver::MySql | DatabaseDriver::MariaDb
                ) {
                    let (user, host) = trigger_definer_parts(&trigger.definer);
                    self.trigger_edit_name = trigger.name.clone();
                    self.trigger_edit_timing = trigger.timing.clone();
                    self.trigger_edit_event = trigger.event.clone();
                    self.trigger_edit_definer_user = user;
                    self.trigger_edit_definer_host = host;
                }
                self.selected_trigger = Some(name);
                (
                    Task::none(),
                    Some(Output::ShowTrigger {
                        table: table.to_string(),
                        trigger,
                    }),
                )
            }
            Message::TriggerEditNameChanged(value) => {
                self.trigger_edit_name = value;
                (Task::none(), None)
            }
            Message::TriggerEditTimingChanged(value) => {
                self.trigger_edit_timing = value;
                (Task::none(), None)
            }
            Message::TriggerEditEventChanged(value) => {
                self.trigger_edit_event = value;
                (Task::none(), None)
            }
            Message::TriggerEditDefinerUserChanged(value) => {
                self.trigger_edit_definer_user = value;
                (Task::none(), None)
            }
            Message::TriggerEditDefinerHostChanged(value) => {
                self.trigger_edit_definer_host = value;
                (Task::none(), None)
            }
            Message::UpdateTrigger => {
                if !self.can_update_selected_trigger(
                    context.driver,
                    context.query_running,
                    context.pool.is_some(),
                    &context.query,
                ) {
                    return (Task::none(), None);
                }
                let (Some(pool), Some(table), Some(old_name)) = (
                    context.pool.cloned(),
                    self.triggers_table.clone(),
                    self.selected_trigger.clone(),
                ) else {
                    return (Task::none(), None);
                };
                let user = self.trigger_edit_definer_user.trim();
                let host = self.trigger_edit_definer_host.trim();
                let trigger = TriggerInfo {
                    name: self.trigger_edit_name.trim().to_string(),
                    timing: self.trigger_edit_timing.trim().to_ascii_uppercase(),
                    event: self.trigger_edit_event.trim().to_ascii_uppercase(),
                    statement: context.query.trim().to_string(),
                    definer: if user.is_empty() {
                        String::new()
                    } else {
                        format!("{}@{}", user, if host.is_empty() { "%" } else { host })
                    },
                };
                self.table_action_running = true;
                let database = context.database.unwrap_or_default();
                (
                    Task::perform(
                        update_mysql_trigger(pool, database, table.clone(), old_name, trigger),
                        move |result| Message::TriggerUpdated { table, result },
                    ),
                    Some(Output::ApplyStatus {
                        error: None,
                        message: None,
                    }),
                )
            }
            Message::TriggerUpdated { table, result } => {
                self.table_action_running = false;
                if result.is_ok() {
                    self.table_triggers_cache.remove(&table);
                }
                (
                    Task::none(),
                    Some(Output::TriggerUpdateFinished { table, result }),
                )
            }
            Message::SidebarTableDragMoved { table } => {
                if self
                    .sidebar_drag_table
                    .as_deref()
                    .is_some_and(|name| name == table.as_str())
                {
                    self.sidebar_drag_active = true;
                    self.sidebar_drop_folder = None;
                }
                (Task::none(), None)
            }
            Message::SidebarDragReleased => {
                if self.sidebar_drag_active
                    && let (Some(table), Some(folder)) = (
                        self.sidebar_drag_table.clone(),
                        self.sidebar_drop_folder.clone(),
                    )
                {
                    self.assign_table_to_folder(
                        context.driver,
                        context.folder_keys,
                        &table,
                        Some(folder),
                    );
                }
                self.sidebar_drag_table = None;
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                (Task::none(), None)
            }
            Message::SidebarCursorMoved(position) => {
                self.sidebar_table_cursor = Some(position);
                (Task::none(), None)
            }
            Message::TableContextMenuRequested { table } => {
                if !self.tables.iter().any(|name| name == &table) {
                    return (Task::none(), None);
                }
                self.sidebar_tools_open = false;
                self.sidebar_filter_open = false;
                self.postgres_object_context_menu = None;
                self.folder_context_menu = None;
                self.sidebar_drag_table = None;
                self.sidebar_drag_active = false;
                self.sidebar_drop_folder = None;
                let position = self.sidebar_table_cursor.unwrap_or(context.menu_fallback);
                self.table_context_menu = Some(TableContextMenuState { position, table });
                (Task::none(), Some(Output::CloseTabContextMenu))
            }
            Message::FolderContextMenuRequested { folder } => {
                if !self.folder_exists(&folder) {
                    return (Task::none(), None);
                }
                self.sidebar_tools_open = false;
                self.sidebar_filter_open = false;
                self.table_context_menu = None;
                self.postgres_object_context_menu = None;
                let position = self.sidebar_table_cursor.unwrap_or(context.menu_fallback);
                self.folder_context_menu = Some(FolderContextMenuState { position, folder });
                (Task::none(), Some(Output::CloseTabContextMenu))
            }
            Message::ClosePostgresObjectContextMenu => {
                self.postgres_object_context_menu = None;
                (Task::none(), None)
            }
            Message::SidebarPostgresObjectContextMenuRequested(object) => {
                if object.kind != PostgresObjectKind::Role {
                    return (Task::none(), None);
                }
                let exists = self.postgres_sidebar_objects.iter().any(|entry| {
                    entry.kind == object.kind
                        && entry.schema.eq_ignore_ascii_case(&object.schema)
                        && entry.name.eq_ignore_ascii_case(&object.name)
                });
                if !exists {
                    return (Task::none(), None);
                }
                self.sidebar_tools_open = false;
                self.sidebar_filter_open = false;
                self.table_context_menu = None;
                self.folder_context_menu = None;
                let position = self.sidebar_table_cursor.unwrap_or(context.menu_fallback);
                self.postgres_object_context_menu =
                    Some(PostgresObjectContextMenuState { position, object });
                (Task::none(), Some(Output::CloseTabContextMenu))
            }
            Message::TableSearchChanged(search) => {
                self.table_search = search;
                (Task::none(), None)
            }

            Message::PostgresRoleContextCreate => self.postgres_role_context_create(context),
            Message::PostgresRoleContextDrop => self.postgres_role_context_drop(context),
            Message::PostgresRoleContextCreateScript => {
                self.postgres_role_context_create_script(context)
            }
            Message::PostgresRoleContextProperties => {
                self.postgres_role_context_properties(context)
            }
            Message::PostgresRolePropertiesLoaded { role, result } => {
                self.postgres_role_properties_loaded(role, result, context)
            }
            Message::PostgresRoleScriptLoaded { role, result } => {
                self.postgres_role_script_loaded(role, result, context)
            }
            Message::PostgresRoleDropFinished { role, result } => {
                self.postgres_role_drop_finished(role, result, context)
            }
            Message::PostgresRoleModalTabSelected(tab) => {
                self.postgres_role_modal_tab_selected(tab, context)
            }
            Message::PostgresRoleModalTextChanged { field, value } => {
                self.postgres_role_modal_text_changed(field, value, context)
            }
            Message::PostgresRoleModalToggleChanged { field, enabled } => {
                self.postgres_role_modal_toggle_changed(field, enabled, context)
            }
            Message::PostgresRoleModalSqlAction(action) => {
                self.postgres_role_modal_sql_action(action, context)
            }
            Message::PostgresRoleModalReset => self.postgres_role_modal_reset(context),
            Message::PostgresRoleModalGenerateSqlWithAi => {
                self.postgres_role_modal_generate_sql_with_ai(context)
            }
            Message::PostgresRoleModalSave => self.postgres_role_modal_save(context),
            Message::PostgresRoleModalSaved(result) => {
                self.postgres_role_modal_saved(result, context)
            }
            Message::ClosePostgresRoleModal => self.close_postgres_role_modal(context),

            Message::CloseTableContextMenu => {
                self.table_context_menu = None;
                (Task::none(), None)
            }
            Message::CloseFolderContextMenu => {
                self.folder_context_menu = None;
                (Task::none(), None)
            }
            Message::TableContextAskAi => {
                let output = self
                    .table_context_menu
                    .take()
                    .map(|menu| Output::AskAboutTable(menu.table));
                (Task::none(), output)
            }

            Message::TableContextAction(action) => {
                (Task::none(), self.open_table_action(action, context.driver))
            }
            Message::TableCommandFinished { command, result } => {
                self.table_action_running = false;
                let output = match result {
                    Ok(changed) => Output::TableChanged {
                        message: Self::table_command_success_text(&command, changed),
                        command,
                    },
                    Err(error) => Output::Error(Some(ErrorModalState::new(
                        "Table action",
                        "Action failed.",
                        error,
                    ))),
                };
                (Task::none(), Some(output))
            }

            Message::SubmitTableModal => self.submit_table_modal(context),
            Message::TextEdited(edit) => (Task::none(), Some(Output::TextEdited(edit))),
            Message::TableModalPrimaryChanged(value) => {
                if let Some(modal) = self.table_modal.as_mut() {
                    match modal {
                        TableModalState::MakeFolder { name, .. } => {
                            *name = value;
                        }
                        TableModalState::AlterTable { clause, .. } => {
                            *clause = value;
                        }
                        TableModalState::DuplicateTable { new_name, .. } => {
                            *new_name = value;
                        }
                        TableModalState::RenameTable { new_name, .. } => {
                            *new_name = value;
                        }
                        TableModalState::RenameQueryTab { name, .. } => {
                            *name = value;
                        }

                        TableModalState::AddColumn { .. }
                        | TableModalState::MoveToFolder { .. }
                        | TableModalState::ConfirmFoldersImport { .. }
                        | TableModalState::Confirm { .. } => {}
                    }
                }
                (Task::none(), None)
            }
            Message::AddColumnFieldChanged { field, value } => {
                if let Some(TableModalState::AddColumn { draft, .. }) = self.table_modal.as_mut() {
                    draft.set_field(field, value);
                }
                (Task::none(), None)
            }
            Message::AddColumnFlagChanged { flag, enabled } => {
                if let Some(TableModalState::AddColumn { draft, .. }) = self.table_modal.as_mut() {
                    draft.set_flag(flag, enabled);
                }
                (Task::none(), None)
            }
            Message::AddColumnDefaultKindSelected(kind) => {
                if let Some(TableModalState::AddColumn { draft, .. }) = self.table_modal.as_mut() {
                    draft.default_kind = kind;
                }
                (Task::none(), None)
            }
            Message::AddColumnPositionSelected(position) => {
                if let Some(TableModalState::AddColumn { draft, .. }) = self.table_modal.as_mut() {
                    draft.position = position;
                }
                (Task::none(), None)
            }
            Message::TableModalFolderSelected(option) => {
                if let Some(TableModalState::MoveToFolder { folder, .. }) =
                    self.table_modal.as_mut()
                {
                    *folder = option.value;
                }
                (Task::none(), None)
            }
            Message::TableModalOpenByDefaultToggled(enabled) => {
                if let Some(TableModalState::MakeFolder {
                    open_by_default, ..
                }) = self.table_modal.as_mut()
                {
                    *open_by_default = enabled;
                }
                (Task::none(), None)
            }
            Message::TableModalPinToTopToggled(enabled) => {
                if let Some(TableModalState::MakeFolder { pin_to_top, .. }) =
                    self.table_modal.as_mut()
                {
                    *pin_to_top = enabled;
                }
                (Task::none(), None)
            }
            Message::CloseTableModal => {
                self.table_modal = None;
                (Task::none(), None)
            }

            Message::ModalBlocked => (Task::none(), None),
            Message::TableDdlLoaded { table, result } => {
                let editor = result.as_ref().ok().map(|sql| {
                    code_snippet::new(
                        sql,
                        &context.theme,
                        crate::ui::presentation::font_for_choice(&context.settings.editor_font),
                        crate::ui::presentation::Presentation {
                            settings: context.settings,
                        }
                        .input_text_size() as f32,
                    )
                });
                if let Some(state) = self.table_ddl.as_mut()
                    && state.table == table
                {
                    match result {
                        Ok(sql) => {
                            state.editor = editor;
                            state.sql = Some(sql);
                        }
                        Err(error) => state.error = Some(error),
                    }
                }
                (Task::none(), None)
            }
            Message::CopyTableDdl => {
                let Some(sql) = self.table_ddl.as_ref().and_then(|state| state.sql.clone()) else {
                    return (Task::none(), None);
                };
                (iced::clipboard::write(sql), Some(Output::DdlCopied))
            }
            Message::TableDdlAction(action) => {
                if !code_snippet::is_edit(&action)
                    && let Some(editor) = self.table_ddl.as_mut().and_then(|s| s.editor.as_mut())
                {
                    return (editor.update(&action).map(Message::TableDdlAction), None);
                }
                (Task::none(), None)
            }
            Message::CloseTableDdl => {
                self.table_ddl = None;
                (Task::none(), None)
            }

            Message::CloseTableInfoSidebar => {
                self.table_info_sidebar_open = false;
                self.table_info_loading = false;
                (Task::none(), None)
            }

            Message::TableTriggersLoaded { table, result } => {
                if !context.is_connected {
                    return (Task::none(), None);
                }
                match result {
                    Ok(triggers) => {
                        self.table_triggers_cache
                            .insert(table.clone(), triggers.clone());
                        self.update_sidebar_triggers_for_table(&table, triggers.clone());
                        if context.selected_table == Some(table.as_str()) {
                            self.table_triggers = triggers;
                            self.triggers_table = Some(table);
                        }
                    }
                    Err(_) => {
                        self.table_triggers_cache.remove(&table);
                        self.update_sidebar_triggers_for_table(&table, Vec::new());
                        if context.selected_table == Some(table.as_str()) {
                            self.table_triggers.clear();
                            self.triggers_table = Some(table);
                            self.selected_trigger = None;
                        }
                    }
                }
                (Task::none(), None)
            }
            Message::TableRelationsLoaded { table, result } => {
                if !context.is_connected {
                    return (Task::none(), None);
                }
                match result {
                    Ok(relations) => {
                        self.table_relations_cache
                            .insert(table.clone(), relations.clone());
                        self.update_sidebar_relations_for_table(&table, relations.clone());
                        if context.selected_table == Some(table.as_str()) {
                            self.table_relations = relations;
                            self.relations_table = Some(table);
                        }
                    }
                    Err(_) => {
                        self.table_relations_cache.remove(&table);
                        self.update_sidebar_relations_for_table(&table, Vec::new());
                        if context.selected_table == Some(table.as_str()) {
                            self.table_relations.clear();
                            self.relations_table = Some(table);
                        }
                    }
                }
                (Task::none(), None)
            }
            Message::TableInfoLoaded { table, result } => {
                if self.table_info_table.as_deref() != Some(table.as_str()) {
                    return (Task::none(), None);
                }
                self.table_info_loading = false;
                match result {
                    Ok(info) => {
                        self.insert_table_info_cache_entry(table, info.clone());
                        self.table_info = Some(info);
                        self.table_info_error = None;
                    }
                    Err(error) => {
                        self.table_info = None;
                        self.table_info_error = Some(error);
                    }
                }
                (Task::none(), None)
            }

            Message::PostgresSidebarObjectsLoaded { database, result } => {
                if !context.is_connected || context.driver != DatabaseDriver::PostgreSql {
                    return (Task::none(), None);
                }
                if context.database.as_deref().unwrap_or_default().trim() != database.trim() {
                    return (Task::none(), None);
                }

                match result {
                    Ok(objects) => {
                        self.postgres_sidebar_objects = objects;
                        self.sync_postgres_schema_open_state();
                        if let Some(selected) = self.selected_postgres_object.clone() {
                            let still_exists = self.postgres_sidebar_objects.iter().any(|object| {
                                object.kind == selected.kind
                                    && object.schema.eq_ignore_ascii_case(&selected.schema)
                                    && object.name.eq_ignore_ascii_case(&selected.name)
                            });
                            if !still_exists {
                                self.selected_postgres_object = None;
                            }
                        }
                    }
                    Err(_) => {
                        self.postgres_sidebar_objects.clear();
                        self.postgres_schema_open.clear();
                        self.postgres_schema_kind_open.clear();
                        self.selected_postgres_object = None;
                    }
                }

                (Task::none(), Some(Output::ObjectsChanged))
            }

            Message::TogglePostgresSchemaOpen(schema) => {
                self.toggle_postgres_schema_open(&schema);
                (Task::none(), None)
            }
            Message::TogglePostgresSchemaKindOpen { schema, kind } => {
                self.toggle_postgres_schema_kind_open(&schema, kind);
                (Task::none(), None)
            }

            Message::GenerateFolders => self.generate_folders(context),
            Message::FolderGenerationFinished(result) => {
                self.folder_generation_finished(result, context)
            }
            message @ (Message::ImportFolders
            | Message::ExportFolders
            | Message::FoldersImportFilePicked(_)
            | Message::FoldersExportFilePicked(_)) => self.folder_file_update(message, context),
            Message::DissolveFolderGroup => {
                let Some(folder_name) = self
                    .folder_context_menu
                    .as_ref()
                    .map(|menu| menu.folder.clone())
                else {
                    return (Task::none(), None);
                };
                self.folder_context_menu = None;
                if self.dissolve_folder(&folder_name, context.folder_keys) {
                    return (Task::none(), Some(Output::FolderRemoved(folder_name)));
                }
                (Task::none(), None)
            }
            Message::ToggleFolderPinFromMenu => {
                let Some(folder_name) = self
                    .folder_context_menu
                    .as_ref()
                    .map(|menu| menu.folder.clone())
                else {
                    return (Task::none(), None);
                };
                if let Some(folder) = self
                    .table_folders
                    .iter_mut()
                    .find(|folder| folder.name == folder_name)
                {
                    folder.pin_to_top = !folder.pin_to_top;
                    self.sort_folders();
                    self.persist_folder_store(context.folder_keys);
                }
                self.folder_context_menu = None;
                (Task::none(), None)
            }
            Message::RenameFolderFromMenu => {
                let Some(folder_name) = self
                    .folder_context_menu
                    .as_ref()
                    .map(|menu| menu.folder.clone())
                else {
                    return (Task::none(), None);
                };
                let Some(folder) = self
                    .table_folders
                    .iter()
                    .find(|folder| folder.name == folder_name)
                    .cloned()
                else {
                    self.folder_context_menu = None;
                    return (Task::none(), None);
                };
                self.folder_context_menu = None;
                self.table_modal = Some(TableModalState::MakeFolder {
                    name: folder.name.clone(),
                    open_by_default: folder.is_open,
                    pin_to_top: folder.pin_to_top,
                    source_folder: Some(folder.name),
                });
                (Task::none(), None)
            }
            Message::OpenMakeFolderModal => {
                self.sidebar_tools_open = false;
                self.table_context_menu = None;
                self.postgres_object_context_menu = None;
                self.folder_context_menu = None;
                self.table_modal = Some(TableModalState::MakeFolder {
                    name: String::new(),
                    open_by_default: true,
                    pin_to_top: false,
                    source_folder: None,
                });
                (Task::none(), None)
            }

            Message::ToggleFolderOpen(folder) => {
                if let Some(current) = self
                    .table_folders
                    .iter_mut()
                    .find(|entry| entry.name == folder)
                {
                    current.is_open = !current.is_open;
                    self.persist_folder_store(context.folder_keys);
                }
                (Task::none(), None)
            }
            Message::CollapseAllFolders => {
                for folder in &mut self.table_folders {
                    folder.is_open = false;
                }
                self.persist_folder_store(context.folder_keys);
                (Task::none(), None)
            }
            Message::ExpandAllFolders => {
                for folder in &mut self.table_folders {
                    folder.is_open = true;
                }
                self.persist_folder_store(context.folder_keys);
                (Task::none(), None)
            }

            Message::SidebarTriggersLoaded(result) => {
                if !context.is_connected {
                    return (Task::none(), None);
                }

                match result {
                    Ok(entries) => {
                        self.set_sidebar_trigger_entries(context.selected_table, entries)
                    }
                    Err(_) => {
                        self.sidebar_triggers.clear();
                        self.table_triggers_cache.clear();
                        if context.selected_table.is_some() {
                            self.table_triggers.clear();
                            self.triggers_table = None;
                            self.selected_trigger = None;
                        }
                    }
                }

                (Task::none(), None)
            }
            Message::SidebarRelationsLoaded(result) => {
                if !context.is_connected {
                    return (Task::none(), None);
                }

                match result {
                    Ok(entries) => {
                        self.set_sidebar_relation_entries(context.selected_table, entries)
                    }
                    Err(_) => {
                        self.sidebar_relations.clear();
                        self.table_relations_cache.clear();
                        if context.selected_table.is_some() {
                            self.table_relations.clear();
                            self.relations_table = None;
                        }
                    }
                }

                (Task::none(), None)
            }
            Message::Tooltip(value) => (Task::none(), Some(Output::Tooltip(value))),
            Message::ToggleSidebarTools => {
                if !self.sidebar_tools_open {
                    self.table_context_menu = None;
                    self.postgres_object_context_menu = None;
                    self.folder_context_menu = None;
                    self.sidebar_filter_open = false;
                }
                self.sidebar_tools_open = !self.sidebar_tools_open;
                (Task::none(), None)
            }
            Message::CloseSidebarTools => {
                self.sidebar_tools_open = false;
                (Task::none(), None)
            }
            Message::ToggleSidebarFilter => {
                if !self.sidebar_filter_open {
                    self.table_context_menu = None;
                    self.postgres_object_context_menu = None;
                    self.folder_context_menu = None;
                    self.sidebar_tools_open = false;
                }
                self.sidebar_filter_open = !self.sidebar_filter_open;
                (Task::none(), None)
            }
            Message::CloseSidebarFilter => {
                self.sidebar_filter_open = false;
                (Task::none(), None)
            }
            Message::SidebarFilterToggled { filter, enabled } => {
                if filter.visible_for_driver(context.driver) {
                    self.set_sidebar_filter_enabled(filter, enabled);
                    if enabled && !self.tables.is_empty() {
                        if filter == SidebarFilterKind::Triggers
                            && self.sidebar_triggers.is_empty()
                            && let (Some(pool), Some(database)) =
                                (context.pool, context.database.clone())
                        {
                            return (
                                Task::perform(
                                    fetch_sidebar_triggers(pool.clone(), database),
                                    Message::SidebarTriggersLoaded,
                                ),
                                None,
                            );
                        }
                        if filter == SidebarFilterKind::Relationships
                            && self.sidebar_relations.is_empty()
                            && let (Some(pool), Some(database)) =
                                (context.pool, context.database.clone())
                        {
                            return (
                                Task::perform(
                                    fetch_sidebar_relations(pool.clone(), database),
                                    Message::SidebarRelationsLoaded,
                                ),
                                None,
                            );
                        }
                    }
                }
                (Task::none(), None)
            }
        }
    }
}

pub(crate) struct Context<'a> {
    pub(crate) menu_fallback: Point,
    pub(crate) workspace_busy: bool,
    pub(crate) query_running: bool,
    pub(crate) theme: iced::Theme,
    pub(crate) settings: &'a crate::model::settings::Settings,
    pub(crate) folder_keys: Option<(String, String)>,
    pub(crate) driver: DatabaseDriver,
    pub(crate) pool: Option<&'a DatabasePool>,
    pub(crate) database: Option<String>,
    pub(crate) is_connected: bool,
    pub(crate) selected_table: Option<&'a str>,
    pub(crate) query: String,
}

fn trigger_definer_parts(definer: &str) -> (String, String) {
    let trimmed = definer.trim();
    if trimmed.is_empty() {
        return (String::new(), String::from("%"));
    }
    match trimmed.rsplit_once('@') {
        Some((user, host)) => (
            user.trim().trim_matches('`').trim_matches('"').to_string(),
            host.trim().trim_matches('`').trim_matches('"').to_string(),
        ),
        None => (
            trimmed.trim_matches('`').trim_matches('"').to_string(),
            String::from("%"),
        ),
    }
}
