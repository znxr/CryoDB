use crate::app::core::App;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::types::{
    OmniBarMode, OmniCommandEntry, OmniCommandHandler, OmniCommandScope, OmniResultAction,
    OmniResultItem, OmniSettingAction,
};
use crate::constants::{
    AI_PROVIDERS, DENSITY_CHOICES, FONT_SIZES, MAX_RECENT_CONNECTIONS, MIN_FONT_SIZE, MODE_COMMAND,
    MODE_TABLE_SEARCH,
};
use crate::model::connection::DatabaseDriver;
use crate::model::settings::{
    AppearanceColor, ShortcutBinding, SystemThemeMode, ThemeChoice, ThemeVariant,
};
use crate::model::table::PostgresObjectKind;
use crate::storage::resolve_font_choice;
use crate::utils::fuzzy::fuzzy_match;
use crate::utils::helpers::{
    normalize_omni_prefix, parse_omni_setting_usize, parse_omni_setting_value,
    parse_shortcut_binding, register_command, resolve_ai_provider, resolve_theme_choice,
    resolve_ui_density, strip_ascii_prefix_case_insensitive,
};
use std::collections::HashSet;

impl App {
    pub(crate) fn build_omni_command_registry() -> Vec<OmniCommandEntry> {
        let mut registry = Vec::new();
        register_command(
            &mut registry,
            "db.connect",
            "db connect",
            OmniCommandHandler::DbConnect,
            "Database",
            &["database", "connection", "connect"],
        );
        register_command(
            &mut registry,
            "db.switch_database",
            "switch database",
            OmniCommandHandler::DbSwitchDatabase,
            "Database",
            &["database", "switch", "use"],
        );
        register_command(
            &mut registry,
            "db.disconnect",
            "db disconnect",
            OmniCommandHandler::DbDisconnect,
            "Database",
            &["database", "disconnect"],
        );
        register_command(
            &mut registry,
            "db.refresh",
            "db refresh",
            OmniCommandHandler::DbRefresh,
            "Database",
            &["database", "reload", "refresh"],
        );
        register_command(
            &mut registry,
            "db.reconnect",
            "db reconnect",
            OmniCommandHandler::DbReconnect,
            "Database",
            &["database", "reconnect"],
        );
        register_command(
            &mut registry,
            "db.open_postgres_terminal",
            "Open PostgreSQL Terminal",
            OmniCommandHandler::OpenPostgresTerminal,
            "Database",
            &["postgres", "postgresql", "psql", "terminal", "shell"],
        );
        register_command(
            &mut registry,
            "db.select_driver",
            "select driver",
            OmniCommandHandler::SelectDriver,
            "Database",
            &[
                "driver", "database", "mysql", "mariadb", "sqlite", "postgres",
            ],
        );
        register_command(
            &mut registry,
            "query.new",
            "new query",
            OmniCommandHandler::NewQuery,
            "Query",
            &["query", "tab", "new"],
        );
        register_command(
            &mut registry,
            "query.duplicate_tab",
            "duplicate query tab",
            OmniCommandHandler::DuplicateQueryTab,
            "Query",
            &["query", "tab", "duplicate"],
        );
        register_command(
            &mut registry,
            "query.close_tab",
            "close query tab",
            OmniCommandHandler::CloseQueryTab,
            "Query",
            &["query", "tab", "close"],
        );
        register_command(
            &mut registry,
            "query.run",
            "run query",
            OmniCommandHandler::RunQuery,
            "Query",
            &["query", "execute", "run"],
        );
        register_command(
            &mut registry,
            "query.format",
            "format query",
            OmniCommandHandler::FormatQuery,
            "Query",
            &["query", "format", "pretty"],
        );
        register_command(
            &mut registry,
            "query.explain",
            "explain query",
            OmniCommandHandler::ExplainQuery,
            "Query",
            &["query", "explain", "plan"],
        );
        register_command(
            &mut registry,
            "folders.generate",
            "group folders with AI",
            OmniCommandHandler::GenerateFolders,
            "Navigation",
            &["folders", "groups", "ai", "generate"],
        );
        register_command(
            &mut registry,
            "folders.ungroup",
            "ungroup folders",
            OmniCommandHandler::UngroupFolders,
            "Navigation",
            &["folders", "ungroup", "flatten"],
        );
        register_command(
            &mut registry,
            "folders.expand",
            "expand folders",
            OmniCommandHandler::ExpandFolders,
            "Navigation",
            &["folders", "expand", "open"],
        );
        register_command(
            &mut registry,
            "folders.collapse",
            "collapse folders",
            OmniCommandHandler::CollapseFolders,
            "Navigation",
            &["folders", "collapse", "close"],
        );
        register_command(
            &mut registry,
            "schema.diagram",
            "open schema diagram",
            OmniCommandHandler::OpenSchemaDiagram,
            "Navigation",
            &["schema", "diagram", "er", "erd", "relations", "canvas"],
        );
        register_command(
            &mut registry,
            "folder.create",
            "create folder",
            OmniCommandHandler::CreateFolder,
            "Navigation",
            &["folder", "create", "new", "group"],
        );
        register_command(
            &mut registry,
            "folder.assign_table",
            "assign table to folder",
            OmniCommandHandler::MoveTableToFolder,
            "Navigation",
            &["folder", "table", "assign", "move"],
        );
        register_command(
            &mut registry,
            "folder.remove",
            "remove folder",
            OmniCommandHandler::RemoveFolder,
            "Navigation",
            &["folder", "remove", "dissolve", "ungroup"],
        );
        register_command(
            &mut registry,
            "tabs.switch",
            "switch tabs",
            OmniCommandHandler::SwitchTabs,
            "Navigation",
            &["tabs", "switch", "query", "table"],
        );
        register_command(
            &mut registry,
            "ui.hide_sidebar",
            "hide sidebar",
            OmniCommandHandler::HideSidebar,
            "Navigation",
            &["ui", "sidebar", "hide", "panel"],
        );
        register_command(
            &mut registry,
            "ui.show_sidebar",
            "show sidebar",
            OmniCommandHandler::ShowSidebar,
            "Navigation",
            &["ui", "sidebar", "show", "panel"],
        );
        register_command(
            &mut registry,
            "ui.hide_tabs",
            "hide tabs",
            OmniCommandHandler::HideTabs,
            "Navigation",
            &["ui", "tabs", "hide", "strip"],
        );
        register_command(
            &mut registry,
            "ui.show_tabs",
            "show tabs",
            OmniCommandHandler::ShowTabs,
            "Navigation",
            &["ui", "tabs", "show", "strip"],
        );
        register_command(
            &mut registry,
            "ui.hide_query_editor",
            "hide query editor",
            OmniCommandHandler::HideQueryEditor,
            "Navigation",
            &["ui", "query", "editor", "hide"],
        );
        register_command(
            &mut registry,
            "ui.show_query_editor",
            "show query editor",
            OmniCommandHandler::ShowQueryEditor,
            "Navigation",
            &["ui", "query", "editor", "show"],
        );
        register_command(
            &mut registry,
            "ui.zen_mode",
            "zen mode",
            OmniCommandHandler::ToggleZenMode,
            "Navigation",
            &["ui", "focus", "zen", "mode"],
        );
        register_command(
            &mut registry,
            "ui.focus_on",
            "focus on",
            OmniCommandHandler::FocusOn,
            "Navigation",
            &["focus", "sidebar", "query", "results"],
        );
        register_command(
            &mut registry,
            "query.clear",
            "clear query editor",
            OmniCommandHandler::ClearQueryEditor,
            "Navigation",
            &["query", "clear", "editor"],
        );
        register_command(
            &mut registry,
            "connection.favorite",
            "open favorite connection",
            OmniCommandHandler::OpenFavoriteConnection,
            "Navigation",
            &["favorite", "connection", "connect"],
        );
        register_command(
            &mut registry,
            "connection.recent",
            "open recent connection",
            OmniCommandHandler::OpenRecentConnection,
            "Navigation",
            &["recent", "connection", "connect"],
        );
        register_command(
            &mut registry,
            "query.reopen",
            "reopen last query",
            OmniCommandHandler::ReopenLastQuery,
            "Navigation",
            &["query", "history", "reopen"],
        );
        register_command(
            &mut registry,
            "query.clear_recent",
            "clear recent queries",
            OmniCommandHandler::ClearRecentQueries,
            "Navigation",
            &["query", "recent", "history", "clear"],
        );
        register_command(
            &mut registry,
            "connection.switch",
            "switch connection",
            OmniCommandHandler::SwitchConnection,
            "Navigation",
            &["switch", "connection", "database"],
        );
        register_command(
            &mut registry,
            "settings.commands",
            "settings",
            OmniCommandHandler::OpenSettingsCommands,
            "Settings",
            &["settings", "preferences", "theme", "font"],
        );
        register_command(
            &mut registry,
            "app.check_for_updates",
            "check for updates",
            OmniCommandHandler::CheckForUpdates,
            "Settings",
            &["update", "upgrade", "version", "release"],
        );
        register_command(
            &mut registry,
            "settings.modal",
            "open settings modal",
            OmniCommandHandler::OpenSettingsModal,
            "Settings",
            &["settings", "preferences", "modal"],
        );
        register_command(
            &mut registry,
            "settings.appearance",
            "appearance settings",
            OmniCommandHandler::OpenSettingsThemes,
            "Settings",
            &["settings", "theme", "appearance", "color", "palette"],
        );
        register_command(
            &mut registry,
            "settings.font_family",
            "change font family",
            OmniCommandHandler::OpenSettingsFonts,
            "Settings",
            &["settings", "font", "family", "appearance"],
        );
        register_command(
            &mut registry,
            "settings.font_size",
            "change font size",
            OmniCommandHandler::OpenSettingsFontSizes,
            "Settings",
            &["settings", "font", "size", "appearance"],
        );
        register_command(
            &mut registry,
            "settings.ui_density",
            "change ui density",
            OmniCommandHandler::OpenSettingsUiDensity,
            "Settings",
            &["settings", "density", "ui", "appearance"],
        );
        register_command(
            &mut registry,
            "settings.ai_provider",
            "change ai provider",
            OmniCommandHandler::OpenSettingsAiProviders,
            "Settings",
            &["settings", "ai", "provider"],
        );
        register_command(
            &mut registry,
            "settings.table_shortcut",
            "change table-search shortcut",
            OmniCommandHandler::OpenSettingsTableShortcuts,
            "Settings",
            &["settings", "shortcut", "table", "omni"],
        );
        register_command(
            &mut registry,
            "settings.command_shortcut",
            "change command-palette shortcut",
            OmniCommandHandler::OpenSettingsCommandShortcuts,
            "Settings",
            &["settings", "shortcut", "command", "omni"],
        );
        register_command(
            &mut registry,
            "ai.chat",
            "open AI chat",
            OmniCommandHandler::GenerateSql,
            "AI",
            &["ai", "chat", "generate", "sql", "ask"],
        );
        register_command(
            &mut registry,
            "ai.fix_query",
            "fix query with AI",
            OmniCommandHandler::FixQueryWithAi,
            "AI",
            &["ai", "fix", "query", "error"],
        );
        register_command(
            &mut registry,
            "ai.optimize_sql",
            "optimize sql (AI)",
            OmniCommandHandler::OptimizeSql,
            "AI",
            &["ai", "optimize", "sql"],
        );
        register_command(
            &mut registry,
            "ai.explain_schema",
            "explain schema (AI)",
            OmniCommandHandler::ExplainSchema,
            "AI",
            &["ai", "schema", "explain"],
        );
        registry
    }

    pub(crate) fn omni_bar_results(&self) -> &[OmniResultItem] {
        self.shell
            .omni_bar
            .as_ref()
            .map(|omni_bar| omni_bar.results.as_slice())
            .unwrap_or(&[])
    }

    pub(crate) fn refresh_omni_bar_results(&mut self) {
        let Some(snapshot) = self.shell.omni_bar.as_ref().map(|omni_bar| {
            (
                omni_bar.mode,
                omni_bar.command_scope,
                omni_bar.query.clone(),
                omni_bar.selected_index,
            )
        }) else {
            return;
        };
        let (mode, scope, query, selected_index) = snapshot;
        let results = self.build_omni_bar_results(mode, scope, &query);
        if let Some(omni_bar) = self.shell.omni_bar.as_mut() {
            omni_bar.results = results;
            if omni_bar.results.is_empty() {
                omni_bar.selected_index = 0;
            } else {
                let clamped = selected_index.min(omni_bar.results.len().saturating_sub(1));
                omni_bar.selected_index = clamped;
            }
        }
    }

    pub(crate) fn maybe_refresh_omni_bar_results(&mut self) {
        if self.is_omni_bar_open() {
            self.refresh_omni_bar_results();
        }
    }

    pub(crate) fn omni_setting_choice_results<T: Copy + PartialEq + ToString>(
        choices: &[T],
        selected: T,
        query: &str,
        subtitle: &str,
        action: impl Fn(T) -> OmniSettingAction,
    ) -> Vec<OmniResultItem> {
        choices
            .iter()
            .copied()
            .filter_map(|choice| {
                let title = choice.to_string();
                let matching = fuzzy_match(&title, query);
                (query.is_empty() || matching.is_some()).then(|| OmniResultItem {
                    title: title.clone(),
                    subtitle: subtitle.to_string(),
                    category: String::from("Appearance"),
                    match_indices: matching
                        .map(|matching| matching.indices)
                        .unwrap_or_default(),
                    score: if choice == selected { 30 } else { 0 },
                    action: OmniResultAction::ApplySetting(action(choice)),
                    autocomplete: title,
                })
            })
            .collect()
    }

    pub(crate) fn build_omni_bar_results(
        &self,
        mode: OmniBarMode,
        scope: OmniCommandScope,
        query: &str,
    ) -> Vec<OmniResultItem> {
        let query = query.trim();
        let query_lower = (!query.is_empty()).then(|| query.to_ascii_lowercase());
        let mut results = Vec::new();

        match mode {
            MODE_TABLE_SEARCH => {
                if self.connections.current.driver == DatabaseDriver::PostgreSql
                    && !self.workspace.explorer.postgres_sidebar_objects.is_empty()
                {
                    let mut seen_selectable_keys = HashSet::new();
                    for object in &self.workspace.explorer.postgres_sidebar_objects {
                        let qualified = object.qualified_name();
                        let searchable = format!(
                            "{} {} {} {}",
                            qualified,
                            object.schema,
                            object.name,
                            object.kind.searchable_label()
                        );
                        let is_selected = self.postgres_object_is_selected(object)
                            || self
                                .workspace
                                .selected_table
                                .as_ref()
                                .is_some_and(|selected| {
                                    self.table_key(selected) == self.table_key(&qualified)
                                });
                        let score_bonus = if is_selected { 20 } else { 0 };
                        let action = if object.kind.is_selectable_table() {
                            let key = self.table_key(&qualified);
                            if !key.is_empty() {
                                seen_selectable_keys.insert(key);
                            }
                            OmniResultAction::OpenTable(qualified.clone())
                        } else {
                            OmniResultAction::OpenPostgresObject(object.clone())
                        };

                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: qualified.clone(),
                                subtitle: String::from(Self::postgres_object_label(object.kind)),
                                category: String::from(object.kind.section_label()),
                                match_indices: Vec::new(),
                                score: score_bonus,
                                action,
                                autocomplete: qualified,
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: qualified.clone(),
                                subtitle: String::from(Self::postgres_object_label(object.kind)),
                                category: String::from(object.kind.section_label()),
                                match_indices: fuzzy_match(&qualified, query)
                                    .map(|value| value.indices)
                                    .unwrap_or_default(),
                                score: matching.score + score_bonus,
                                action,
                                autocomplete: qualified,
                            });
                        }
                    }

                    for table in &self.workspace.explorer.tables {
                        let table_key = self.table_key(table);
                        if !table_key.is_empty() && seen_selectable_keys.contains(&table_key) {
                            continue;
                        }

                        let kind = self
                            .postgres_object_kind_for_table(table)
                            .unwrap_or(PostgresObjectKind::Table);
                        let searchable = format!(
                            "{} {}",
                            self.table_search_terms(table),
                            kind.searchable_label()
                        );
                        let is_selected = self
                            .workspace
                            .selected_table
                            .as_ref()
                            .is_some_and(|selected| self.table_key(selected) == table_key);
                        let score_bonus = if is_selected { 20 } else { 0 };
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: String::from(Self::postgres_object_label(kind)),
                                category: String::from(kind.section_label()),
                                match_indices: Vec::new(),
                                score: score_bonus,
                                action: OmniResultAction::OpenTable(table.clone()),
                                autocomplete: table.clone(),
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: String::from(Self::postgres_object_label(kind)),
                                category: String::from(kind.section_label()),
                                match_indices: fuzzy_match(table, query)
                                    .map(|value| value.indices)
                                    .unwrap_or_default(),
                                score: matching.score + score_bonus,
                                action: OmniResultAction::OpenTable(table.clone()),
                                autocomplete: table.clone(),
                            });
                        }
                    }
                } else {
                    for table in &self.workspace.explorer.tables {
                        let searchable = self.table_search_terms(table);
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: String::from("Table"),
                                category: String::from("Tables"),
                                match_indices: Vec::new(),
                                score: if self
                                    .workspace
                                    .selected_table
                                    .as_deref()
                                    .is_some_and(|selected| selected == table)
                                {
                                    20
                                } else {
                                    0
                                },
                                action: OmniResultAction::OpenTable(table.clone()),
                                autocomplete: table.clone(),
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: String::from("Table"),
                                category: String::from("Tables"),
                                match_indices: fuzzy_match(table, query)
                                    .map(|value| value.indices)
                                    .unwrap_or_default(),
                                score: matching.score,
                                action: OmniResultAction::OpenTable(table.clone()),
                                autocomplete: table.clone(),
                            });
                        }
                    }
                }
            }
            MODE_COMMAND => match scope {
                OmniCommandScope::Default => {
                    if query.is_empty() {
                        for (index, id) in
                            self.settings.values.recent_omni_commands.iter().enumerate()
                        {
                            let Some(command) = self
                                .shell
                                .omni_commands
                                .iter()
                                .find(|command| command.id == id)
                            else {
                                continue;
                            };
                            if self.omni_command_visible(command.handler)
                                && (self.settings.values.ai_enabled
                                    || (command.category != "AI"
                                        && command.id != "settings.ai_provider"))
                            {
                                results.push(OmniResultItem {
                                    title: command.title.to_string(),
                                    subtitle: command.id.to_string(),
                                    category: String::from("Recently used"),
                                    match_indices: Vec::new(),
                                    score: 1000 - index as i32,
                                    action: OmniResultAction::RunCommand(command.handler),
                                    autocomplete: command.title.to_string(),
                                });
                            }
                        }
                    }
                    for command in &self.shell.omni_commands {
                        if !self.omni_command_visible(command.handler) {
                            continue;
                        }
                        if !self.settings.values.ai_enabled
                            && (command.category == "AI" || command.id == "settings.ai_provider")
                        {
                            continue;
                        }
                        if query.is_empty()
                            && self
                                .settings
                                .values
                                .recent_omni_commands
                                .iter()
                                .any(|id| id == command.id)
                        {
                            continue;
                        }

                        let title_match = fuzzy_match(command.title, query);
                        let has_keyword_match = if query.is_empty() {
                            false
                        } else {
                            command
                                .id
                                .to_ascii_lowercase()
                                .contains(query_lower.as_deref().unwrap_or_default())
                                || command.keywords.iter().any(|keyword| {
                                    keyword
                                        .to_ascii_lowercase()
                                        .contains(query_lower.as_deref().unwrap_or_default())
                                })
                        };

                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: command.title.to_string(),
                                subtitle: command.id.to_string(),
                                category: command.category.to_string(),
                                match_indices: Vec::new(),
                                score: 0,
                                action: OmniResultAction::RunCommand(command.handler),
                                autocomplete: command.title.to_string(),
                            });
                        } else if let Some(matching) = title_match {
                            results.push(OmniResultItem {
                                title: command.title.to_string(),
                                subtitle: command.id.to_string(),
                                category: command.category.to_string(),
                                match_indices: matching.indices,
                                score: matching.score,
                                action: OmniResultAction::RunCommand(command.handler),
                                autocomplete: command.title.to_string(),
                            });
                        } else if has_keyword_match {
                            results.push(OmniResultItem {
                                title: command.title.to_string(),
                                subtitle: command.id.to_string(),
                                category: command.category.to_string(),
                                match_indices: Vec::new(),
                                score: -25,
                                action: OmniResultAction::RunCommand(command.handler),
                                autocomplete: command.title.to_string(),
                            });
                        }
                    }

                    if let Some(target) = query.strip_prefix("connect ") {
                        let target = target.trim();
                        for database in &self.connections.databases {
                            if target.is_empty() {
                                results.push(OmniResultItem {
                                    title: format!("connect {}", database),
                                    subtitle: String::from("Switch active database"),
                                    category: String::from("Database"),
                                    match_indices: Vec::new(),
                                    score: 30,
                                    action: OmniResultAction::ConnectDatabase(database.clone()),
                                    autocomplete: format!("connect {database}"),
                                });
                            } else if let Some(matching) = fuzzy_match(database, target) {
                                let mut indices = Vec::new();
                                let prefix_offset = "connect ".chars().count();
                                indices.extend(
                                    matching
                                        .indices
                                        .into_iter()
                                        .map(|index| index + prefix_offset),
                                );
                                results.push(OmniResultItem {
                                    title: format!("connect {}", database),
                                    subtitle: String::from("Switch active database"),
                                    category: String::from("Database"),
                                    match_indices: indices,
                                    score: matching.score + 40,
                                    action: OmniResultAction::ConnectDatabase(database.clone()),
                                    autocomplete: format!("connect {database}"),
                                });
                            }
                        }
                    }
                    if let Some(target) =
                        strip_ascii_prefix_case_insensitive(query, "connect favorite ")
                    {
                        let target = target.trim();
                        for (index, connection) in self.connections.favorites.iter().enumerate() {
                            let label = connection.display_label();
                            if target.is_empty() {
                                results.push(OmniResultItem {
                                    title: format!("connect favorite {label}"),
                                    subtitle: format!(
                                        "{}:{} as {}",
                                        connection.host, connection.port, connection.username
                                    ),
                                    category: String::from("Connections"),
                                    match_indices: Vec::new(),
                                    score: 35,
                                    action: OmniResultAction::ConnectFavorite(index),
                                    autocomplete: format!("connect favorite {label}"),
                                });
                            } else if let Some(matching) = fuzzy_match(&label, target) {
                                let prefix_offset = "connect favorite ".chars().count();
                                results.push(OmniResultItem {
                                    title: format!("connect favorite {label}"),
                                    subtitle: format!(
                                        "{}:{} as {}",
                                        connection.host, connection.port, connection.username
                                    ),
                                    category: String::from("Connections"),
                                    match_indices: matching
                                        .indices
                                        .into_iter()
                                        .map(|index| index + prefix_offset)
                                        .collect(),
                                    score: matching.score + 45,
                                    action: OmniResultAction::ConnectFavorite(index),
                                    autocomplete: format!("connect favorite {label}"),
                                });
                            }
                        }
                    }
                }
                OmniCommandScope::Databases => {
                    for database in &self.connections.databases {
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: database.clone(),
                                subtitle: String::from("Switch active database"),
                                category: String::from("Database"),
                                match_indices: Vec::new(),
                                score: if self.connections.current.database == *database {
                                    20
                                } else {
                                    0
                                },
                                action: OmniResultAction::ConnectDatabase(database.clone()),
                                autocomplete: database.clone(),
                            });
                        } else if let Some(matching) = fuzzy_match(database, query) {
                            results.push(OmniResultItem {
                                title: database.clone(),
                                subtitle: String::from("Switch active database"),
                                category: String::from("Database"),
                                match_indices: matching.indices,
                                score: matching.score + 10,
                                action: OmniResultAction::ConnectDatabase(database.clone()),
                                autocomplete: database.clone(),
                            });
                        }
                    }
                }
                OmniCommandScope::Drivers => {
                    for driver in DatabaseDriver::ALL {
                        let title = driver.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Select connection driver"),
                                category: String::from("Database"),
                                match_indices: Vec::new(),
                                score: if self.connections.current.driver == driver {
                                    20
                                } else {
                                    0
                                },
                                action: OmniResultAction::SelectDriver(driver),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Select connection driver"),
                                category: String::from("Database"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if self.connections.current.driver == driver {
                                        20
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::SelectDriver(driver),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::FolderTableTargets => {
                    for table in &self.workspace.explorer.tables {
                        let folder = self
                            .folder_for_table(table)
                            .unwrap_or_else(|| String::from("No folder"));
                        let searchable = format!("{} {folder}", self.table_search_terms(table));
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: format!("Current folder: {folder}"),
                                category: String::from("Folders"),
                                match_indices: Vec::new(),
                                score: 20,
                                action: OmniResultAction::OpenMoveTableToFolder(table.clone()),
                                autocomplete: table.clone(),
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: table.clone(),
                                subtitle: format!("Current folder: {folder}"),
                                category: String::from("Folders"),
                                match_indices: fuzzy_match(table, query)
                                    .map(|value| value.indices)
                                    .unwrap_or_default(),
                                score: matching.score + 20,
                                action: OmniResultAction::OpenMoveTableToFolder(table.clone()),
                                autocomplete: table.clone(),
                            });
                        }
                    }
                }
                OmniCommandScope::FolderDissolveTargets => {
                    for folder in &self.workspace.explorer.table_folders {
                        let title = folder.name.clone();
                        let table_count = self
                            .workspace
                            .explorer
                            .table_folder_map
                            .values()
                            .filter(|current| *current == &folder.name)
                            .count();
                        let subtitle =
                            format!("Remove folder group ({table_count} table assignments)");
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: subtitle.clone(),
                                category: String::from("Folders"),
                                match_indices: Vec::new(),
                                score: 20,
                                action: OmniResultAction::DissolveFolder(title.clone()),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&folder.name, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle,
                                category: String::from("Folders"),
                                match_indices: matching.indices,
                                score: matching.score + 20,
                                action: OmniResultAction::DissolveFolder(title.clone()),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::FavoriteConnections => {
                    for (index, connection) in self.connections.favorites.iter().enumerate() {
                        let label = connection.display_label();
                        let searchable = format!(
                            "{} {} {} {}",
                            label, connection.host, connection.database, connection.username
                        );
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: label.clone(),
                                subtitle: format!(
                                    "{}:{} as {}",
                                    connection.host, connection.port, connection.username
                                ),
                                category: String::from("Favorites"),
                                match_indices: Vec::new(),
                                score: 100 - index as i32,
                                action: OmniResultAction::ConnectFavorite(index),
                                autocomplete: label,
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: label.clone(),
                                subtitle: format!(
                                    "{}:{} as {}",
                                    connection.host, connection.port, connection.username
                                ),
                                category: String::from("Favorites"),
                                match_indices: fuzzy_match(&label, query)
                                    .map(|matching| matching.indices)
                                    .unwrap_or_default(),
                                score: matching.score + 12,
                                action: OmniResultAction::ConnectFavorite(index),
                                autocomplete: label,
                            });
                        }
                    }
                }
                OmniCommandScope::RecentConnections => {
                    for (index, connection) in self.connections.recents.iter().enumerate() {
                        let label = connection.display_label();
                        let searchable = format!(
                            "{} {} {} {}",
                            label, connection.host, connection.database, connection.username
                        );
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: label.clone(),
                                subtitle: format!(
                                    "{}:{} as {}",
                                    connection.host, connection.port, connection.username
                                ),
                                category: String::from("Connections"),
                                match_indices: Vec::new(),
                                score: (MAX_RECENT_CONNECTIONS as i32) - index as i32,
                                action: OmniResultAction::ConnectRecent(index),
                                autocomplete: label,
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: label.clone(),
                                subtitle: format!(
                                    "{}:{} as {}",
                                    connection.host, connection.port, connection.username
                                ),
                                category: String::from("Connections"),
                                match_indices: fuzzy_match(&label, query)
                                    .map(|matching| matching.indices)
                                    .unwrap_or_default(),
                                score: matching.score + 10,
                                action: OmniResultAction::ConnectRecent(index),
                                autocomplete: label,
                            });
                        }
                    }
                }
                OmniCommandScope::TabSwitcher => {
                    let tabs = self.visible_switchable_tabs();
                    let active = self.active_tab_entry();
                    for (index, entry) in tabs.into_iter().enumerate() {
                        let title = self.tab_entry_title(&entry);
                        let subtitle = match &entry {
                            TabEntry::Query(tab_index) => self
                                .workspace
                                .tabs
                                .query_tabs
                                .get(*tab_index)
                                .map(|tab| {
                                    if tab.pinned {
                                        String::from("Query tab (pinned)")
                                    } else {
                                        String::from("Query tab")
                                    }
                                })
                                .unwrap_or_else(|| String::from("Query tab")),
                            TabEntry::Table(table) => {
                                if self.workspace.tabs.pinned_table_tabs.contains(table) {
                                    String::from("Table tab (pinned)")
                                } else {
                                    String::from("Table tab")
                                }
                            }
                            TabEntry::Diagram(_) => String::from("Diagram tab"),
                        };
                        let searchable = format!("{title} {subtitle}");
                        let active_bonus =
                            if active.as_ref().is_some_and(|current| current == &entry) {
                                120
                            } else {
                                0
                            };

                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle,
                                category: String::from("Tabs"),
                                match_indices: Vec::new(),
                                score: active_bonus + (80 - index as i32),
                                action: OmniResultAction::ActivateTab(entry),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&searchable, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle,
                                category: String::from("Tabs"),
                                match_indices: fuzzy_match(&title, query)
                                    .map(|value| value.indices)
                                    .unwrap_or_default(),
                                score: matching.score + active_bonus + 25,
                                action: OmniResultAction::ActivateTab(entry),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::FocusTargets => {
                    let mut entries = Vec::new();
                    if !self.is_sidebar_hidden() {
                        entries.push((
                            "sidebar",
                            "Focus sidebar table search",
                            OmniCommandHandler::FocusSidebar,
                        ));
                    }
                    entries.push((
                        "query editor",
                        "Focus the query editor input",
                        OmniCommandHandler::FocusQueryEditor,
                    ));
                    entries.push((
                        "table results",
                        "Focus results and select first row",
                        OmniCommandHandler::FocusTableResults,
                    ));
                    for (title, subtitle, handler) in entries {
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.to_string(),
                                subtitle: subtitle.to_string(),
                                category: String::from("Focus"),
                                match_indices: Vec::new(),
                                score: 40,
                                action: OmniResultAction::RunCommand(handler),
                                autocomplete: title.to_string(),
                            });
                        } else if let Some(matching) = fuzzy_match(title, query) {
                            results.push(OmniResultItem {
                                title: title.to_string(),
                                subtitle: subtitle.to_string(),
                                category: String::from("Focus"),
                                match_indices: matching.indices,
                                score: matching.score + 40,
                                action: OmniResultAction::RunCommand(handler),
                                autocomplete: title.to_string(),
                            });
                        } else if query_lower
                            .as_ref()
                            .is_some_and(|needle| subtitle.to_ascii_lowercase().contains(needle))
                        {
                            results.push(OmniResultItem {
                                title: title.to_string(),
                                subtitle: subtitle.to_string(),
                                category: String::from("Focus"),
                                match_indices: Vec::new(),
                                score: 12,
                                action: OmniResultAction::RunCommand(handler),
                                autocomplete: title.to_string(),
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsRoot => {
                    let mut push_entry =
                        |title: String,
                         subtitle: String,
                         action: OmniResultAction,
                         autocomplete: String,
                         base_score: i32| {
                            if query.is_empty() {
                                results.push(OmniResultItem {
                                    title,
                                    subtitle,
                                    category: String::from("Settings"),
                                    match_indices: Vec::new(),
                                    score: base_score,
                                    action,
                                    autocomplete,
                                });
                            } else if let Some(matching) = fuzzy_match(&title, query) {
                                results.push(OmniResultItem {
                                    title,
                                    subtitle,
                                    category: String::from("Settings"),
                                    match_indices: matching.indices,
                                    score: matching.score + base_score,
                                    action,
                                    autocomplete,
                                });
                            } else if query_lower.as_ref().is_some_and(|needle| {
                                title.to_ascii_lowercase().contains(needle)
                                    || subtitle.to_ascii_lowercase().contains(needle)
                                    || autocomplete.to_ascii_lowercase().contains(needle)
                            }) {
                                results.push(OmniResultItem {
                                    title,
                                    subtitle,
                                    category: String::from("Settings"),
                                    match_indices: Vec::new(),
                                    score: base_score - 20,
                                    action,
                                    autocomplete,
                                });
                            }
                        };

                    push_entry(
                        String::from("open settings modal"),
                        String::from("Open full Settings dialog"),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsModal),
                        String::from("open settings modal"),
                        10,
                    );
                    push_entry(
                        String::from("appearance settings"),
                        format!(
                            "{} mode · {} style",
                            self.settings.values.system_theme_mode,
                            self.settings.values.theme_variant
                        ),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsThemes),
                        String::from("appearance settings"),
                        15,
                    );
                    push_entry(
                        String::from("change font family"),
                        format!("Current: {}", self.settings.values.font),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsFonts),
                        String::from("change font family"),
                        14,
                    );
                    push_entry(
                        String::from("change font size"),
                        format!("Current: {}", self.settings.values.font_size),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsFontSizes),
                        String::from("change font size"),
                        14,
                    );
                    push_entry(
                        String::from("change ui density"),
                        format!("Current: {}", self.settings.values.ui_density),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsUiDensity),
                        String::from("change ui density"),
                        12,
                    );
                    push_entry(
                        String::from("change ai provider"),
                        format!("Current: {}", self.settings.values.ai_provider),
                        OmniResultAction::RunCommand(OmniCommandHandler::OpenSettingsAiProviders),
                        String::from("change ai provider"),
                        11,
                    );
                    push_entry(
                        String::from("change table-search shortcut"),
                        format!("Current: {}", self.settings.values.omni_table_shortcut),
                        OmniResultAction::RunCommand(
                            OmniCommandHandler::OpenSettingsTableShortcuts,
                        ),
                        String::from("change table-search shortcut"),
                        10,
                    );
                    push_entry(
                        String::from("change command-palette shortcut"),
                        format!("Current: {}", self.settings.values.omni_command_shortcut),
                        OmniResultAction::RunCommand(
                            OmniCommandHandler::OpenSettingsCommandShortcuts,
                        ),
                        String::from("change command-palette shortcut"),
                        10,
                    );
                    push_entry(
                        String::from("set open-settings shortcut <binding>"),
                        format!("Current: {}", self.settings.values.open_settings_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set open-settings shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set switch-database shortcut <binding>"),
                        format!("Current: {}", self.settings.values.switch_database_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set switch-database shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set focus-table-search shortcut <binding>"),
                        format!(
                            "Current: {}",
                            self.settings.values.focus_table_search_shortcut
                        ),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set focus-table-search shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set toggle-tab-pin shortcut <binding>"),
                        format!("Current: {}", self.settings.values.toggle_tab_pin_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set toggle-tab-pin shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set cycle-theme shortcut <binding>"),
                        format!("Current: {}", self.settings.values.cycle_theme_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set cycle-theme shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set run-query shortcut <binding>"),
                        format!("Current: {}", self.settings.values.run_query_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set run-query shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set run-selection shortcut <binding>"),
                        format!("Current: {}", self.settings.values.run_selection_shortcut),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set run-selection shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set autocomplete-tables shortcut <binding>"),
                        format!(
                            "Current: {}",
                            self.settings.values.autocomplete_tables_shortcut
                        ),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set autocomplete-tables shortcut "),
                        9,
                    );
                    push_entry(
                        String::from("set command-palette alt shortcut <binding>"),
                        format!(
                            "Current: {}",
                            self.settings.values.omni_command_alt_shortcut
                        ),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set command-palette alt shortcut "),
                        9,
                    );
                    let next_tabs_state = !self.settings.values.tabs_enabled;
                    push_entry(
                        if next_tabs_state {
                            String::from("enable table tabs")
                        } else {
                            String::from("disable table tabs")
                        },
                        format!(
                            "Current: {}",
                            if self.settings.values.tabs_enabled {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ),
                        OmniResultAction::ApplySetting(OmniSettingAction::SetTabsEnabled(
                            next_tabs_state,
                        )),
                        if next_tabs_state {
                            String::from("enable table tabs")
                        } else {
                            String::from("disable table tabs")
                        },
                        8,
                    );
                    let next_header_emphasis = !self.settings.values.emphasize_column_headers;
                    push_entry(
                        if next_header_emphasis {
                            String::from("enable emphasized column headers")
                        } else {
                            String::from("disable emphasized column headers")
                        },
                        format!(
                            "Current: {}",
                            if self.settings.values.emphasize_column_headers {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ),
                        OmniResultAction::ApplySetting(
                            OmniSettingAction::SetEmphasizeColumnHeaders(next_header_emphasis),
                        ),
                        if next_header_emphasis {
                            String::from("enable emphasized column headers")
                        } else {
                            String::from("disable emphasized column headers")
                        },
                        8,
                    );
                    let next_auto_scroll_sidebar =
                        !self.settings.values.auto_scroll_sidebar_to_selected_table;
                    push_entry(
                        if next_auto_scroll_sidebar {
                            String::from("enable auto-scroll to selected table")
                        } else {
                            String::from("disable auto-scroll to selected table")
                        },
                        format!(
                            "Current: {}",
                            if self.settings.values.auto_scroll_sidebar_to_selected_table {
                                "enabled"
                            } else {
                                "disabled"
                            }
                        ),
                        OmniResultAction::ApplySetting(
                            OmniSettingAction::SetAutoScrollSidebarToSelectedTable(
                                next_auto_scroll_sidebar,
                            ),
                        ),
                        if next_auto_scroll_sidebar {
                            String::from("enable auto-scroll to selected table")
                        } else {
                            String::from("disable auto-scroll to selected table")
                        },
                        8,
                    );
                    push_entry(
                        String::from("set table query limit <number>"),
                        format!("Current: {}", self.settings.values.table_query_limit),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set table query limit "),
                        6,
                    );
                    push_entry(
                        String::from("set history limit <number>"),
                        format!("Current: {}", self.settings.values.history_limit),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set history limit "),
                        6,
                    );
                    push_entry(
                        String::from("set command-palette prefix <value>"),
                        format!("Current: {}", self.settings.values.omni_prefix),
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set command-palette prefix "),
                        6,
                    );
                    push_entry(
                        String::from("set ai endpoint <value>"),
                        if self.settings.values.ai_endpoint.trim().is_empty() {
                            String::from("Current: (empty)")
                        } else {
                            format!("Current: {}", self.settings.values.ai_endpoint)
                        },
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set ai endpoint "),
                        5,
                    );
                    push_entry(
                        String::from("set ai model <value>"),
                        if self.settings.values.ai_model.trim().is_empty() {
                            String::from("Current: (empty)")
                        } else {
                            format!("Current: {}", self.settings.values.ai_model)
                        },
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set ai model "),
                        5,
                    );
                    push_entry(
                        String::from("set ai api key <value>"),
                        if self.settings.values.ai_api_key.trim().is_empty() {
                            String::from("Current: (empty)")
                        } else {
                            String::from("Current: (set)")
                        },
                        OmniResultAction::RunCommand(OmniCommandHandler::FocusOmniBar),
                        String::from("set ai api key "),
                        5,
                    );

                    if let Some(limit) = parse_omni_setting_usize(query, "set table query limit ")
                        && limit > 0
                    {
                        push_entry(
                            format!("set table query limit {limit}"),
                            String::from("Apply table query row limit"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetTableQueryLimit(
                                limit,
                            )),
                            format!("set table query limit {limit}"),
                            60,
                        );
                    }
                    if let Some(limit) = parse_omni_setting_usize(query, "set history limit ") {
                        push_entry(
                            format!("set history limit {limit}"),
                            String::from("Apply query history limit"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetHistoryLimit(
                                limit,
                            )),
                            format!("set history limit {limit}"),
                            60,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set command-palette prefix ")
                            .or_else(|| parse_omni_setting_value(query, "set command prefix "))
                    {
                        push_entry(
                            format!(
                                "set command-palette prefix {}",
                                normalize_omni_prefix(&value)
                            ),
                            String::from("Update command palette prefix"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetOmniPrefix(
                                value.clone(),
                            )),
                            format!(
                                "set command-palette prefix {}",
                                normalize_omni_prefix(&value)
                            ),
                            60,
                        );
                    }
                    if query.eq_ignore_ascii_case("enable table tabs") {
                        push_entry(
                            String::from("enable table tabs"),
                            String::from("Turn on table tabs"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetTabsEnabled(true)),
                            String::from("enable table tabs"),
                            65,
                        );
                    }
                    if query.eq_ignore_ascii_case("disable table tabs") {
                        push_entry(
                            String::from("disable table tabs"),
                            String::from("Turn off table tabs"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetTabsEnabled(
                                false,
                            )),
                            String::from("disable table tabs"),
                            65,
                        );
                    }
                    if query.eq_ignore_ascii_case("enable emphasized column headers") {
                        push_entry(
                            String::from("enable emphasized column headers"),
                            String::from("Use bold labels in column headers"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetEmphasizeColumnHeaders(true),
                            ),
                            String::from("enable emphasized column headers"),
                            65,
                        );
                    }
                    if query.eq_ignore_ascii_case("disable emphasized column headers") {
                        push_entry(
                            String::from("disable emphasized column headers"),
                            String::from("Use regular labels in column headers"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetEmphasizeColumnHeaders(false),
                            ),
                            String::from("disable emphasized column headers"),
                            65,
                        );
                    }
                    if query.eq_ignore_ascii_case("enable auto-scroll to selected table") {
                        push_entry(
                            String::from("enable auto-scroll to selected table"),
                            String::from("Auto-scroll sidebar when opening a table"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetAutoScrollSidebarToSelectedTable(true),
                            ),
                            String::from("enable auto-scroll to selected table"),
                            65,
                        );
                    }
                    if query.eq_ignore_ascii_case("disable auto-scroll to selected table") {
                        push_entry(
                            String::from("disable auto-scroll to selected table"),
                            String::from("Keep sidebar scroll position when opening a table"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetAutoScrollSidebarToSelectedTable(false),
                            ),
                            String::from("disable auto-scroll to selected table"),
                            65,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set ai endpoint ") {
                        push_entry(
                            format!("set ai endpoint {value}"),
                            String::from("Update AI endpoint"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiEndpoint(
                                value.clone(),
                            )),
                            format!("set ai endpoint {value}"),
                            62,
                        );
                    } else if query.eq_ignore_ascii_case("clear ai endpoint") {
                        push_entry(
                            String::from("clear ai endpoint"),
                            String::from("Reset AI endpoint"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiEndpoint(
                                String::new(),
                            )),
                            String::from("clear ai endpoint"),
                            62,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set ai model ") {
                        push_entry(
                            format!("set ai model {value}"),
                            String::from("Update AI model"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiModel(
                                value.clone(),
                            )),
                            format!("set ai model {value}"),
                            62,
                        );
                    } else if query.eq_ignore_ascii_case("clear ai model") {
                        push_entry(
                            String::from("clear ai model"),
                            String::from("Reset AI model"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiModel(
                                String::new(),
                            )),
                            String::from("clear ai model"),
                            62,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set ai api key ") {
                        push_entry(
                            String::from("set ai api key • apply"),
                            String::from("Update AI API key"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiApiKey(value)),
                            String::from("set ai api key "),
                            62,
                        );
                    } else if query.eq_ignore_ascii_case("clear ai api key") {
                        push_entry(
                            String::from("clear ai api key"),
                            String::from("Reset AI API key"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiApiKey(
                                String::new(),
                            )),
                            String::from("clear ai api key"),
                            62,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set theme ")
                        && let Some(theme) = resolve_theme_choice(&value)
                    {
                        push_entry(
                            format!("set theme {}", theme),
                            String::from("Apply UI theme"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetTheme(theme)),
                            format!("set theme {}", theme),
                            61,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set font ")
                        && let Some(font) = resolve_font_choice(&value, &self.settings.font_choices)
                    {
                        push_entry(
                            format!("set font {}", font),
                            String::from("Apply UI font"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetFont(
                                font.clone(),
                            )),
                            format!("set font {}", font),
                            61,
                        );
                    }
                    if let Some(size) = parse_omni_setting_usize(query, "set font size ")
                        && let Ok(size) = u32::try_from(size)
                        && size >= MIN_FONT_SIZE
                    {
                        push_entry(
                            format!("set font size {}", size),
                            String::from("Apply font size"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetFontSize(size)),
                            format!("set font size {}", size),
                            61,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set ui density ")
                        && let Some(density) = resolve_ui_density(&value)
                    {
                        push_entry(
                            format!("set ui density {}", density),
                            String::from("Apply UI density"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetUiDensity(
                                density,
                            )),
                            format!("set ui density {}", density),
                            61,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set ai provider ")
                        && let Some(provider) = resolve_ai_provider(&value)
                    {
                        push_entry(
                            format!("set ai provider {}", provider),
                            String::from("Apply AI provider"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetAiProvider(
                                provider,
                            )),
                            format!("set ai provider {}", provider),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set table-search shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set table-search shortcut {}", shortcut_label),
                            String::from("Apply table search shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetOmniTableShortcut(shortcut),
                            ),
                            format!("set table-search shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set command-palette shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set command-palette shortcut {}", shortcut_label),
                            String::from("Apply command palette shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetOmniCommandShortcut(shortcut),
                            ),
                            format!("set command-palette shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set open-settings shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set open-settings shortcut {}", shortcut_label),
                            String::from("Apply open settings shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetOpenSettingsShortcut(shortcut),
                            ),
                            format!("set open-settings shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set switch-database shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set switch-database shortcut {}", shortcut_label),
                            String::from("Apply switch database shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetSwitchDatabaseShortcut(shortcut),
                            ),
                            format!("set switch-database shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set focus-table-search shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set focus-table-search shortcut {}", shortcut_label),
                            String::from("Apply focus table search shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetFocusTableSearchShortcut(shortcut),
                            ),
                            format!("set focus-table-search shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set toggle-tab-pin shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set toggle-tab-pin shortcut {}", shortcut_label),
                            String::from("Apply toggle current tab pin shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetToggleTabPinShortcut(shortcut),
                            ),
                            format!("set toggle-tab-pin shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set cycle-theme shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set cycle-theme shortcut {}", shortcut_label),
                            String::from("Apply cycle theme shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetCycleThemeShortcut(shortcut),
                            ),
                            format!("set cycle-theme shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) = parse_omni_setting_value(query, "set run-query shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set run-query shortcut {}", shortcut_label),
                            String::from("Apply run query shortcut"),
                            OmniResultAction::ApplySetting(OmniSettingAction::SetRunQueryShortcut(
                                shortcut,
                            )),
                            format!("set run-query shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set run-selection shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set run-selection shortcut {}", shortcut_label),
                            String::from("Apply run selection shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetRunSelectionShortcut(shortcut),
                            ),
                            format!("set run-selection shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set autocomplete-tables shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set autocomplete-tables shortcut {}", shortcut_label),
                            String::from("Apply autocomplete tables shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetAutocompleteTablesShortcut(shortcut),
                            ),
                            format!("set autocomplete-tables shortcut {}", shortcut_label),
                            61,
                        );
                    }
                    if let Some(value) =
                        parse_omni_setting_value(query, "set command-palette alt shortcut ")
                        && let Some(shortcut) = parse_shortcut_binding(&value)
                    {
                        let shortcut_label = shortcut.to_string();
                        push_entry(
                            format!("set command-palette alt shortcut {}", shortcut_label),
                            String::from("Apply alternate command palette shortcut"),
                            OmniResultAction::ApplySetting(
                                OmniSettingAction::SetOmniCommandAltShortcut(shortcut),
                            ),
                            format!("set command-palette alt shortcut {}", shortcut_label),
                            61,
                        );
                    }
                }
                OmniCommandScope::SettingsThemes => {
                    let entries = [
                        (
                            "appearance mode",
                            format!("Current: {}", self.settings.values.system_theme_mode),
                            OmniCommandScope::SettingsThemeModes,
                        ),
                        (
                            "interface style",
                            format!("Current: {}", self.settings.values.theme_variant),
                            OmniCommandScope::SettingsThemeVariants,
                        ),
                        (
                            "manual color palette",
                            format!("Current: {}", self.settings.theme_choice),
                            OmniCommandScope::SettingsManualThemes,
                        ),
                        (
                            "dark color palette",
                            format!("Current: {}", self.settings.values.dark_theme),
                            OmniCommandScope::SettingsDarkThemes,
                        ),
                        (
                            "light color palette",
                            format!("Current: {}", self.settings.values.light_theme),
                            OmniCommandScope::SettingsLightThemes,
                        ),
                        (
                            "accent color",
                            format!("Current: {}", self.settings.values.accent_color),
                            OmniCommandScope::SettingsAccentColors,
                        ),
                    ];
                    for (title, subtitle, scope) in entries {
                        if query.is_empty() || fuzzy_match(title, query).is_some() {
                            results.push(OmniResultItem {
                                title: title.to_string(),
                                subtitle,
                                category: String::from("Appearance"),
                                match_indices: fuzzy_match(title, query)
                                    .map(|matching| matching.indices)
                                    .unwrap_or_default(),
                                score: 20,
                                action: OmniResultAction::OpenCommandScope(scope),
                                autocomplete: title.to_string(),
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsThemeModes => {
                    results.extend(Self::omni_setting_choice_results(
                        SystemThemeMode::ALL,
                        self.settings.values.system_theme_mode,
                        query,
                        "Appearance mode",
                        OmniSettingAction::SetSystemThemeMode,
                    ));
                }
                OmniCommandScope::SettingsThemeVariants => {
                    results.extend(Self::omni_setting_choice_results(
                        ThemeVariant::ALL,
                        self.settings.values.theme_variant,
                        query,
                        "Interface style",
                        OmniSettingAction::SetThemeVariant,
                    ));
                }
                OmniCommandScope::SettingsManualThemes => {
                    results.extend(Self::omni_setting_choice_results(
                        ThemeChoice::ALL,
                        self.settings.theme_choice,
                        query,
                        "Manual color palette",
                        OmniSettingAction::SetTheme,
                    ));
                }
                OmniCommandScope::SettingsDarkThemes => {
                    results.extend(Self::omni_setting_choice_results(
                        ThemeChoice::ALL,
                        self.settings.values.dark_theme,
                        query,
                        "Dark color palette",
                        OmniSettingAction::SetDarkTheme,
                    ));
                }
                OmniCommandScope::SettingsLightThemes => {
                    results.extend(Self::omni_setting_choice_results(
                        ThemeChoice::ALL,
                        self.settings.values.light_theme,
                        query,
                        "Light color palette",
                        OmniSettingAction::SetLightTheme,
                    ));
                }
                OmniCommandScope::SettingsAccentColors => {
                    results.extend(Self::omni_setting_choice_results(
                        AppearanceColor::ALL,
                        self.settings.values.accent_color,
                        query,
                        "Accent color",
                        OmniSettingAction::SetAccentColor,
                    ));
                }
                OmniCommandScope::SettingsFonts => {
                    for font in self.settings.font_choices.iter().cloned() {
                        let title = font.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Font family"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if font == self.settings.values.font {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(OmniSettingAction::SetFont(
                                    font,
                                )),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Font family"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if font == self.settings.values.font {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(OmniSettingAction::SetFont(
                                    font,
                                )),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsFontSizes => {
                    for size in FONT_SIZES {
                        let title = size.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Font size"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if size == self.settings.values.font_size {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetFontSize(size),
                                ),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Font size"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if size == self.settings.values.font_size {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetFontSize(size),
                                ),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsUiDensity => {
                    for density in DENSITY_CHOICES {
                        let title = density.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("UI density"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if density == self.settings.values.ui_density {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetUiDensity(density),
                                ),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("UI density"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if density == self.settings.values.ui_density {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetUiDensity(density),
                                ),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsAiProviders => {
                    for provider in AI_PROVIDERS {
                        let title = provider.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("AI provider"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if provider == self.settings.values.ai_provider {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetAiProvider(provider),
                                ),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("AI provider"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if provider == self.settings.values.ai_provider {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetAiProvider(provider),
                                ),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsTableShortcuts => {
                    for shortcut in ShortcutBinding::table_options() {
                        let title = shortcut.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Table-search shortcut"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if shortcut == self.settings.values.omni_table_shortcut {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetOmniTableShortcut(shortcut.clone()),
                                ),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Table-search shortcut"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if shortcut == self.settings.values.omni_table_shortcut {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetOmniTableShortcut(shortcut.clone()),
                                ),
                                autocomplete: title,
                            });
                        }
                    }
                }
                OmniCommandScope::SettingsCommandShortcuts => {
                    for shortcut in ShortcutBinding::command_options() {
                        let title = shortcut.to_string();
                        if query.is_empty() {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Command-palette shortcut"),
                                category: String::from("Settings"),
                                match_indices: Vec::new(),
                                score: if shortcut == self.settings.values.omni_command_shortcut {
                                    30
                                } else {
                                    0
                                },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetOmniCommandShortcut(shortcut.clone()),
                                ),
                                autocomplete: title,
                            });
                        } else if let Some(matching) = fuzzy_match(&title, query) {
                            results.push(OmniResultItem {
                                title: title.clone(),
                                subtitle: String::from("Command-palette shortcut"),
                                category: String::from("Settings"),
                                match_indices: matching.indices,
                                score: matching.score
                                    + if shortcut == self.settings.values.omni_command_shortcut {
                                        30
                                    } else {
                                        0
                                    },
                                action: OmniResultAction::ApplySetting(
                                    OmniSettingAction::SetOmniCommandShortcut(shortcut.clone()),
                                ),
                                autocomplete: title,
                            });
                        }
                    }
                }
            },
        }

        if mode == MODE_TABLE_SEARCH && query.is_empty() {
            let selected_title = if self.connections.current.driver == DatabaseDriver::PostgreSql {
                self.workspace
                    .explorer
                    .selected_postgres_object
                    .as_ref()
                    .map(|object| object.qualified_name())
                    .or_else(|| self.workspace.selected_table.clone())
            } else {
                self.workspace.selected_table.clone()
            };

            if let Some(selected) = selected_title {
                let index = if self.connections.current.driver == DatabaseDriver::PostgreSql {
                    results
                        .iter()
                        .position(|item| item.title.eq_ignore_ascii_case(&selected))
                } else {
                    results.iter().position(|item| item.title == selected)
                };
                if let Some(index) = index {
                    let selected_item = results.remove(index);
                    results.insert(0, selected_item);
                }
            }
            return results;
        }

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.title.len().cmp(&right.title.len()))
                .then_with(|| left.title.cmp(&right.title))
        });

        results
    }
}
