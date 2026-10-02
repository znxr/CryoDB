#[cfg(test)]
use crate::app::core::App;
#[cfg(test)]
use crate::app::features::workspace::tabs::TabEntry;
#[cfg(test)]
use crate::app::message::Message;
#[cfg(test)]
use crate::app::types::{OmniCommandHandler, ToastAction, ToastLevel};
#[cfg(test)]
use crate::constants::CHANGELOG_TOAST_TIMEOUT_SECS;
#[cfg(test)]
use crate::model::connection::DatabaseDriver;
#[cfg(test)]
use crate::model::table::{ColumnKind, ResultSet, TableCacheEntry, TableRelationFilter};
#[cfg(test)]
use iced_code_editor::Message as CodeEditorMessage;

pub(crate) fn load_font_choices_task() -> iced::Task<crate::app::message::Message> {
    crate::app::features::settings::effects::load_font_choices_task()
        .map(crate::app::message::Message::Settings)
}
#[cfg(test)]
use crate::app::features::settings::effects::settings_store_for_persistence;

#[cfg(test)]
mod changelog_announcement_tests {
    use super::*;

    fn app_with_baseline(baseline: Option<&str>) -> App {
        let (mut app, _) = App::new();
        app.onboarding.step = None;
        app.settings.values.last_changelog_version = baseline.map(str::to_string);
        app
    }

    #[test]
    fn a_feature_upgrade_is_announced() {
        let app = app_with_baseline(Some("0.1.1"));
        let announced = app.changelog_announcement();

        assert!(!announced.is_empty());
        assert!(
            announced
                .iter()
                .any(|release| release.version == crate::ui::changelog::current_version())
        );
    }

    #[test]
    fn the_same_version_is_not_announced() {
        let app = app_with_baseline(Some(crate::ui::changelog::current_version()));
        assert!(app.changelog_announcement().is_empty());
    }

    #[test]
    fn onboarding_suppresses_the_announcement() {
        let mut app = app_with_baseline(Some("0.1.1"));
        app.onboarding.step = Some(0);
        assert!(app.changelog_announcement().is_empty());
    }

    #[test]
    fn a_toast_can_carry_the_notes_instead_of_dismissing() {
        let (mut app, _) = App::new();
        app.push_toast_with(
            ToastLevel::Info,
            "CryoDB 9.9.9 — see what's new",
            ToastAction::OpenChangelog,
            std::time::Duration::from_secs(CHANGELOG_TOAST_TIMEOUT_SECS),
        );

        let toast = app.shell.toasts.first().expect("toast");
        assert_eq!(toast.action, ToastAction::OpenChangelog);
        assert_eq!(toast.level, ToastLevel::Info);
    }

    #[test]
    fn the_modal_falls_back_to_the_full_history() {
        let (app, _) = App::new();
        assert!(app.shell.changelog_releases.is_empty());
        assert_eq!(
            app.changelog_visible_releases().len(),
            crate::ui::changelog::releases().len()
        );
    }
}

#[cfg(test)]
mod database_session_tests {
    use super::*;
    use std::sync::Arc;

    fn configured_app(database: &str) -> App {
        let (mut app, _) = App::new();
        app.connections.connected = true;
        app.connections.current.driver = DatabaseDriver::MySql;
        app.connections.current.host = String::from("db.local");
        app.connections.current.port = String::from("3306");
        app.connections.current.username = String::from("cryo");
        app.connections.current.database = database.to_string();
        app
    }

    #[test]
    fn database_session_key_includes_database_when_identity_changes() {
        let app = configured_app("alpha");

        let alpha = app.database_session_key_for("alpha");
        let beta = app.database_session_key_for("beta");

        assert_ne!(alpha, beta);
        assert_eq!(alpha, app.current_database_session_key().unwrap());
    }

    #[test]
    fn the_last_workspace_tab_cannot_be_closed() {
        let mut app = configured_app("alpha");

        let _ = app.close_query_tab(0);

        assert_eq!(app.workspace.tabs.query_tabs.len(), 1);
        assert_eq!(app.workspace.tabs.tab_strip, vec![TabEntry::Query(0)]);
    }

    #[test]
    fn database_session_restores_tabs_and_results_when_saved() {
        let mut app = configured_app("alpha");
        let session_key = app.current_database_session_key().unwrap();
        app.set_query_text("select 1");
        app.workspace.tabs.next_query_tab_number = 7;
        app.workspace.tabs.open_tables.push(String::from("users"));
        app.workspace
            .tabs
            .tab_strip
            .push(TabEntry::Table(String::from("users")));
        app.workspace.selected_table = Some(String::from("users"));
        let results = Arc::new(ResultSet {
            columns: vec![String::from("id")],
            column_kinds: vec![ColumnKind::Integer],
            column_nullable: vec![false],
            rows: vec![vec![String::from("1")]],
        });
        app.workspace.results.current = Some(results.clone());
        app.workspace.table_cache.insert(
            (String::from("users"), 0),
            TableCacheEntry {
                results,
                has_next_page: false,
            },
        );

        app.save_current_database_session();
        app.reset_database_workspace();
        let session = app
            .connections
            .database_sessions
            .remove(&session_key)
            .unwrap();
        app.restore_database_session(session);

        assert_eq!(app.workspace.query.editor.content(), "select 1");
        assert_eq!(app.workspace.tabs.next_query_tab_number, 7);
        assert_eq!(app.workspace.tabs.open_tables, vec![String::from("users")]);
        assert!(
            app.workspace
                .tabs
                .tab_strip
                .iter()
                .any(|entry| matches!(entry, TabEntry::Table(table) if table == "users"))
        );
        assert_eq!(
            app.workspace.results.current.as_ref().unwrap().rows[0][0].as_str(),
            "1"
        );
        assert!(
            app.workspace
                .table_cache
                .contains_key(&(String::from("users"), 0))
        );

        app.open_new_query_tab(app.blank_query_state());
        assert_eq!(
            app.workspace
                .tabs
                .query_tabs
                .last()
                .unwrap()
                .display_title(),
            "Query #7"
        );
        assert_eq!(app.workspace.tabs.next_query_tab_number, 8);
    }

    #[test]
    fn disconnect_clears_saved_database_sessions() {
        let mut app = configured_app("alpha");
        app.set_query_text("select 1");
        app.save_current_database_session();

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::Disconnect,
        ));

        assert!(app.connections.database_sessions.is_empty());
    }

    #[test]
    fn sql_keyword_suggestions_appear_when_schema_autocomplete_is_disabled() {
        let (mut app, _) = App::new();
        app.settings.values.schema_autocomplete_enabled = false;
        app.settings.values.sql_keyword_autocomplete_enabled = true;

        let suggestions = app.build_query_suggestions("sel");

        assert!(suggestions.iter().any(|suggestion| suggestion == "SELECT"));
    }

    #[test]
    fn sql_keyword_suggestions_are_hidden_when_sql_keyword_autocomplete_is_disabled() {
        let (mut app, _) = App::new();
        app.settings.values.sql_keyword_autocomplete_enabled = false;

        let suggestions = app.build_query_suggestions("sel");

        assert!(!suggestions.iter().any(|suggestion| suggestion == "SELECT"));
    }

    fn ai_key_store_succeeds(_api_key: &str) -> Result<(), String> {
        Ok(())
    }

    fn ai_key_store_fails(_api_key: &str) -> Result<(), String> {
        Err(String::from("keyring unavailable"))
    }

    #[test]
    fn settings_store_keeps_ai_api_key_when_keyring_write_succeeds() {
        let (mut app, _) = App::new();
        app.settings.values.ai_api_key = String::from("sk-secret");

        let (store, keyring_error) = settings_store_for_persistence(
            &app.settings.values,
            app.settings.theme_choice,
            ai_key_store_succeeds,
        );

        assert_eq!(keyring_error, None);
        assert_eq!(store.ai_api_key, Some(String::from("sk-secret")));
    }

    #[test]
    fn settings_store_keeps_ai_api_key_fallback_when_keyring_write_fails() {
        let (mut app, _) = App::new();
        app.settings.values.ai_api_key = String::from("sk-secret");

        let (store, keyring_error) = settings_store_for_persistence(
            &app.settings.values,
            app.settings.theme_choice,
            ai_key_store_fails,
        );

        assert_eq!(keyring_error, Some(String::from("keyring unavailable")));
        assert_eq!(store.ai_api_key, Some(String::from("sk-secret")));
    }

    #[test]
    fn query_inline_suggestion_idle_check_uses_configured_delay() {
        let (mut app, _) = App::new();
        app.settings.values.query_inline_suggestion_delay_ms = 1_000;

        let recent_edit = std::time::Instant::now() - std::time::Duration::from_millis(750);
        let stale_edit = std::time::Instant::now() - std::time::Duration::from_millis(1_250);

        assert!(!app.query_inline_suggestion_idle_elapsed(recent_edit));
        assert!(app.query_inline_suggestion_idle_elapsed(stale_edit));
    }

    #[test]
    fn query_editor_palette_exposes_cryodb_commands() {
        let (app, _) = App::new();
        assert!(app.workspace.query.editor.command_palette_enabled());
        assert_eq!(
            app.workspace
                .query
                .editor
                .custom_command_palette_entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["cryodb.format", "cryodb.command_palette"]
        );
    }

    #[test]
    fn query_editor_palette_format_uses_native_undo() {
        let (mut app, _) = App::new();
        app.set_query_text("select * from users where id = 1");

        let _ = app.execute_omni_command(OmniCommandHandler::FormatQuery);
        assert_eq!(
            app.workspace.query.editor.content(),
            "SELECT * FROM users WHERE id = 1"
        );
        let _ = app.workspace.query.editor.update(&CodeEditorMessage::Undo);
        assert_eq!(
            app.workspace.query.editor.content(),
            "select * from users where id = 1"
        );
    }

    #[test]
    fn query_editor_palette_applies_autocomplete_as_editor_edit() {
        let (mut app, _) = App::new();
        app.settings.values.sql_keyword_autocomplete_enabled = true;
        app.set_query_text("sel");
        let _ = app.workspace.query.editor.set_cursor(0, usize::MAX);
        app.workspace.query.suggestions_open = true;
        app.refresh_query_suggestions();
        let index = app
            .workspace
            .query
            .suggestions
            .iter()
            .position(|suggestion| suggestion == "SELECT")
            .expect("SELECT suggestion");

        let _ = app.apply_query_suggestion(index);
        assert_eq!(app.workspace.query.editor.content(), "SELECT");
        let _ = app.workspace.query.editor.update(&CodeEditorMessage::Undo);
        assert_eq!(app.workspace.query.editor.content(), "sel");
    }
}

#[cfg(test)]
mod inline_suggestion_cache_tests {
    use super::*;

    fn autocomplete_app(text: &str) -> App {
        let (mut app, _) = App::new();
        app.settings.values.ai_enabled = true;
        app.settings.values.ai_provider = crate::ai::AiProvider::OpenAI;
        app.settings.values.ai_autocomplete_enabled = true;
        app.settings.values.ai_autocomplete_use_main_provider = true;
        app.settings.values.ai_endpoint = String::from("https://example.test/v1");
        app.settings.values.ai_model = String::from("test-model");
        app.set_query_text(text);
        let _ = app
            .workspace
            .query
            .editor
            .set_cursor(text.lines().count().saturating_sub(1), usize::MAX);
        app
    }

    #[test]
    fn a_cached_answer_is_served_without_waiting_for_the_idle_delay() {
        let mut app = autocomplete_app("SELECT * FROM us");
        let anchor = app.workspace.query.editor.content();
        let offset = app.query_cursor_byte_offset().expect("cursor offset");
        app.workspace.query.inline_suggestion_cache =
            Some((anchor, offset, String::from("ers WHERE id = 1")));

        app.workspace.query.inline_suggestion_last_edit_at = Some(std::time::Instant::now());
        app.workspace.query.inline_suggestion_pending = true;

        let _ = app.request_inline_ai_suggestion();

        assert_eq!(
            app.workspace.query.inline_suggestion.as_deref(),
            Some("ers WHERE id = 1")
        );
        assert!(
            !app.workspace.query.inline_suggestion_loading,
            "no request was sent"
        );
        assert!(!app.workspace.query.inline_suggestion_pending);
    }

    #[test]
    fn typing_into_the_cached_answer_serves_only_the_remainder() {
        let mut app = autocomplete_app("SELECT * FROM us");
        let anchor = app.workspace.query.editor.content();
        let offset = app.query_cursor_byte_offset().expect("cursor offset");
        app.workspace.query.inline_suggestion_cache = Some((anchor, offset, String::from("ers")));

        app.replace_query_buffer("SELECT * FROM use");
        app.workspace.query.inline_suggestion_pending = true;

        let _ = app.request_inline_ai_suggestion();

        assert_eq!(app.workspace.query.inline_suggestion.as_deref(), Some("rs"));
    }

    #[test]
    fn a_cache_from_another_query_is_not_reused() {
        let mut app = autocomplete_app("SELECT * FROM orders");
        app.workspace.query.inline_suggestion_cache =
            Some((String::from("SELECT * FROM us"), 16, String::from("ers")));
        app.workspace.query.inline_suggestion_pending = true;

        let _ = app.request_inline_ai_suggestion();

        assert_eq!(app.workspace.query.inline_suggestion, None);
    }
}

#[cfg(test)]
mod results_pagination_tests {
    use super::*;

    fn app_with_page_size(limit: usize) -> App {
        let (mut app, _) = App::new();
        app.settings.values.table_query_limit = limit;
        app
    }

    #[test]
    fn a_partly_filled_final_page_is_still_the_last_one() {
        let app = app_with_page_size(100);

        assert_eq!(app.last_page_for_row_count(250), 2);
    }

    #[test]
    fn a_table_that_ends_exactly_on_a_page_boundary_has_no_empty_page_after_it() {
        let app = app_with_page_size(100);

        assert_eq!(app.last_page_for_row_count(200), 1);
    }

    #[test]
    fn a_single_row_stays_on_the_first_page() {
        let app = app_with_page_size(100);

        assert_eq!(app.last_page_for_row_count(1), 0);
    }

    #[test]
    fn a_row_filter_narrows_the_count_the_same_way_it_narrows_the_page() {
        let mut app = app_with_page_size(100);
        app.workspace.table_filters.insert(
            String::from("orders"),
            TableRelationFilter {
                column: String::from("customer_id"),
                value: String::from("7"),
            },
        );

        let count = app.table_count_query("orders");

        assert!(count.starts_with("SELECT COUNT(*) FROM"));
        assert!(count.contains("customer_id"));
        assert!(count.contains("'7'"));
    }
}
