#[cfg(test)]
mod tests {
    use crate::app::core::App;
    use crate::app::features::ai::types::{ChatBlockKind, ChatDirective, ChatMessage, ChatRole};
    use crate::app::features::settings;
    use crate::app::features::workspace::diagram::model::{DiagramState, DiagramTab, SchemaChange};
    use crate::app::features::workspace::tabs::TabEntry;
    use crate::app::message::Message;
    use crate::app::types::{DiagramAgentAction, LayoutMode, ToastLevel};
    use crate::app::update::{favorite_tags_input_within_limit, normalize_favorite_tags};
    use crate::constants::{
        AI_CHAT_DIAGRAM_NOTE_MARK, AI_CHAT_DIRECTIVE_PREFIX, AI_CHAT_MAX_AUTO_RETRIES,
        AI_CHAT_MAX_CONTEXT_TABLES, AI_CHAT_MAX_DIAGRAM_STEPS, AI_CHAT_MAX_HISTORY,
        AI_CHAT_RESULT_SAMPLE_ROWS, AI_CHAT_TITLE_MAX_CHARS, AI_TEST_OUTPUT_MAX_CHARS,
        RESPONSIVE_BREAKPOINT,
    };
    use crate::model::connection::{DatabaseDriver, StoredConnection};
    use crate::model::settings::{ChatMode, ShortcutBinding};
    use crate::model::table::{ColumnKind, ResultSet};
    use crate::model::transfer::TransferStage;
    use iced::widget::text_editor;
    use iced::{Size, keyboard};

    fn ai_context(app: &App) -> crate::app::features::ai::Context<'_> {
        crate::app::features::ai::Context {
            scope: "",
            settings: &app.settings.values,
            connection: &app.connections.current,
            database: None,
            pool: None,
            tables: &app.workspace.explorer.tables,
            selected_table: app.workspace.selected_table.as_deref(),
            query: &app.workspace.query.editor,
            query_error: None,
            query_running: false,
            role_modal_open: false,
            theme: app.theme(),
        }
    }

    #[test]
    fn open_import_modal_allows_sqlite() {
        let (mut app, _) = App::new();
        app.connections.current.driver = DatabaseDriver::Sqlite;

        let _ = app.update(Message::Transfer(
            crate::app::features::transfer::Message::OpenImportModal,
        ));

        assert!(app.transfer.import_modal_open);
        assert!(!app.transfer.export_modal_open);
        assert!(app.shell.error_modal.is_none());
        assert_eq!(app.transfer.import_stage, TransferStage::Configure);
    }

    #[test]
    fn open_export_modal_allows_sqlite() {
        let (mut app, _) = App::new();
        app.connections.current.driver = DatabaseDriver::Sqlite;

        let _ = app.update(Message::Transfer(
            crate::app::features::transfer::Message::OpenExportModal,
        ));

        assert!(app.transfer.export_modal_open);
        assert!(!app.transfer.import_modal_open);
        assert!(app.shell.error_modal.is_none());
        assert_eq!(app.transfer.export_stage, TransferStage::Configure);
    }

    #[test]
    fn driver_selection_updates_login_state_and_defaults() {
        let (mut app, _) = App::new();
        app.connections.driver_picker_open = true;
        app.connections.current.driver = DatabaseDriver::MySql;
        app.connections.current.port = DatabaseDriver::MySql.default_port().to_string();

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::DriverSelected(DatabaseDriver::PostgreSql),
        ));

        assert_eq!(app.connections.current.driver, DatabaseDriver::PostgreSql);
        assert_eq!(
            app.connections.current.port,
            DatabaseDriver::PostgreSql.default_port()
        );
        assert!(!app.connections.driver_picker_open);

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::DriverSelected(DatabaseDriver::Sqlite),
        ));

        assert_eq!(app.connections.current.driver, DatabaseDriver::Sqlite);
        assert_eq!(app.connections.current.database, "main");
    }

    #[test]
    fn driver_selection_is_ignored_while_connecting() {
        let (mut app, _) = App::new();
        app.connections.connecting = true;
        app.connections.current.driver = DatabaseDriver::MySql;

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::DriverSelected(DatabaseDriver::PostgreSql),
        ));

        assert_eq!(app.connections.current.driver, DatabaseDriver::MySql);
    }

    #[test]
    fn favorites_preserve_distinct_driver_identities() {
        let (mut app, _) = App::new();
        app.connections.current.host = String::from("db.example.com");
        app.connections.current.port = String::from("3306");
        app.connections.current.database = String::from("app");
        app.connections.current.username = String::from("root");
        app.connections.current.password.clear();

        app.connections.current.driver = DatabaseDriver::MySql;
        let mysql = StoredConnection::from_info(&app.connections.current);
        app.connections.current.driver = DatabaseDriver::MariaDb;
        let mariadb = StoredConnection::from_info(&app.connections.current);
        app.connections.favorites = vec![mysql, mariadb];

        assert_eq!(app.connections.favorites.len(), 2);
        assert!(
            app.connections
                .favorites
                .iter()
                .any(|entry| entry.driver == DatabaseDriver::MySql)
        );
        assert!(
            app.connections
                .favorites
                .iter()
                .any(|entry| entry.driver == DatabaseDriver::MariaDb)
        );
    }

    #[test]
    fn window_resize_switches_responsive_layout_modes() {
        let (mut app, _) = App::new();

        let _ = app.update(Message::Shell(crate::app::shell::Message::WindowResized(
            Size::new(RESPONSIVE_BREAKPOINT - 1.0, 720.0),
        )));
        assert_eq!(app.shell.layout_mode, LayoutMode::Compact);

        let _ = app.update(Message::Shell(crate::app::shell::Message::WindowResized(
            Size::new(RESPONSIVE_BREAKPOINT + 1.0, 720.0),
        )));
        assert_eq!(app.shell.layout_mode, LayoutMode::Wide);
        assert!(!app.shell.sidebar_hidden);
    }

    #[test]
    fn disconnect_resets_query_tabs_to_welcome() {
        let (mut app, _) = App::new();
        app.connections.connected = true;

        let mut state = app.blank_query_state();
        state.query = App::new_query_editor(
            "SELECT * FROM addresses;",
            &app.settings.values,
            app.settings.theme_choice,
        );
        app.open_new_query_tab(state);

        assert_eq!(app.workspace.tabs.query_tabs.len(), 2);
        assert_eq!(app.workspace.tabs.active_query_tab, Some(1));

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::Disconnect,
        ));

        assert!(!app.connections.connected);
        assert_eq!(app.workspace.tabs.query_tabs.len(), 1);
        assert_eq!(app.workspace.tabs.active_query_tab, Some(0));
        assert_eq!(app.workspace.tabs.tab_strip, vec![TabEntry::Query(0)]);
        assert_eq!(app.workspace.tabs.query_tabs[0].display_title(), "Welcome");
        assert_eq!(app.workspace.tabs.next_query_tab_number, 2);
        assert_eq!(
            app.workspace.query.editor.content(),
            app.connections.current.driver.default_query()
        );
    }

    #[test]
    fn disconnect_clears_chat_and_ai_work() {
        let (mut app, _) = App::new();
        app.connections.connected = true;
        app.ai.chat_open = true;
        app.ai.chat_input = iced::widget::text_editor::Content::with_text("describe this schema");
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("describe this schema"),
        );
        app.ai.chat_sending = true;
        app.ai.chat_activity = Some(String::from("Reading tables"));
        app.ai.chat_auto_run_pending = true;
        app.ai.chat_extra_tables.push(String::from("users"));
        app.ai.chat_diagram_steps = 1;

        let _ = app.update(Message::Connections(
            crate::app::features::connections::Message::Disconnect,
        ));

        assert!(!app.ai.chat_open);
        assert!(app.ai.chat_input.text().is_empty());
        assert!(app.ai.chat_sessions.is_empty());
        assert!(app.ai.chat_active_session.is_empty());
        assert!(!app.ai.chat_sending);
        assert!(app.ai.chat_activity.is_none());
        assert!(!app.ai.chat_auto_run_pending);
        assert!(app.ai.chat_extra_tables.is_empty());
        assert_eq!(app.ai.chat_diagram_steps, 0);
    }

    #[test]
    fn the_diagram_agent_stops_at_its_step_cap_without_erroring() {
        let (mut app, _) = App::new();
        app.connections.connected = true;
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("tidy the diagram"),
        );
        app.ai.chat_diagram_steps = AI_CHAT_MAX_DIAGRAM_STEPS;
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("New Diagram #1"),
            pinned: false,
            state: DiagramState::default(),
        });

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: app.chat_scope_key(),
                request_id: app.ai.chat_request_id,
                result: Ok(format!("{AI_CHAT_DIRECTIVE_PREFIX}diagram-layout")),
            },
        ));

        assert!(app.ai.chat_error.is_none());
        assert_eq!(app.ai.chat_diagram_steps, 0);
        assert!(app.workspace.diagram.diagram_tabs[0].state.agent.is_none());
        assert!(
            app.ai
                .chat_messages(&app.chat_scope_key())
                .last()
                .is_some_and(|message| message.content.contains("limit of diagram steps"))
        );
    }

    #[test]
    fn a_reply_for_another_connection_waits_in_that_connection() {
        let (mut app, _) = App::new();
        app.settings.values.multiple_connections_layout = true;
        app.connections.connected = true;
        app.connections.current.database = String::from("beta");
        app.ai.chat_request_id = 7;
        let beta = app.chat_scope_key();
        app.save_active_connection_tab_snapshot();
        app.connections.current.database = String::from("alpha");
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("describe alpha"),
        );

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: beta.clone(),
                request_id: 7,
                result: Ok(String::from("beta is done")),
            },
        ));

        let waiting = app
            .inactive_chat_state_mut(&beta)
            .and_then(|chat| chat.deferred_reply.take());
        assert!(matches!(waiting, Some((7, Ok(reply))) if reply == "beta is done"));
        assert!(
            app.ai
                .chat_messages(&app.chat_scope_key())
                .iter()
                .all(|message| message.role == ChatRole::User)
        );

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: beta.clone(),
                request_id: 9,
                result: Ok(String::from("stale")),
            },
        ));

        assert!(
            app.inactive_chat_state_mut(&beta)
                .is_some_and(|chat| chat.deferred_reply.is_none())
        );
    }

    #[test]
    fn a_diagram_result_from_another_database_is_ignored() {
        let (mut app, _) = App::new();
        app.connections.current.database = String::from("sqlite");
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("New Diagram #1"),
            pinned: false,
            state: DiagramState {
                loading: true,
                ..DiagramState::default()
            },
        });

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Diagram(
                crate::app::features::workspace::diagram::Message::SchemaDiagramLoaded {
                    diagram_id: 1,
                    connection_scope: app.chat_scope_key(),
                    database: String::from("halaxia"),
                    result: Err(String::from("old request")),
                },
            ),
        ));

        let state = &app.workspace.diagram.diagram_tabs[0].state;
        assert!(state.loading);
        assert!(state.error.is_none());
    }

    #[test]
    fn diagram_results_stay_with_their_connection_and_tab() {
        let (mut app, _) = App::new();
        app.connections.current.database = String::from("app");
        app.workspace.diagram.diagram_tabs = vec![
            DiagramTab {
                id: 1,
                title: String::from("A"),
                pinned: false,
                state: DiagramState {
                    applying: true,
                    changes: vec![SchemaChange::DropTable {
                        table: String::from("a"),
                    }],
                    ..DiagramState::default()
                },
            },
            DiagramTab {
                id: 2,
                title: String::from("B"),
                pinned: false,
                state: DiagramState {
                    changes: vec![SchemaChange::DropTable {
                        table: String::from("b"),
                    }],
                    ..DiagramState::default()
                },
            },
        ];
        app.workspace.diagram.active_diagram_tab = Some(1);
        let scope = app.chat_scope_key();

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Diagram(
                crate::app::features::workspace::diagram::Message::SchemaDiagramApplied {
                    diagram_id: 1,
                    connection_scope: scope.clone(),
                    result: Ok(1),
                },
            ),
        ));
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Diagram(
                crate::app::features::workspace::diagram::Message::SchemaDiagramLoaded {
                    diagram_id: 2,
                    connection_scope: String::from("old"),
                    database: String::from("app"),
                    result: Err(String::from("stale")),
                },
            ),
        ));

        assert!(!app.workspace.diagram.diagram_tabs[0].state.applying);
        assert!(
            app.workspace.diagram.diagram_tabs[0]
                .state
                .changes
                .is_empty()
        );
        assert_eq!(app.workspace.diagram.diagram_tabs[1].state.changes.len(), 1);
        assert!(app.workspace.diagram.diagram_tabs[1].state.error.is_none());
    }

    #[test]
    fn ai_query_generation_failure_preserves_final_error_detail() {
        let (mut app, _) = App::new();
        app.ai.is_generating_ai = true;
        let error = String::from("AI request failed (503): upstream unavailable (after retries)");

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::AiQueryGenerated(Err(error.clone())),
        ));

        assert!(!app.ai.is_generating_ai);
        assert_eq!(app.ai.ai_prompt_error.as_deref(), Some(error.as_str()));
    }

    #[test]
    fn chat_title_is_cleaned_and_capped() {
        assert_eq!(
            crate::app::features::ai::State::shorten_chat_title("  \"Candidate stages\".  "),
            "Candidate stages"
        );
        assert_eq!(
            crate::app::features::ai::State::shorten_chat_title("one\ntwo"),
            "one two"
        );

        let long = "a".repeat(AI_CHAT_TITLE_MAX_CHARS + 20);
        let shortened = crate::app::features::ai::State::shorten_chat_title(&long);
        assert!(shortened.ends_with("..."));
        assert_eq!(shortened.chars().count(), AI_CHAT_TITLE_MAX_CHARS + 3);
    }

    #[test]
    fn chat_title_reply_renames_only_the_matching_session() {
        let (mut app, _) = App::new();
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("first question"),
        );
        let scope = app.chat_scope_key();
        let session_id = app
            .ai
            .chat_active_session(&app.chat_scope_key())
            .unwrap()
            .id;

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatTitleReady {
                scope: scope.clone(),
                session_id: session_id.wrapping_add(99),
                result: Ok(String::from("Wrong session")),
            },
        ));
        assert_eq!(
            app.ai
                .chat_active_session(&app.chat_scope_key())
                .unwrap()
                .title,
            "first question"
        );

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatTitleReady {
                scope,
                session_id,
                result: Ok(String::from("Candidate stages")),
            },
        ));
        assert_eq!(
            app.ai
                .chat_active_session(&app.chat_scope_key())
                .unwrap()
                .title,
            "Candidate stages"
        );
    }

    #[test]
    fn chat_sessions_are_scoped_per_connection_and_database() {
        let (mut app, _) = App::new();
        app.connections.current.host = String::from("localhost");
        app.connections.current.username = String::from("root");
        app.connections.current.database = String::from("one");

        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("first database"),
        );
        assert_eq!(app.ai.chat_messages(&app.chat_scope_key()).len(), 1);

        app.connections.current.database = String::from("two");
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());

        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("second database"),
        );
        assert_eq!(app.ai.chat_messages(&app.chat_scope_key()).len(), 1);

        app.connections.current.database = String::from("one");
        assert_eq!(app.ai.chat_messages(&app.chat_scope_key()).len(), 1);
        assert_eq!(
            app.ai.chat_messages(&app.chat_scope_key())[0].content,
            "first database"
        );
    }

    #[test]
    fn connection_tabs_keep_chat_activity_isolated() {
        let (mut app, _) = App::new();
        app.settings.values.multiple_connections_layout = true;
        app.connections.connected = true;
        app.connections.current.driver = DatabaseDriver::Sqlite;
        app.connections.current.sqlite_path = String::from("/tmp/nolby.db");
        app.connections.current.database = String::from("main");
        app.ai.chat_open = true;
        app.ai.chat_input = text_editor::Content::with_text("hola");
        app.ai.chat_sending = true;
        app.ai.chat_request_id = 41;
        app.ai.next_chat_request_id = 41;
        let nolby_scope = app.chat_scope_key();
        app.ensure_active_connection_tab();
        let nolby = app.connections.active_tab_id.expect("Nolby tab");
        app.save_active_connection_tab_snapshot();

        app.connections.active_tab_id = None;
        app.connections.current.sqlite_path = String::from("/tmp/erdemo.db");
        app.ensure_active_connection_tab();
        let erdemo = app.connections.active_tab_id.expect("erdemo tab");

        assert!(!app.ai.chat_open);
        assert!(!app.ai.chat_sending);
        assert!(app.ai.chat_input.text().is_empty());

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyDelta {
                scope: nolby_scope,
                request_id: 41,
                text: String::from("¡Hola"),
            },
        ));
        let _ = app.switch_connection_tab(nolby);

        assert!(app.ai.chat_open);
        assert!(app.ai.chat_sending);
        assert_eq!(app.ai.chat_input.text(), "hola");
        assert_eq!(app.ai.chat_streaming_reply.as_deref(), Some("¡Hola"));

        let _ = app.switch_connection_tab(erdemo);
        assert!(!app.ai.chat_open);
        assert!(!app.ai.chat_sending);
        assert!(app.ai.chat_streaming_reply.is_none());
    }

    #[test]
    fn new_chat_session_starts_empty_and_keeps_the_old_one() {
        let (mut app, _) = App::new();
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("how many albums"),
        );

        app.ai.new_chat_session(&app.chat_scope_key());

        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
        assert_eq!(app.ai.chat_session_options(&app.chat_scope_key()).len(), 2);
        assert!(
            app.ai
                .chat_session_options(&app.chat_scope_key())
                .iter()
                .any(|option| option.label == "how many albums")
        );
    }

    #[test]
    fn chat_cancel_discards_the_in_flight_reply() {
        let (mut app, _) = App::new();
        app.ai.chat_sending = true;
        let stale = app.ai.chat_request_id;

        let _ = app.update(Message::Ai(crate::app::features::ai::Message::ChatCancel));
        assert!(!app.ai.chat_sending);

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: app.chat_scope_key(),
                request_id: stale,
                result: Ok(String::from("too late")),
            },
        ));
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
    }

    #[test]
    fn read_only_gate_rejects_writes_and_locking_reads() {
        assert!(crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT * FROM albums"
        ));
        assert!(crate::app::features::ai::State::chat_sql_is_read_only(
            "EXPLAIN SELECT 1"
        ));

        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT * FROM t FOR UPDATE"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT * FROM t FOR SHARE"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT * INTO other FROM t"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT 1; DROP TABLE t"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "PRAGMA journal_mode = WAL"
        ));
    }

    #[test]
    fn notes_do_not_evict_the_conversation_from_history() {
        let (mut app, _) = App::new();
        for index in 0..AI_CHAT_MAX_HISTORY {
            let _ = app.ai.push_chat_message(
                &app.chat_scope_key(),
                ChatRole::User,
                format!("question {index}"),
            );
        }
        for index in 0..4 {
            app.ai
                .chat_active_session_mut(&app.chat_scope_key())
                .messages
                .push(ChatMessage::note(
                    String::from("note"),
                    format!("result dump {index}"),
                ));
        }

        let history = app.ai.chat_history_messages(&app.chat_scope_key());
        let conversation = history
            .iter()
            .filter(|message| message.content.starts_with("question"))
            .count();

        assert_eq!(conversation, AI_CHAT_MAX_HISTORY);
    }

    #[test]
    fn detail_tables_match_whole_words_only() {
        let (mut app, _) = App::new();
        app.workspace.explorer.tables = vec![
            String::from("user"),
            String::from("candidates"),
            String::from("job_offers"),
        ];
        app.workspace
            .explorer
            .tables
            .extend((0..30).map(|index| format!("filler_table_{index}")));
        app.workspace.selected_table = None;

        let tables = app.ai.chat_detail_tables(
            &ai_context(&app),
            "how many candidate records mention offers",
        );
        assert!(tables.iter().any(|table| table == "candidates"));
        assert!(tables.iter().any(|table| table == "job_offers"));
        assert!(!tables.iter().any(|table| table == "user"));
    }

    #[test]
    fn ai_test_error_is_flattened_and_capped() {
        let noisy = format!(
            "`codex` failed: Reading prompt from stdin...\n\nERROR 401 Unauthorized\n{}",
            "x".repeat(AI_TEST_OUTPUT_MAX_CHARS)
        );

        let shortened = settings::State::shorten_ai_test_error(&noisy);

        assert!(!shortened.contains('\n'));
        assert!(shortened.ends_with("..."));
        assert_eq!(shortened.chars().count(), AI_TEST_OUTPUT_MAX_CHARS + 3);
        assert!(shortened.starts_with("`codex` failed"));
    }

    #[test]
    fn changing_provider_settings_clears_the_last_test_result() {
        let (mut app, _) = App::new();
        app.settings.ai_test_status = Some(Err(String::from("boom")));

        let _ = app.update(Message::Settings(
            crate::app::features::settings::Message::AiCliPresetSelected("claude -p"),
        ));

        assert!(app.settings.ai_test_status.is_none());
    }

    #[test]
    fn clear_chat_resets_the_title_so_the_next_message_renames_it() {
        let (mut app, _) = App::new();
        let _ = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("old conversation"),
        );
        assert_eq!(
            app.ai
                .chat_active_session(&app.chat_scope_key())
                .unwrap()
                .title,
            "old conversation"
        );

        let _ = app.update(Message::Ai(crate::app::features::ai::Message::ChatClear));
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
        assert_eq!(
            app.ai
                .chat_active_session(&app.chat_scope_key())
                .unwrap()
                .title,
            "New chat"
        );

        let renamed = app.ai.push_chat_message(
            &app.chat_scope_key(),
            ChatRole::User,
            String::from("brand new topic"),
        );
        assert!(renamed);
        assert_eq!(
            app.ai
                .chat_active_session(&app.chat_scope_key())
                .unwrap()
                .title,
            "brand new topic"
        );
    }

    #[test]
    fn each_mode_records_what_it_did_with_the_sql() {
        let reply = String::from("Here:\n\n```sql\nSELECT 1;\n```");
        let outcome = |mode: ChatMode, reply: &str| {
            let (mut app, _) = App::new();
            app.settings.values.chat_mode = mode;
            let request_id = app.ai.chat_request_id;
            let _ = app.update(Message::Ai(
                crate::app::features::ai::Message::ChatReplyReady {
                    scope: app.chat_scope_key(),
                    request_id,
                    result: Ok(reply.to_string()),
                },
            ));
            app.ai
                .chat_messages(&app.chat_scope_key())
                .iter()
                .rev()
                .find(|message| message.role == ChatRole::Note)
                .map(|message| message.summary.clone().unwrap_or_default())
                .unwrap_or_default()
        };

        assert!(outcome(ChatMode::Ask, &reply).contains("not put in the editor"));
        assert!(outcome(ChatMode::Draft, &reply).contains("not run"));
        assert!(outcome(ChatMode::AutoRun, &reply).contains("run"));

        let write = String::from("```sql\nDELETE FROM users;\n```");
        assert!(outcome(ChatMode::AutoRun, &write).contains("Not read-only"));
    }

    #[test]
    fn read_only_detection_covers_the_ways_a_write_can_hide_behind_select() {
        for sql in [
            "SELECT * FROM users",
            "WITH recent AS (SELECT 1) SELECT * FROM recent",
            "SHOW TABLES",
            "EXPLAIN SELECT 1",
        ] {
            assert!(
                crate::app::features::ai::State::chat_sql_is_read_only(sql),
                "should be read-only: {sql}"
            );
        }

        for sql in [
            "WITH moved AS (DELETE FROM a RETURNING *) INSERT INTO b SELECT * FROM moved",
            "WITH x AS (SELECT 1) UPDATE t SET a = 1",
            "CALL rebuild_totals()",
            "SELECT 1; DROP TABLE users",
            "SELECT * FROM users INTO OUTFILE '/tmp/x'",
            "SELECT * FROM users FOR UPDATE",
            "PRAGMA journal_mode = WAL",
            "SELECT setval('users_id_seq', 1)",
            "SELECT load_extension('/tmp/evil.so')",
            "SELECT dblink_exec('dbname=x', 'DROP TABLE users')",
            "SELECT lo_export(16384, '/tmp/x')",
            "SELECT pg_sleep(600)",
            "SELECT benchmark(10000000, md5('x'))",
        ] {
            assert!(
                !crate::app::features::ai::State::chat_sql_is_read_only(sql),
                "should not auto-run: {sql}"
            );
        }
    }

    #[test]
    fn chat_directive_parses_each_tool_strictly() {
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb schema candidates, `jobs`"),
            Some(ChatDirective::Schema(vec![
                String::from("candidates"),
                String::from("jobs"),
            ]))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive("Sure.\n`@cryodb sample albums`\n"),
            Some(ChatDirective::Sample(String::from("albums")))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb open-tab"),
            Some(ChatDirective::OpenTab)
        );
        assert!(matches!(
            crate::app::features::ai::State::chat_directive("@cryodb schema"),
            Some(ChatDirective::Malformed(_))
        ));
        assert!(matches!(
            crate::app::features::ai::State::chat_directive("@cryodb explode everything"),
            Some(ChatDirective::Malformed(_))
        ));
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb explain SELECT 1"),
            Some(ChatDirective::Explain(String::from("SELECT 1")))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb search email"),
            Some(ChatDirective::Search(String::from("email")))
        );
        assert!(matches!(
            crate::app::features::ai::State::chat_directive("@cryodb search a"),
            Some(ChatDirective::Malformed(_))
        ));
        assert_eq!(
            crate::app::features::ai::State::chat_directive("SELECT * FROM jobs"),
            None
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive("NEED_SCHEMA: jobs"),
            None
        );
    }

    #[test]
    fn chat_directive_parses_the_diagram_tools() {
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb diagram"),
            Some(ChatDirective::Diagram(DiagramAgentAction::Open))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive("@cryodb diagram-focus `orders`"),
            Some(ChatDirective::Diagram(DiagramAgentAction::Focus(
                String::from("orders")
            )))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-area Billing: orders, invoices"
            ),
            Some(ChatDirective::Diagram(DiagramAgentAction::Area {
                name: String::from("Billing"),
                tables: vec![String::from("orders"), String::from("invoices")],
            }))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-note orders: one row per order"
            ),
            Some(ChatDirective::Diagram(DiagramAgentAction::Note {
                table: String::from("orders"),
                text: String::from("one row per order"),
            }))
        );
        assert!(matches!(
            crate::app::features::ai::State::chat_directive("@cryodb diagram-note orders"),
            Some(ChatDirective::Malformed(_))
        ));
        assert!(matches!(
            crate::app::features::ai::State::chat_directive("@cryodb diagram-focus"),
            Some(ChatDirective::Malformed(_))
        ));
        assert_eq!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-add-table shipments: id INTEGER, label TEXT"
            ),
            Some(ChatDirective::Diagram(DiagramAgentAction::AddTable {
                table: String::from("shipments"),
                columns: String::from("id INTEGER, label TEXT"),
            }))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-add-column orders.note: TEXT"
            ),
            Some(ChatDirective::Diagram(DiagramAgentAction::AddColumn {
                table: String::from("orders"),
                column: String::from("note"),
                data_type: String::from("TEXT"),
            }))
        );
        assert_eq!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-link orders.customer_id: customers.id"
            ),
            Some(ChatDirective::Diagram(DiagramAgentAction::Link {
                table: String::from("orders"),
                column: String::from("customer_id"),
                referenced_table: String::from("customers"),
                referenced_column: String::from("id"),
            }))
        );
        assert!(matches!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-add-column orders: TEXT"
            ),
            Some(ChatDirective::Malformed(_))
        ));
        assert!(matches!(
            crate::app::features::ai::State::chat_directive(
                "@cryodb diagram-link orders.customer_id"
            ),
            Some(ChatDirective::Malformed(_))
        ));
    }

    #[test]
    fn the_diagram_agent_areas_notes_and_marks_where_it_worked() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![
                        (String::from("orders"), Vec::new()),
                        (String::from("customers"), Vec::new()),
                    ],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);

        let _ = app.diagram_agent_run(
            DiagramAgentAction::Area {
                name: String::from("Billing"),
                tables: vec![String::from("orders"), String::from("customers")],
            },
            String::from("group them"),
        );
        let _ = app.diagram_agent_run(
            DiagramAgentAction::Note {
                table: String::from("orders"),
                text: String::from("one row per order"),
            },
            String::from("note it"),
        );

        let state = app.active_diagram().expect("diagram");
        assert_eq!(state.diagram.groups.len(), 1);
        assert_eq!(state.diagram.groups[0].members.len(), 2);
        assert_eq!(state.diagram.notes.len(), 1);
        assert_eq!(state.agent.as_ref().map(|agent| agent.table), Some(0));
        assert!(app.diagram_report().contains("2 table(s)"));

        app.diagram_agent_clear();
        assert!(app.active_diagram().expect("diagram").agent.is_none());
    }

    #[test]
    fn the_diagram_agent_queues_schema_edits_instead_of_applying_them() {
        let column = |name: &str, unique: bool| crate::DiagramColumn {
            name: String::from(name),
            data_type: String::from("INTEGER"),
            primary: unique,
            foreign: false,
            nullable: false,
            default: None,
            indexed: unique,
            unique,
            attributes: String::new(),
        };
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![
                        (String::from("orders"), vec![column("customer_id", false)]),
                        (String::from("customers"), vec![column("id", true)]),
                    ],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);

        let _ = app.diagram_agent_run(
            DiagramAgentAction::AddTable {
                table: String::from("shipments"),
                columns: String::from("id INTEGER, label TEXT"),
            },
            String::from("add a shipments table"),
        );
        let _ = app.diagram_agent_run(
            DiagramAgentAction::AddColumn {
                table: String::from("orders"),
                column: String::from("note"),
                data_type: String::from("TEXT"),
            },
            String::from("add a note column"),
        );
        let _ = app.diagram_agent_run(
            DiagramAgentAction::Link {
                table: String::from("orders"),
                column: String::from("customer_id"),
                referenced_table: String::from("customers"),
                referenced_column: String::from("id"),
            },
            String::from("link them"),
        );

        let state = app.active_diagram().expect("diagram");
        assert_eq!(state.changes.len(), 3);
        assert!(matches!(
            &state.changes[0],
            SchemaChange::CreateTable { table, columns }
                if table == "shipments" && columns.len() == 2
        ));
        assert!(matches!(
            &state.changes[1],
            SchemaChange::AddColumn { table, column, data_type }
                if table == "orders" && column == "note" && data_type == "TEXT"
        ));
        assert!(matches!(
            &state.changes[2],
            SchemaChange::AddForeignKey { table, columns, referenced_table, .. }
                if table == "orders" && columns == &[String::from("customer_id")] && referenced_table == "customers"
        ));
        assert_eq!(state.diagram.edges.len(), 1);
        assert!(
            state
                .diagram
                .tables
                .iter()
                .any(|table| table.name == "shipments")
        );
        assert!(
            state.diagram.tables[0]
                .columns
                .iter()
                .any(|item| item.name == "note")
        );

        let _ = app.diagram_agent_run(
            DiagramAgentAction::Link {
                table: String::from("orders"),
                column: String::from("missing"),
                referenced_table: String::from("customers"),
                referenced_column: String::from("id"),
            },
            String::from("link a column that is not there"),
        );
        assert_eq!(app.active_diagram().expect("diagram").changes.len(), 3);
    }

    #[test]
    fn the_diagram_agent_never_draws_the_same_area_twice() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![
                        (String::from("orders"), Vec::new()),
                        (String::from("customers"), Vec::new()),
                        (String::from("audit_log"), Vec::new()),
                    ],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);
        let area = |tables: Vec<&str>| DiagramAgentAction::Area {
            name: String::from("Billing"),
            tables: tables.into_iter().map(String::from).collect(),
        };

        let _ = app.diagram_agent_run(area(vec!["orders", "customers"]), String::from("group"));
        let _ = app.diagram_agent_run(area(vec!["orders", "customers"]), String::from("group"));
        let _ = app.diagram_agent_run(area(vec!["orders"]), String::from("just orders"));

        let state = app.active_diagram().expect("diagram");
        assert_eq!(state.diagram.groups.len(), 1);
        assert_eq!(
            state.diagram.groups[0].members,
            vec![String::from("orders")]
        );

        let _ = app.diagram_agent_run(
            DiagramAgentAction::Lock {
                name: String::from("Billing"),
                locked: true,
            },
            String::from("lock it"),
        );
        let group = &app.active_diagram().expect("diagram").diagram.groups[0];
        assert!(group.locked && group.contents_locked);
    }

    #[test]
    fn the_diagram_agent_carries_each_table_into_the_area() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![
                        (String::from("orders"), Vec::new()),
                        (String::from("customers"), Vec::new()),
                    ],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);
        let before: Vec<_> = app
            .active_diagram()
            .expect("diagram")
            .diagram
            .tables
            .iter()
            .map(|table| table.position)
            .collect();

        let _ = app.diagram_agent_run(
            DiagramAgentAction::Area {
                name: String::from("Billing"),
                tables: vec![String::from("orders"), String::from("customers")],
            },
            String::from("group them"),
        );

        let state = app.active_diagram().expect("diagram");
        assert!(state.diagram.groups[0].members.len() == 2);
        assert_eq!(
            state.diagram.tables[0].position, before[0],
            "the pointer has not carried it anywhere yet"
        );
        assert!(app.diagram_agent_animating());

        for _ in 0..400 {
            app.diagram_agent_tick();
            if !app.diagram_agent_animating() {
                break;
            }
        }

        let state = app.active_diagram().expect("diagram");
        assert!(!app.diagram_agent_animating(), "the walk has to finish");
        let box_bounds = state.diagram.groups[0].bounds;
        for table in &state.diagram.tables {
            assert!(
                box_bounds.contains(table.bounds().center()),
                "{} never made it into the area",
                table.name
            );
        }
    }

    #[test]
    fn only_the_newest_diagram_note_keeps_its_report() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![(String::from("orders"), Vec::new())],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);
        app.settings.values.ai_enabled = true;
        app.settings.values.ai_endpoint = String::from("http://localhost:1234");
        app.settings.values.ai_model = String::from("test-model");

        for _ in 0..3 {
            let _ = app.diagram_agent_run(
                DiagramAgentAction::Focus(String::from("orders")),
                String::from("look at orders"),
            );
            app.ai.chat_diagram_last = None;
            app.ai.chat_sending = false;
        }

        let notes: Vec<String> = app
            .ai
            .chat_active_session_mut(&app.chat_scope_key())
            .messages
            .iter()
            .filter(|message| message.content.starts_with(AI_CHAT_DIAGRAM_NOTE_MARK))
            .map(|message| message.content.clone())
            .collect();
        assert!(notes.len() >= 2);
        assert_eq!(
            notes
                .iter()
                .filter(|content| content.contains("Diagram: "))
                .count(),
            1,
            "only the last step may carry the state of the canvas"
        );
    }

    #[test]
    fn the_diagram_agent_says_when_a_table_is_not_there() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Diagram"),
            pinned: false,
            state: DiagramState {
                loading: false,
                diagram: crate::SchemaDiagram::build(
                    vec![(String::from("orders"), Vec::new())],
                    &[],
                ),
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);

        let _ = app.diagram_agent_run(
            DiagramAgentAction::Focus(String::from("ghosts")),
            String::from("focus it"),
        );

        let state = app.active_diagram().expect("diagram");
        assert!(state.selected.is_none());
        assert!(state.agent.is_none());
    }

    #[test]
    fn chat_detail_tables_includes_every_table_for_small_schemas() {
        let (mut app, _) = App::new();
        app.workspace.explorer.tables = vec![String::from("jobs"), String::from("candidates")];
        app.workspace.selected_table = None;

        let tables = app.ai.chat_detail_tables(&ai_context(&app), "hola");

        assert_eq!(tables.len(), 2);
    }

    #[test]
    fn chat_detail_tables_picks_up_tables_named_in_plain_language() {
        let (mut app, _) = App::new();
        app.workspace.explorer.tables = vec![
            String::from("candidates"),
            String::from("candidate_process_env"),
            String::from("albums"),
        ];
        app.workspace.selected_table = Some(String::from("albums"));

        let tables = app
            .ai
            .chat_detail_tables(&ai_context(&app), "search candidates by environment 139");

        assert!(tables.iter().any(|table| table == "albums"));
        assert!(tables.iter().any(|table| table == "candidates"));
    }

    #[test]
    fn chat_detail_tables_is_capped() {
        let (mut app, _) = App::new();
        app.workspace.explorer.tables = (0..60).map(|index| format!("table_{index}")).collect();
        app.workspace.selected_table = None;
        let prompt = app.workspace.explorer.tables.join(" ");

        assert!(
            app.ai.chat_detail_tables(&ai_context(&app), &prompt).len()
                <= AI_CHAT_MAX_CONTEXT_TABLES
        );
    }

    #[test]
    fn chat_auto_retry_stops_at_the_cap() {
        let (mut app, _) = App::new();
        app.settings.values.ai_endpoint.clear();
        app.settings.values.ai_model.clear();
        app.connections.connected = true;
        app.ai.chat_auto_run_pending = true;
        app.ai.chat_auto_retries = AI_CHAT_MAX_AUTO_RETRIES;

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryFinished(Err(String::from(
                    "1054 (42S22): Unknown column 'cpe.environment' in 'where clause'",
                ))),
            ),
        ));

        assert!(!app.ai.chat_auto_run_pending);
        assert_eq!(app.ai.chat_auto_retries, AI_CHAT_MAX_AUTO_RETRIES);
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
    }

    #[test]
    fn chat_sql_read_only_accepts_single_select_and_rejects_writes() {
        assert!(crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT * FROM albums;"
        ));
        assert!(crate::app::features::ai::State::chat_sql_is_read_only(
            "-- comment\nWITH recent AS (SELECT 1) SELECT * FROM recent"
        ));
        assert!(crate::app::features::ai::State::chat_sql_is_read_only(
            "EXPLAIN SELECT 1"
        ));

        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "DELETE FROM albums"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "DROP TABLE albums"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "WITH removed AS (DELETE FROM albums RETURNING *) SELECT * FROM removed"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(
            "SELECT 1; DELETE FROM albums;"
        ));
        assert!(!crate::app::features::ai::State::chat_sql_is_read_only(""));
    }

    #[test]
    fn chat_result_summary_reports_shape_and_truncates() {
        let (mut app, _) = App::new();
        app.workspace.results.current = Some(std::sync::Arc::new(ResultSet {
            columns: vec![String::from("id"), String::from("name")],
            column_kinds: vec![ColumnKind::Integer, ColumnKind::Text],
            column_nullable: vec![false, true],
            rows: (0..9)
                .map(|index| vec![index.to_string(), format!("name-{index}")])
                .collect(),
        }));

        let summary = app.chat_result_summary().expect("summary");
        assert!(summary.starts_with("Rows returned: 9. Columns: id, name."));
        assert!(summary.contains("+4 more rows not shown"));
        assert_eq!(summary.matches(" | ").count(), AI_CHAT_RESULT_SAMPLE_ROWS);
    }

    #[test]
    fn chat_result_summary_handles_no_rows() {
        let (mut app, _) = App::new();
        app.workspace.results.current = Some(std::sync::Arc::new(ResultSet {
            columns: vec![String::from("id")],
            column_kinds: vec![ColumnKind::Integer],
            column_nullable: vec![false],
            rows: Vec::new(),
        }));

        let summary = app.chat_result_summary().expect("summary");
        assert!(summary.contains("returned no rows"));
    }

    #[test]
    fn typing_in_the_editor_turns_the_pagination_bindings_off() {
        let (mut app, _) = App::new();
        app.workspace.selected_table = Some(String::from("users"));
        let (display, _) = app.table_queries("users", 0);
        app.set_query_text(&display);
        app.workspace.query.table_query = Some(display.trim().to_string());
        assert!(app.is_table_query_active());

        app.set_query_text(&format!("{display} WHERE"));
        assert!(!app.is_table_query_active());
    }

    #[test]
    fn accepting_one_word_inserts_it_and_keeps_the_remainder() {
        let (mut app, _) = App::new();
        app.set_query_text("SELECT * FROM us");
        let _ = app.workspace.query.editor.set_cursor(0, usize::MAX);
        app.workspace.query.inline_suggestion = Some(String::from("ers WHERE id = 1"));

        app.apply_inline_query_suggestion_word();

        assert_eq!(
            app.workspace.query.editor.content().trim_end(),
            "SELECT * FROM users"
        );
        assert_eq!(
            app.workspace.query.inline_suggestion.as_deref(),
            Some(" WHERE id = 1")
        );
    }

    #[test]
    fn scrolling_the_editor_keeps_the_inline_suggestion() {
        let (mut app, _) = App::new();
        app.set_query_text("SELECT 1");
        app.workspace.query.inline_suggestion = Some(String::from(" FROM t"));

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(
                    iced_code_editor::Message::Tick,
                ),
            ),
        ));
        assert_eq!(
            app.workspace.query.inline_suggestion.as_deref(),
            Some(" FROM t")
        );

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(
                    iced_code_editor::Message::ArrowKey(
                        iced_code_editor::ArrowDirection::Left,
                        false,
                    ),
                ),
            ),
        ));
        assert_eq!(app.workspace.query.inline_suggestion, None);
    }

    #[test]
    fn query_editor_uses_code_editor_state() {
        let _guard = EDITOR_FOCUS_LOCK.lock();
        let (mut app, _) = App::new();
        app.set_query_text(
            &(1..=20)
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let _ = app.workspace.query.editor.set_cursor(19, usize::MAX);

        assert!(app.workspace.query.editor.line_numbers_enabled());
        assert_eq!(app.workspace.query.editor.cursor_position(), (19, 2));
    }

    static EDITOR_FOCUS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn query_editor_tab_indents_and_the_shortcut_opens_autocomplete() {
        let _guard = EDITOR_FOCUS_LOCK.lock();
        let (mut app, _) = App::new();
        app.connections.connected = true;
        app.settings.values.sql_keyword_autocomplete_enabled = true;
        app.settings.values.autocomplete_tables_shortcut =
            ShortcutBinding::default_autocomplete_tables();
        app.set_query_text("sel");
        app.workspace.query.editor.request_focus();
        let _ = app
            .workspace
            .query
            .editor
            .update(&iced_code_editor::Message::CanvasFocusGained);

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(
                    iced_code_editor::Message::Tab,
                ),
            ),
        ));
        let key = keyboard::Key::Named(keyboard::key::Named::Tab);
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryEditorCapturedKeyPressed(
                    keyboard::Event::KeyPressed {
                        key: key.clone(),
                        modified_key: key,
                        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
                        location: keyboard::Location::Standard,
                        modifiers: keyboard::Modifiers::empty(),
                        text: None,
                        repeat: false,
                    },
                ),
            ),
        ));

        assert_eq!(app.workspace.query.editor.content(), "    sel");
        assert!(!app.workspace.query.suggestions_open);

        let _ = app.workspace.query.editor.set_cursor(0, usize::MAX);
        let key = keyboard::Key::Named(keyboard::key::Named::Space);
        let _ = app.update(Message::Shell(
            crate::app::shell::Message::ClientKeyPressed(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Space),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::CTRL,
                text: None,
                repeat: false,
            }),
        ));

        assert!(app.workspace.query.suggestions_open);

        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(
                    iced_code_editor::Message::Enter,
                ),
            ),
        ));
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryEditorCapturedKeyPressed(
                    keyboard::Event::KeyPressed {
                        key: key.clone(),
                        modified_key: key,
                        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
                        location: keyboard::Location::Standard,
                        modifiers: keyboard::Modifiers::empty(),
                        text: None,
                        repeat: false,
                    },
                ),
            ),
        ));

        assert_eq!(app.workspace.query.editor.content(), "    SELECT");
        assert!(!app.workspace.query.suggestions_open);
    }

    #[test]
    fn escape_releases_the_query_editor_so_app_shortcuts_work_again() {
        let _guard = EDITOR_FOCUS_LOCK.lock();
        let (mut app, _) = App::new();
        app.connections.connected = true;
        app.workspace.query.editor.request_focus();
        let _ = app
            .workspace
            .query
            .editor
            .update(&iced_code_editor::Message::CanvasFocusGained);
        assert!(app.query_editor_focused());

        let key = keyboard::Key::Named(keyboard::key::Named::Escape);
        let _ = app.update(Message::Shell(
            crate::app::shell::Message::ClientKeyPressed(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
        ));

        assert!(!app.query_editor_focused());
    }

    #[test]
    fn query_editor_run_shortcut_does_not_insert_a_newline() {
        let _guard = EDITOR_FOCUS_LOCK.lock();
        let (mut app, _) = App::new();
        app.settings.values.run_selection_shortcut = ShortcutBinding::default_run_selection();
        app.set_query_text("SELECT 1");
        app.workspace.query.editor.request_focus();
        let _ = app
            .workspace
            .query
            .editor
            .update(&iced_code_editor::Message::CanvasFocusGained);

        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryAction(
                    iced_code_editor::Message::Enter,
                ),
            ),
        ));
        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let _ = app.update(Message::Workspace(
            crate::app::features::workspace::Message::Query(
                crate::app::features::workspace::query::Message::QueryEditorCapturedKeyPressed(
                    keyboard::Event::KeyPressed {
                        key: key.clone(),
                        modified_key: key,
                        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
                        location: keyboard::Location::Standard,
                        modifiers: keyboard::Modifiers::CTRL,
                        text: None,
                        repeat: false,
                    },
                ),
            ),
        ));

        assert_eq!(app.workspace.query.editor.content(), "SELECT 1");
        assert!(app.workspace.query.editor_pending_key_action.is_none());
    }

    #[test]
    fn result_followup_reply_never_auto_runs_again() {
        let (mut app, _) = App::new();
        app.settings.values.chat_mode = ChatMode::AutoRun;
        app.ai.chat_result_followup = true;
        let request_id = app.ai.chat_request_id;

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: app.chat_scope_key(),
                request_id,
                result: Ok(String::from("Here:\n\n```sql\nSELECT 1;\n```")),
            },
        ));

        assert!(!app.ai.chat_auto_run_pending);
        assert!(!app.ai.chat_result_followup);
    }

    #[test]
    fn chat_message_splits_prose_and_code_and_lifts_badges() {
        let reply = "**Warning:** dropping `public.address` is irreversible.\n\n```sql\nDROP TABLE public.address;\n```\n\nRun it yourself.";
        let message = ChatMessage::new(ChatRole::Assistant, String::from(reply));

        assert_eq!(message.blocks.len(), 3);
        assert_eq!(message.blocks[0].kind, ChatBlockKind::Prose);
        assert_eq!(message.blocks[0].badge.as_deref(), Some("Warning"));
        assert!(message.blocks[0].text.starts_with("dropping"));
        assert!(!message.blocks[0].text.contains("**"));

        assert_eq!(message.blocks[1].kind, ChatBlockKind::Code);
        assert_eq!(message.blocks[1].text, "DROP TABLE public.address;");

        assert_eq!(message.blocks[2].kind, ChatBlockKind::Prose);
        assert_eq!(message.blocks[2].text, "Run it yourself.");
    }

    #[test]
    fn chat_message_without_fences_is_one_prose_block() {
        let message = ChatMessage::new(ChatRole::Assistant, String::from("Just prose **here**."));

        assert_eq!(message.blocks.len(), 1);
        assert_eq!(message.blocks[0].kind, ChatBlockKind::Prose);
        assert_eq!(message.blocks[0].badge, None);
        assert_eq!(message.blocks[0].text, "Just prose here.");
    }

    #[test]
    fn chat_sql_blocks_extracts_fenced_sql_only() {
        let reply = "Here you go:\n\n```sql\nSELECT 1;\n```\n\nAnd some prose.\n\n```python\nprint(1)\n```\n\n```\nSELECT 2;\n```";
        let blocks = crate::app::features::ai::State::chat_sql_blocks(reply);

        assert_eq!(blocks, vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn chat_sql_blocks_ignores_unterminated_fence() {
        assert!(crate::app::features::ai::State::chat_sql_blocks("```sql\nSELECT 1;").is_empty());
        assert!(crate::app::features::ai::State::chat_sql_blocks("no fences here").is_empty());
    }

    #[test]
    fn chat_send_requires_a_configured_provider() {
        let (mut app, _) = App::new();
        app.settings.values.ai_provider = crate::ai::AiProvider::OpenAI;
        app.settings.values.ai_endpoint.clear();
        app.settings.values.ai_model.clear();
        app.settings.values.ai_cli_command.clear();
        app.ai.chat_input = text_editor::Content::with_text("list the tables");

        let _ = app.update(Message::Ai(crate::app::features::ai::Message::ChatSend));

        assert!(!app.ai.chat_sending);
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
        assert!(app.ai.chat_error.is_some());
    }

    #[test]
    fn chat_reply_from_stale_request_is_discarded() {
        let (mut app, _) = App::new();
        app.ai.chat_sending = true;
        app.ai.chat_request_id = 7;

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatReplyReady {
                scope: app.chat_scope_key(),
                request_id: 6,
                result: Ok(String::from("stale")),
            },
        ));

        assert!(app.ai.chat_sending);
        assert!(app.ai.chat_messages(&app.chat_scope_key()).is_empty());
    }

    #[test]
    fn ai_query_fix_failure_surfaces_final_error_in_toast() {
        let (mut app, _) = App::new();
        app.ai.is_fixing_query_with_ai = true;
        let error = String::from("AI request failed (429): rate limited (after retries)");

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::AiQueryFixFinished {
                source_query: String::from("SELECT 1"),
                result: Err(error.clone()),
            },
        ));

        assert!(!app.ai.is_fixing_query_with_ai);
        assert!(app.workspace.results.apply_error.is_none());
        let toast = app.shell.toasts.last().expect("error toast");
        assert_eq!(toast.level, ToastLevel::Error);
        assert_eq!(
            toast.message,
            crate::i18n::tr_with("AI fix failed: {error}", &[("{error}", &error)])
        );
    }

    #[test]
    fn favorite_tags_keep_ascii_tags_up_to_twelve_characters() {
        let tags = normalize_favorite_tags("prod, local-host12, ignored-taggg");

        assert_eq!(tags, vec!["prod", "local-host12"]);
    }

    #[test]
    fn favorite_tags_count_non_ascii_by_characters_not_bytes() {
        let tags = normalize_favorite_tags("데이터베이스태그12, 데이터베이스태그12345");

        assert_eq!(tags, vec!["데이터베이스태그12"]);
    }

    #[test]
    fn favorite_tags_input_rejects_tags_over_twelve_characters() {
        assert!(favorite_tags_input_within_limit("prod, local-host12"));
        assert!(!favorite_tags_input_within_limit("prod, ignored-taggg"));
    }
}
