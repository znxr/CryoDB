use crate::ai::AiProvider;
pub(crate) use crate::app::features::ai::types::AiModalTarget;
use crate::app::features::workspace::tabs::TabEntry;
use crate::model::connection::DatabaseDriver;
use crate::model::settings::{
    AppearanceColor, FontChoice, ShortcutBinding, SystemThemeMode, ThemeChoice, ThemeVariant,
    UiDensity,
};
use crate::model::table::PostgresSidebarObject;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiagramAgentAction {
    Open,
    Focus(String),
    Layout,
    Area {
        name: String,
        tables: Vec<String>,
    },
    Note {
        table: String,
        text: String,
    },
    Lock {
        name: String,
        locked: bool,
    },
    AddTable {
        table: String,
        columns: String,
    },
    AddColumn {
        table: String,
        column: String,
        data_type: String,
    },
    Link {
        table: String,
        column: String,
        referenced_table: String,
        referenced_column: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OmniBarMode {
    TableSearch,
    Command,
}

impl OmniBarMode {
    pub(crate) fn title(self, driver: DatabaseDriver) -> &'static str {
        match self {
            OmniBarMode::TableSearch => {
                if driver == DatabaseDriver::PostgreSql {
                    "Objects"
                } else {
                    "Table Search"
                }
            }
            OmniBarMode::Command => "Command Palette",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OmniCommandScope {
    Default,
    Databases,
    Drivers,
    FolderTableTargets,
    FolderDissolveTargets,
    FavoriteConnections,
    RecentConnections,
    TabSwitcher,
    FocusTargets,
    SettingsRoot,
    SettingsThemes,
    SettingsThemeModes,
    SettingsThemeVariants,
    SettingsManualThemes,
    SettingsDarkThemes,
    SettingsLightThemes,
    SettingsAccentColors,
    SettingsFonts,
    SettingsFontSizes,
    SettingsUiDensity,
    SettingsAiProviders,
    SettingsTableShortcuts,
    SettingsCommandShortcuts,
}

#[derive(Debug, Clone)]
pub(crate) struct OmniBarState {
    pub(crate) mode: OmniBarMode,
    pub(crate) command_scope: OmniCommandScope,
    pub(crate) query: String,
    pub(crate) selected_index: usize,
    pub(crate) scroll_offset_y: f32,
    pub(crate) results: Vec<OmniResultItem>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OmniCommandEntry {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) handler: OmniCommandHandler,
    pub(crate) category: &'static str,
    pub(crate) keywords: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OmniCommandHandler {
    DbConnect,
    DbSwitchDatabase,
    DbDisconnect,
    DbRefresh,
    DbReconnect,
    OpenPostgresTerminal,
    OpenSchemaDiagram,
    SelectDriver,
    NewQuery,
    DuplicateQueryTab,
    CloseQueryTab,
    RunQuery,
    FormatQuery,
    ExplainQuery,
    GenerateFolders,
    UngroupFolders,
    ExpandFolders,
    CollapseFolders,
    CreateFolder,
    MoveTableToFolder,
    RemoveFolder,
    SwitchTabs,
    HideSidebar,
    ShowSidebar,
    HideTabs,
    ShowTabs,
    HideQueryEditor,
    ShowQueryEditor,
    ToggleZenMode,
    FocusOn,
    FocusSidebar,
    FocusQueryEditor,
    FocusTableResults,
    FocusOmniBar,
    ClearQueryEditor,
    OpenFavoriteConnection,
    OpenRecentConnection,
    ReopenLastQuery,
    ClearRecentQueries,
    SwitchConnection,
    OpenSettingsCommands,
    OpenSettingsModal,
    OpenSettingsThemes,
    OpenSettingsFonts,
    OpenSettingsFontSizes,
    OpenSettingsUiDensity,
    OpenSettingsAiProviders,
    OpenSettingsTableShortcuts,
    OpenSettingsCommandShortcuts,
    CheckForUpdates,
    GenerateSql,
    FixQueryWithAi,
    OptimizeSql,
    ExplainSchema,
}

#[derive(Debug, Clone)]
pub(crate) struct OmniResultItem {
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) category: String,
    pub(crate) match_indices: Vec<usize>,
    pub(crate) score: i32,
    pub(crate) action: OmniResultAction,
    pub(crate) autocomplete: String,
}

#[derive(Debug, Clone)]
pub(crate) enum OmniResultAction {
    OpenCommandScope(OmniCommandScope),
    OpenTable(String),
    OpenPostgresObject(PostgresSidebarObject),
    ActivateTab(TabEntry),
    ConnectDatabase(String),
    SelectDriver(DatabaseDriver),
    OpenMoveTableToFolder(String),
    DissolveFolder(String),
    ConnectFavorite(usize),
    ConnectRecent(usize),
    ApplySetting(OmniSettingAction),
    RunCommand(OmniCommandHandler),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QueryEditorResizeDrag {
    pub(crate) start_cursor_y: f32,
    pub(crate) start_ratio: f32,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub(crate) enum OmniSettingAction {
    SetTheme(ThemeChoice),
    SetSystemThemeMode(SystemThemeMode),
    SetThemeVariant(ThemeVariant),
    SetDarkTheme(ThemeChoice),
    SetLightTheme(ThemeChoice),
    SetAccentColor(AppearanceColor),
    SetFont(FontChoice),
    SetFontSize(u32),
    SetUiDensity(UiDensity),
    SetTableQueryLimit(usize),
    SetHistoryLimit(usize),
    SetTabsEnabled(bool),
    SetOmniTableShortcut(ShortcutBinding),
    SetOmniCommandShortcut(ShortcutBinding),
    SetOpenSettingsShortcut(ShortcutBinding),
    SetSwitchDatabaseShortcut(ShortcutBinding),
    SetFocusTableSearchShortcut(ShortcutBinding),
    SetToggleTabPinShortcut(ShortcutBinding),
    SetCycleThemeShortcut(ShortcutBinding),
    SetRunQueryShortcut(ShortcutBinding),
    SetRunSelectionShortcut(ShortcutBinding),
    SetAutocompleteTablesShortcut(ShortcutBinding),
    SetOmniCommandAltShortcut(ShortcutBinding),
    SetAiProvider(AiProvider),
    SetAiEndpoint(String),
    SetAiModel(String),
    SetAiApiKey(String),
    SetOmniPrefix(String),
    SetEmphasizeColumnHeaders(bool),
    SetAutoScrollSidebarToSelectedTable(bool),
}

pub(crate) use crate::ui::presentation::LayoutMode;

#[derive(Debug, Clone)]
pub(crate) struct FavoriteContextMenuState {
    pub(crate) index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct FavoriteEditModalState {
    pub(crate) index: usize,
    pub(crate) name_input: String,
    pub(crate) tags_input: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResultsExportFormat {
    Csv,
    Json,
    Xlsx,
}

impl std::fmt::Display for ResultsExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl ResultsExportFormat {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            ResultsExportFormat::Csv => "csv",
            ResultsExportFormat::Json => "json",
            ResultsExportFormat::Xlsx => "xlsx",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ResultsExportFormat::Csv => "CSV",
            ResultsExportFormat::Json => "JSON",
            ResultsExportFormat::Xlsx => "XLSX",
        }
    }

    pub(crate) fn default_file_name(self) -> &'static str {
        match self {
            ResultsExportFormat::Csv => "results.csv",
            ResultsExportFormat::Json => "results.json",
            ResultsExportFormat::Xlsx => "results.xlsx",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastLevel {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastAction {
    Dismiss,
    OpenChangelog,
}

#[derive(Debug, Clone)]
pub(crate) struct ToastNotification {
    pub(crate) id: u64,
    pub(crate) level: ToastLevel,
    pub(crate) message: String,
    pub(crate) action: ToastAction,
    pub(crate) expires_at: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorModalState {
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) details: String,
}

impl ErrorModalState {
    pub(crate) fn new(
        title: impl Into<String>,
        summary: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            summary: summary.into(),
            details: details.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PaneKind {
    Sidebar,
    Editor,
    Results,
    Chat,
    Columns,
}
