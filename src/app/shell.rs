use crate::ai::AiRequestConfig;
use crate::app::core::App;
use crate::app::text_history::{TextEditFactory, TextHistory};
use crate::app::types::{
    ErrorModalState, LayoutMode, OmniBarMode, OmniBarState, OmniCommandEntry, PaneKind,
    ToastAction, ToastLevel, ToastNotification,
};
use crate::constants::{
    AI_CHAT_RESULT_CELL_MAX_CHARS, AI_CHAT_RESULT_SAMPLE_ROWS, CHANGELOG_TOAST_TIMEOUT_SECS,
    CHAT_SIDEBAR_MIN_WIDTH, DEVICON_FONT_FAMILY, ICON_LOADER_LINE, MODERN_COLUMNS_RATIO,
    PANE_MIN_SIZE, QUERY_EDITOR_MIN_HEIGHT, SIDEBAR_MIN_WIDTH, TOAST_TIMEOUT_SECS,
};
use crate::model::connection::{ConnectionInfo, DatabaseDriver, StoredConnection};
use crate::model::settings::{ShortcutBinding, ThemeChoice};
use iced::widget::{container, pane_grid, pick_list, scrollable, space, text};
use iced::{Background, Color, Element, Fill, Font, Length, Point, Size, Task, Theme, keyboard};
use std::f32::consts::TAU;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    GlobalCursorMoved(Point),
    OpenOmniBar(OmniBarMode),
    CloseOmniBar,
    OmniBarQueryChanged(String),
    OmniBarMoveSelection(i32),
    OmniBarAutocomplete,
    OmniBarSubmit,
    OmniBarResultPressed(usize),
    OmniBarResultsScrolled(scrollable::Viewport),
    ToggleSidebar,
    PaneResized(pane_grid::ResizeEvent),
    GlobalPointerReleased,
    ClientKeyPressed(keyboard::Event),
    WindowResized(Size),
    AiPulseTick,
    ToastTick,
    ToastDismissed(u64),
    ToggleZenMode,
    SetHoveredTooltip(String),
    ClearHoveredTooltip,
    TooltipTick,
    AnnounceChangelog,
    OpenChangelog,
    CloseChangelog,
    ChangelogReleaseToggled(usize),
    TextFieldEdited {
        id: iced::widget::Id,
        factory: TextEditFactory,
        previous: String,
        value: String,
    },
    UndoFocusedTextField,
    RedoFocusedTextField,
    UndoTextField(iced::widget::Id),
    RedoTextField(iced::widget::Id),
    CycleTheme,
    ModalBlocked,
    ToggleMoreOptions,
    CloseMoreOptions,
    CopyErrorModal,
    CloseErrorModal,
}

pub(crate) struct State {
    pub(crate) panes: pane_grid::State<PaneKind>,
    pub(crate) sidebar_split: pane_grid::Split,
    pub(crate) chat_split: pane_grid::Split,
    pub(crate) chat_ratio: f32,
    pub(crate) columns_split: pane_grid::Split,
    pub(crate) editor_split: pane_grid::Split,
    pub(crate) editor_split_collapsed: bool,
    pub(crate) sidebar_ratio: f32,
    pub(crate) editor_ratio: f32,
    pub(crate) editor_ratio_pinned: bool,
    pub(crate) columns_split_applied: f32,
    pub(crate) columns_ratio: f32,
    pub(crate) global_cursor: Option<Point>,
    pub(crate) loading_spinner_progress: f32,
    pub(crate) loading_spinner_frame: usize,
    pub(crate) hovered_tooltip_text: std::cell::RefCell<Option<String>>,
    pub(crate) tooltip_visible_since: Option<std::time::Instant>,
    pub(crate) toasts: Vec<ToastNotification>,
    pub(crate) next_toast_id: u64,
    pub(crate) text_history: TextHistory,
    pub(crate) layout_mode: LayoutMode,
    pub(crate) window_size: Size,
    pub(crate) omni_commands: Vec<OmniCommandEntry>,
    pub(crate) sidebar_hidden: bool,
    pub(crate) query_editor_hidden: bool,
    pub(crate) zen_mode: bool,
    pub(crate) omni_bar: Option<OmniBarState>,
    pub(crate) omni_bar_anim_progress: f32,
    pub(crate) omni_bar_closing: bool,
    pub(crate) database_switcher_open: bool,
    pub(crate) database_switcher_index: usize,
    pub(crate) error_modal: Option<ErrorModalState>,
    pub(crate) changelog_releases: Vec<&'static crate::ui::changelog::Release>,
    pub(crate) changelog_open: bool,
    pub(crate) changelog_expanded: usize,
}

impl State {
    pub(crate) fn new(
        panes: pane_grid::State<PaneKind>,
        sidebar_split: pane_grid::Split,
        chat_split: pane_grid::Split,
        columns_split: pane_grid::Split,
        editor_split: pane_grid::Split,
        omni_commands: Vec<OmniCommandEntry>,
    ) -> Self {
        Self {
            panes,
            sidebar_split,
            chat_split,
            chat_ratio: 0.68,
            columns_split,
            editor_split,
            editor_split_collapsed: false,
            sidebar_ratio: 0.24,
            editor_ratio: 0.35,
            editor_ratio_pinned: false,
            columns_split_applied: f32::NAN,
            columns_ratio: MODERN_COLUMNS_RATIO,
            global_cursor: None,
            loading_spinner_progress: 0.0,
            loading_spinner_frame: 0,
            hovered_tooltip_text: std::cell::RefCell::new(None),
            tooltip_visible_since: None,
            toasts: Vec::new(),
            next_toast_id: 1,
            text_history: TextHistory::default(),
            layout_mode: LayoutMode::Wide,
            window_size: Size::new(0.0, 0.0),
            omni_commands,
            sidebar_hidden: false,
            query_editor_hidden: false,
            zen_mode: false,
            omni_bar: None,
            omni_bar_anim_progress: 0.0,
            omni_bar_closing: false,
            database_switcher_open: false,
            database_switcher_index: 0,
            error_modal: None,
            changelog_releases: Vec::new(),
            changelog_open: false,
            changelog_expanded: 0,
        }
    }
}

impl App {
    pub(crate) fn presentation(&self) -> crate::ui::presentation::Presentation<'_> {
        crate::ui::presentation::Presentation {
            settings: &self.settings.values,
        }
    }

    pub(crate) fn persist_settings_store(&mut self) {
        self.settings.persist_settings_store()
    }

    pub(crate) fn push_toast(&mut self, level: ToastLevel, message: impl Into<String>) {
        self.push_toast_with(
            level,
            message,
            ToastAction::Dismiss,
            std::time::Duration::from_secs(TOAST_TIMEOUT_SECS),
        );
    }

    pub(crate) fn push_toast_with(
        &mut self,
        level: ToastLevel,
        message: impl Into<String>,
        action: ToastAction,
        lifetime: std::time::Duration,
    ) {
        let message = message.into();
        let message = message.trim();
        if message.is_empty() {
            return;
        }

        let toast_id = self.shell.next_toast_id;
        self.shell.next_toast_id = self.shell.next_toast_id.saturating_add(1);

        let expires_at = std::time::Instant::now() + lifetime;
        self.shell.toasts.insert(
            0,
            ToastNotification {
                id: toast_id,
                level,
                message: message.to_string(),
                action,
                expires_at,
            },
        );
    }

    pub(crate) fn announce_changelog(&mut self) {
        let pending = self.changelog_announcement();
        if self.onboarding.step.is_some() {
            return;
        }

        self.seal_changelog();
        if pending.is_empty() {
            return;
        }

        let headline = crate::i18n::tr_with(
            "CryoDB {version} — see what's new",
            &[("{version}", crate::ui::changelog::current_version())],
        );
        self.shell.changelog_releases = pending;
        self.push_toast_with(
            ToastLevel::Info,
            headline,
            ToastAction::OpenChangelog,
            std::time::Duration::from_secs(CHANGELOG_TOAST_TIMEOUT_SECS),
        );
    }

    pub(crate) fn changelog_announcement(&self) -> Vec<&'static crate::ui::changelog::Release> {
        if self.onboarding.step.is_some() {
            return Vec::new();
        }
        crate::ui::changelog::pending(self.settings.values.last_changelog_version.as_deref())
    }

    pub(crate) fn seal_changelog(&mut self) {
        let current = crate::ui::changelog::current_version();
        if self.settings.values.last_changelog_version.as_deref() == Some(current) {
            return;
        }
        self.settings.values.last_changelog_version = Some(current.to_string());
        self.persist_settings_store();
    }

    pub(crate) fn changelog_visible_releases(&self) -> &[&'static crate::ui::changelog::Release] {
        if self.shell.changelog_releases.is_empty() {
            crate::ui::changelog::all()
        } else {
            &self.shell.changelog_releases
        }
    }

    pub(crate) fn dismiss_toast(&mut self, toast_id: u64) {
        self.shell.toasts.retain(|toast| toast.id != toast_id);
    }

    pub(crate) fn prune_expired_toasts(&mut self) {
        let now = std::time::Instant::now();
        self.shell.toasts.retain(|toast| toast.expires_at > now);
    }

    pub(crate) fn flush_apply_feedback_toasts(&mut self) {
        let apply_error = self.workspace.results.apply_error.take();
        let apply_message = self.workspace.results.apply_message.take();

        if let Some(error) = apply_error {
            self.push_toast(ToastLevel::Error, error);
        }
        if let Some(message) = apply_message {
            let level = Self::toast_level_from_apply_message(&message);
            self.push_toast(level, message);
        }
    }

    pub(crate) fn toast_level_from_apply_message(message: &str) -> ToastLevel {
        let normalized = message.trim().to_ascii_lowercase();
        if normalized.starts_with("applied") {
            ToastLevel::Success
        } else {
            ToastLevel::Info
        }
    }

    pub(crate) fn saved_connection_for_tab_label(
        &self,
        entry: &StoredConnection,
    ) -> Option<&StoredConnection> {
        self.connections
            .favorites
            .iter()
            .chain(self.connections.recents.iter())
            .find(|saved| saved.matches_identity(entry))
    }

    pub(crate) fn leave_query_tab_context(&mut self) {
        if self.workspace.tabs.active_query_tab.is_none() {
            return;
        }
        self.save_active_query_tab();
        self.workspace.tabs.active_query_tab = None;
    }

    pub(crate) fn chat_available(&self) -> bool {
        self.settings.values.ai_enabled
            && AiRequestConfig::from_settings(&self.settings.values).is_usable()
    }

    pub(crate) fn chat_scope_prefix_for(connection: &ConnectionInfo) -> String {
        crate::app::features::ai::State::chat_scope_prefix_for(connection)
    }

    pub(crate) fn chat_scope_key_for(connection: &ConnectionInfo) -> String {
        crate::app::features::ai::State::chat_scope_key_for(connection)
    }

    pub(crate) fn chat_scope_key(&self) -> String {
        Self::chat_scope_key_for(&self.connections.current)
    }

    pub(crate) fn take_chat_connection_state(
        &mut self,
    ) -> crate::app::features::ai::ChatConnectionState {
        let state = self.ai.take_chat_connection_state();
        self.shell.panes.resize(self.shell.chat_split, 1.0);
        state
    }

    pub(crate) fn restore_chat_connection_state(
        &mut self,
        state: crate::app::features::ai::ChatConnectionState,
    ) {
        self.ai.restore_chat_connection_state(state);
        self.shell.panes.resize(
            self.shell.chat_split,
            if self.ai.chat_open {
                self.clamp_chat_ratio(self.shell.chat_ratio)
            } else {
                1.0
            },
        );
    }

    pub(crate) fn inactive_chat_state_mut(
        &mut self,
        scope: &str,
    ) -> Option<&mut crate::app::features::ai::ChatConnectionState> {
        self.connections.tabs.iter_mut().find_map(|tab| {
            let snapshot = tab.snapshot.as_mut()?;
            (Self::chat_scope_key_for(&snapshot.connection) == scope).then_some(&mut snapshot.chat)
        })
    }

    pub(crate) fn open_chat_sidebar(&mut self) {
        self.ai.chat_open = true;
        let ratio = self.clamp_chat_ratio(self.shell.chat_ratio);
        self.shell.panes.resize(self.shell.chat_split, ratio);
    }

    pub(crate) fn clamp_chat_ratio(&self, ratio: f32) -> f32 {
        let sidebar_ratio = if self.shell.sidebar_hidden || self.shell.zen_mode {
            0.0
        } else {
            self.clamp_sidebar_ratio(self.shell.sidebar_ratio)
        };
        let width = self.shell.window_size.width.max(1.0) * (1.0 - sidebar_ratio);
        let min = (self.scale_f32(SIDEBAR_MIN_WIDTH) / width).clamp(0.05, 0.95);
        let max = (1.0 - self.scale_f32(CHAT_SIDEBAR_MIN_WIDTH) / width).clamp(min, 0.95);
        ratio.clamp(min, max)
    }

    pub(crate) fn clamp_sidebar_ratio(&self, ratio: f32) -> f32 {
        let width = self.shell.window_size.width.max(1.0);
        let minimum = self.scale_f32(SIDEBAR_MIN_WIDTH)
            + if self.ai.chat_open {
                self.scale_f32(CHAT_SIDEBAR_MIN_WIDTH)
            } else {
                0.0
            };
        ratio.clamp(0.05, (1.0 - minimum / width).clamp(0.05, 0.95))
    }

    pub(crate) fn chat_result_summary(&self) -> Option<String> {
        let results = self.workspace.results.current.as_ref()?;
        let mut summary = format!(
            "Rows returned: {}. Columns: {}.",
            results.rows.len(),
            results.columns.join(", ")
        );

        if results.rows.is_empty() {
            summary.push_str("\nThe query returned no rows.");
            return Some(summary);
        }

        summary.push_str("\nFirst rows:");
        for row in results.rows.iter().take(AI_CHAT_RESULT_SAMPLE_ROWS) {
            let cells = row
                .iter()
                .map(|cell| {
                    let cell = cell.replace('\n', " ");
                    if cell.chars().count() > AI_CHAT_RESULT_CELL_MAX_CHARS {
                        format!(
                            "{}...",
                            cell.chars()
                                .take(AI_CHAT_RESULT_CELL_MAX_CHARS)
                                .collect::<String>()
                        )
                    } else {
                        cell
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ");
            summary.push_str(&format!("\n{cells}"));
        }

        if results.rows.len() > AI_CHAT_RESULT_SAMPLE_ROWS {
            summary.push_str(&format!(
                "\n(+{} more rows not shown)",
                results.rows.len() - AI_CHAT_RESULT_SAMPLE_ROWS
            ));
        }

        Some(summary)
    }

    pub(crate) fn send_chat_followup(&mut self, prompt: String) -> Task<crate::Message> {
        self.send_chat_note(None, prompt)
    }

    pub(crate) fn send_chat_note(
        &mut self,
        summary: Option<String>,
        prompt: String,
    ) -> Task<crate::Message> {
        self.update_ai(crate::app::features::ai::Message::SendNote { summary, prompt })
    }

    pub(crate) fn capture_shortcut_binding(
        &mut self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Task<crate::Message> {
        self.settings
            .capture_shortcut_binding(key, modifiers)
            .map(crate::Message::Settings)
    }

    pub(crate) fn text_history_shortcut(
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> Option<crate::Message> {
        if !ShortcutBinding::primary_modifier_only(modifiers) {
            return None;
        }
        let keyboard::Key::Character(character) = key else {
            return None;
        };
        if character.eq_ignore_ascii_case("z") {
            return Some(if modifiers.shift() {
                crate::Message::Shell(crate::app::shell::Message::RedoFocusedTextField)
            } else {
                crate::Message::Shell(crate::app::shell::Message::UndoFocusedTextField)
            });
        }
        if character.eq_ignore_ascii_case("y")
            && !modifiers.shift()
            && !ShortcutBinding::uses_command_key()
        {
            return Some(crate::Message::Shell(
                crate::app::shell::Message::RedoFocusedTextField,
            ));
        }
        None
    }

    pub(crate) fn is_sidebar_hidden(&self) -> bool {
        self.shell.zen_mode || self.shell.sidebar_hidden
    }

    pub(crate) fn is_query_editor_hidden(&self) -> bool {
        self.shell.query_editor_hidden || self.is_diagram_active()
    }

    pub(crate) fn has_active_modal(&self) -> bool {
        self.settings.settings_open
            || self.shell.database_switcher_open
            || self.is_omni_bar_visible()
            || self.workspace.results.text_modal_open
            || self.ai.ai_modal_open
            || self.workspace.explorer.table_info_sidebar_open
            || self.connections.postgres_terminal_open
            || self.workspace.explorer.table_modal.is_some()
            || self.workspace.explorer.postgres_role_modal.is_some()
            || self.transfer.import_modal_open
            || self.transfer.export_modal_open
            || self.shell.error_modal.is_some()
            || self.workspace.explorer.table_ddl.is_some()
            || self.workspace.tabs.tab_context_menu.is_some()
            || self.workspace.explorer.table_context_menu.is_some()
            || self
                .workspace
                .explorer
                .postgres_object_context_menu
                .is_some()
            || self.workspace.explorer.folder_context_menu.is_some()
    }

    pub(crate) fn split_workspace_height(&self) -> f32 {
        let window_height = if self.shell.window_size.height > 0.0 {
            self.shell.window_size.height
        } else {
            900.0
        };
        let chrome = if self.shell.zen_mode {
            self.scale_f32(24.0)
        } else {
            self.scale_f32(120.0)
        };
        (window_height - chrome).max(320.0)
    }

    pub(crate) fn clamp_editor_ratio(&self, ratio: f32) -> f32 {
        let workspace_height = self.split_workspace_height();
        let min_ratio = (QUERY_EDITOR_MIN_HEIGHT / workspace_height).clamp(0.05, 0.95);
        let max_ratio = (1.0 - (PANE_MIN_SIZE / workspace_height)).clamp(min_ratio, 0.95);
        ratio.clamp(min_ratio, max_ratio)
    }

    pub(crate) fn theme(&self) -> Theme {
        self.settings.theme()
    }

    pub(crate) fn ui_font(&self) -> Font {
        self.presentation().ui_font()
    }

    pub(crate) fn editor_font(&self) -> Font {
        crate::ui::presentation::font_for_choice(&self.settings.values.editor_font)
    }

    pub(crate) fn icon_font(&self) -> Font {
        self.presentation().icon_font()
    }

    pub(crate) fn font_scale(&self) -> f32 {
        self.presentation().font_scale()
    }

    pub(crate) fn ai_pulse_amount(&self) -> f32 {
        0.5 - 0.5 * (self.ai.ai_pulse_progress * TAU).cos()
    }

    pub(crate) fn has_active_loading_animation(&self) -> bool {
        self.connections.connecting
            || self.connections.loading_tables
            || self.connections.loading_databases
            || self.workspace.query.running
            || self.workspace.results.applying_changes
            || self.ai.is_generating_ai
            || self.ai.is_fixing_query_with_ai
            || self.workspace.explorer.table_info_loading
            || self.workspace.explorer.folder_generation_running
            || self.transfer.export_tables_loading
            || self.workspace.explorer.postgres_role_action_running
            || self.ai.chat_sending
    }

    pub(crate) fn loading_spinner_icon(&self) -> char {
        ICON_LOADER_LINE
    }

    pub(crate) fn modern(&self) -> bool {
        self.presentation().modern()
    }

    pub(crate) fn modern_status_word(&self) -> (&'static str, Color) {
        let theme = self.theme();
        let palette = theme.palette();
        if self.workspace.results.editing_cell.is_some() {
            ("Editing", palette.warning)
        } else if !self.workspace.results.pending_edits.is_empty() {
            ("Modified", palette.warning)
        } else {
            (
                "Ready",
                crate::ui::theme::appearance_color(
                    self.settings.values.accent_color,
                    palette.primary,
                ),
            )
        }
    }

    pub(crate) fn trigger_editor_visible(&self) -> bool {
        matches!(
            self.connections.current.driver,
            DatabaseDriver::MySql | DatabaseDriver::MariaDb
        ) && self.workspace.explorer.selected_trigger.is_some()
    }

    pub(crate) fn columns_panel_visible(&self) -> bool {
        self.modern() && self.workspace.diagram.active_diagram_tab.is_none()
    }

    pub(crate) fn region_gap(&self) -> f32 {
        if self.modern() {
            0.0
        } else {
            self.scale_u16(6)
        }
    }

    pub(crate) fn region_padding(&self) -> f32 {
        if self.modern() {
            0.0
        } else {
            self.scale_u16(6)
        }
    }

    pub(crate) fn inset_bar(&self, active: bool, vertical: bool) -> Element<'_, crate::Message> {
        let thickness = self.scale_f32(if vertical { 3.0 } else { 2.0 });
        let accent = crate::ui::theme::appearance_color(
            self.settings.values.accent_color,
            self.theme().palette().primary,
        );
        let bar = container(space::Space::new()).style(move |_| container::Style {
            background: Some(Background::Color(if active {
                accent
            } else {
                Color::TRANSPARENT
            })),
            ..container::Style::default()
        });

        if vertical {
            bar.width(Length::Fixed(thickness)).height(Fill).into()
        } else {
            bar.height(Length::Fixed(thickness)).width(Fill).into()
        }
    }

    pub(crate) fn tracked_caption(&self, content: &str) -> String {
        let content = crate::i18n::tr(content);
        if !self.modern() {
            return content;
        }
        content.to_uppercase()
    }

    pub(crate) fn scale_u16(&self, value: u16) -> f32 {
        self.presentation().scale_u16(value)
    }

    pub(crate) fn scale_f32(&self, value: f32) -> f32 {
        self.presentation().scale_f32(value)
    }

    pub(crate) fn button_text_size(&self) -> u32 {
        self.presentation().button_text_size()
    }

    pub(crate) fn icon_text_size(&self) -> u32 {
        self.presentation().icon_text_size()
    }

    pub(crate) fn sidebar_icon_size(&self) -> f32 {
        let icon_size = self.icon_text_size() as f32;
        if self.settings.values.large_sidebar_buttons {
            icon_size + self.scale_f32(1.0)
        } else {
            icon_size
        }
    }

    pub(crate) fn input_text_size(&self) -> u32 {
        self.presentation().input_text_size()
    }

    pub(crate) fn label_text_size(&self) -> u32 {
        self.presentation().label_text_size()
    }

    pub(crate) fn scale_padding(&self, padding: [u16; 2]) -> [f32; 2] {
        self.presentation().scale_padding(padding)
    }

    pub(crate) fn button_padding(&self) -> [f32; 2] {
        self.presentation().button_padding()
    }

    pub(crate) fn button_padding_tight(&self) -> [f32; 2] {
        self.presentation().button_padding_tight()
    }

    pub(crate) fn button_padding_icon(&self) -> [f32; 2] {
        self.presentation().button_padding_icon()
    }

    pub(crate) fn input_padding(&self) -> [f32; 2] {
        self.presentation().input_padding()
    }

    pub(crate) fn sidebar_list_spacing(&self) -> f32 {
        if self.modern() {
            return 0.0;
        }
        if self.settings.values.compact_sidebar {
            self.scale_u16(1)
        } else if self.settings.values.large_sidebar_buttons {
            self.scale_u16(5)
        } else {
            self.scale_u16(3)
        }
    }

    pub(crate) fn sidebar_button_padding(&self) -> [f32; 2] {
        if self.modern() {
            return self.scale_padding([8, 16]);
        }
        if self.settings.values.large_sidebar_buttons {
            self.scale_padding([8, 10])
        } else {
            self.button_padding()
        }
    }

    pub(crate) fn sidebar_button_padding_tight(&self) -> [f32; 2] {
        if self.settings.values.large_sidebar_buttons {
            self.scale_padding([7, 10])
        } else {
            self.button_padding_tight()
        }
    }

    pub(crate) fn pick_list_handle(&self) -> pick_list::Handle<Font> {
        self.presentation().pick_list_handle()
    }

    pub(crate) fn pick_list_max_chars(&self, width: f32) -> usize {
        self.presentation().pick_list_max_chars(width)
    }

    pub(crate) fn driver_marker(
        &self,
        driver: DatabaseDriver,
        size: f32,
    ) -> Element<'_, crate::Message> {
        self.driver_marker_in(driver, size, self.driver_marker_color(driver))
    }

    pub(crate) fn driver_marker_in(
        &self,
        driver: DatabaseDriver,
        size: f32,
        color: Color,
    ) -> Element<'_, crate::Message> {
        container(
            text(driver.icon())
                .font(Font::with_name(DEVICON_FONT_FAMILY))
                .size(size)
                .color(color)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .into()
    }

    pub(crate) fn driver_marker_color(&self, _driver: DatabaseDriver) -> Color {
        let background = self.theme().palette().background;
        if (0.299 * background.r + 0.587 * background.g + 0.114 * background.b) < 0.5 {
            Color::WHITE
        } else {
            Color::BLACK
        }
    }

    pub(crate) fn label_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().label_text(content)
    }

    pub(crate) fn muted_label_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().muted_label_text(content)
    }

    pub(crate) fn header_label_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        let font = if self.settings.values.emphasize_column_headers {
            Font {
                weight: iced::font::Weight::Bold,
                ..self.ui_font()
            }
        } else {
            self.ui_font()
        };
        text(crate::i18n::tr(&content.into()))
            .font(font)
            .size(self.label_text_size())
    }

    pub(crate) fn button_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().button_text(content)
    }

    pub(crate) fn icon_text(&self, icon: char) -> iced::widget::Text<'_> {
        self.presentation().icon_text(icon)
    }

    pub(crate) fn action_label(
        &self,
        label: &str,
        icon: char,
        layout: LayoutMode,
    ) -> Element<'_, crate::Message> {
        self.presentation().action_label(label, icon, layout)
    }

    pub(crate) fn input_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().input_text(content)
    }

    pub(crate) fn title_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().title_text(content)
    }

    pub(crate) fn heading_text(&self, content: impl Into<String>) -> iced::widget::Text<'_> {
        self.presentation().heading_text(content)
    }

    pub(crate) fn next_theme(&self) -> ThemeChoice {
        let themes = ThemeChoice::ALL;
        if themes.is_empty() {
            return self.settings.theme_choice;
        }

        let index = themes
            .iter()
            .position(|theme| *theme == self.settings.theme_choice)
            .unwrap_or(0);
        themes[(index + 1) % themes.len()]
    }
}
