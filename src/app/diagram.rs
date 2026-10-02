use crate::app::core::App;
use crate::app::features::workspace::diagram::model::DiagramState;
use crate::app::features::workspace::tabs::TabEntry;
use crate::app::message::Message;
use crate::app::types::DiagramAgentAction;
use crate::constants::AI_CHAT_DIRECTIVE_PREFIX;
use crate::model::connection::StoredConnection;
use iced::Task;

impl App {
    fn apply_diagram_output(
        &mut self,
        output: crate::app::features::workspace::diagram::Output,
    ) -> Task<Message> {
        use crate::app::features::workspace::diagram::Output;
        match output {
            Output::AgentStep { summary, prompt } => {
                return self.diagram_agent_note(summary, prompt);
            }
            Output::ChatNote(summary, prompt) => return self.send_chat_note(summary, prompt),
            Output::Tooltip(text) => {
                return self.update(match text {
                    Some(text) => {
                        Message::Shell(crate::app::shell::Message::SetHoveredTooltip(text))
                    }
                    None => Message::Shell(crate::app::shell::Message::ClearHoveredTooltip),
                });
            }
            Output::Loaded => {
                if let Some(prompt) = self.ai.chat_diagram_prompt.take() {
                    return self.diagram_agent_run(DiagramAgentAction::Open, prompt);
                }
            }
            Output::HideTools => self.workspace.explorer.sidebar_tools_open = false,
            Output::TabAdded(index) => self.workspace.tabs.tab_strip.push(TabEntry::Diagram(index)),
            Output::TabActivated(index) => {
                return self.scroll_tab_entry_into_view(&TabEntry::Diagram(index));
            }
            Output::TabRemoved(index) => {
                self.workspace
                    .tabs
                    .tab_strip
                    .retain(|entry| !matches!(entry, TabEntry::Diagram(other) if *other == index));
                for entry in &mut self.workspace.tabs.tab_strip {
                    if let TabEntry::Diagram(other) = entry
                        && *other > index
                    {
                        *other -= 1;
                    }
                }
            }
            Output::EnsureTab => return self.ensure_workspace_tab(),
            Output::OpenTable(name) => {
                return self.update(Message::Workspace(
                    crate::app::features::workspace::Message::Explorer(
                        crate::app::features::workspace::explorer::Message::SidebarTablePressed(
                            name,
                        ),
                    ),
                ));
            }
            Output::Toast(level, text) => self.push_toast(level, text),
        }
        Task::none()
    }

    fn diagram_context(&self) -> crate::app::features::workspace::diagram::Context {
        crate::app::features::workspace::diagram::Context {
            pool: self.connections.pool.clone(),
            database: self.current_database(),
            driver: self.connections.current.driver,
            connection_scope: self.chat_scope_key(),
            keys: self.diagram_store_keys(),
            theme: self.theme(),
            font_family: self.settings.values.font.family().to_string(),
        }
    }
    pub(crate) fn update_diagram(
        &mut self,
        message: crate::app::features::workspace::diagram::Message,
    ) -> Task<Message> {
        let context = self.diagram_context();
        let (task, outputs) = self.workspace.diagram.update(message, &context);
        let mut tasks = vec![task.map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })];
        for output in outputs {
            tasks.push(self.apply_diagram_output(output));
        }
        Task::batch(tasks)
    }

    pub(crate) fn active_diagram(&self) -> Option<&DiagramState> {
        self.workspace.diagram.active_diagram()
    }

    pub(crate) fn is_diagram_fullscreen(&self) -> bool {
        self.workspace.diagram.is_diagram_fullscreen()
    }

    pub(crate) fn is_diagram_active(&self) -> bool {
        self.workspace.diagram.is_diagram_active()
    }

    pub(crate) fn deactivate_diagram(&mut self) {
        self.workspace.diagram.deactivate_diagram()
    }

    pub(crate) fn diagram_tab_title(&self, index: usize) -> String {
        self.workspace.diagram.diagram_tab_title(index)
    }

    pub(crate) fn diagram_tab_display_title(&self, index: usize) -> String {
        let title = self.diagram_tab_title(index);
        if self.saved_diagram_names().contains(&title) {
            crate::i18n::tr_with("{name} \u{2014} Diagram", &[("{name}", &title)])
        } else {
            title
        }
    }

    pub(crate) fn diagram_search_animating(&self) -> bool {
        self.workspace.diagram.diagram_search_animating()
    }

    pub(crate) fn tick_diagram_search_animation(&mut self) {
        self.workspace.diagram.tick_diagram_search_animation()
    }

    pub(crate) fn open_schema_diagram(&mut self) -> Task<Message> {
        let context = self.diagram_context();
        let mut outputs = Vec::new();
        let task = self
            .workspace
            .diagram
            .open_schema_diagram(&context, &mut outputs);
        let mut tasks = vec![task.map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })];
        for output in outputs {
            tasks.push(self.apply_diagram_output(output));
        }
        Task::batch(tasks)
    }

    pub(crate) fn new_schema_diagram(&mut self) -> Task<Message> {
        let context = self.diagram_context();
        let mut outputs = Vec::new();
        let task = self
            .workspace
            .diagram
            .new_schema_diagram(&context, &mut outputs);
        let mut tasks = vec![task.map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })];
        for output in outputs {
            tasks.push(self.apply_diagram_output(output));
        }
        Task::batch(tasks)
    }

    pub(crate) fn activate_diagram_tab(&mut self, index: usize) -> Task<Message> {
        let mut outputs = Vec::new();
        let task = self
            .workspace
            .diagram
            .activate_diagram_tab(&mut outputs, index);
        let mut tasks = vec![task.map(|value| {
            Message::Workspace(crate::app::features::workspace::Message::Diagram(value))
        })];
        for output in outputs {
            tasks.push(self.apply_diagram_output(output));
        }
        Task::batch(tasks)
    }

    pub(crate) fn close_schema_diagram(&mut self, index: usize) -> Task<Message> {
        self.update_diagram(
            crate::app::features::workspace::diagram::Message::CloseSchemaDiagram(index),
        )
    }

    pub(crate) fn ensure_workspace_tab(&mut self) -> Task<Message> {
        if !self.workspace.tabs.tab_strip.is_empty() {
            return Task::none();
        }
        let state = self.blank_query_state();
        self.open_new_query_tab(state);
        Task::none()
    }

    pub(crate) fn saved_diagram_names(&self) -> Vec<String> {
        let keys = self.diagram_store_keys();
        self.workspace.diagram.saved_diagram_names(keys)
    }

    pub(crate) fn saved_diagram_summaries(&self) -> Vec<String> {
        self.workspace.diagram.saved_diagram_summaries()
    }

    pub(crate) fn saved_diagram_entries(&self) -> Vec<(String, String, String)> {
        self.workspace.diagram.saved_diagram_entries()
    }

    pub(crate) fn clear_saved_diagrams(&mut self) -> Result<usize, String> {
        self.workspace.diagram.clear_saved_diagrams()
    }

    pub(crate) fn diagram_store_keys(&self) -> Option<(String, String)> {
        let database = self.current_database()?;
        let mut connection = self.connections.current.clone();
        connection.database.clear();
        Some((
            StoredConnection::from_info(&connection)
                .normalized()
                .secret_lookup_key(),
            database,
        ))
    }

    pub(crate) fn diagram_report(&self) -> String {
        self.workspace.diagram.diagram_report()
    }

    fn diagram_agent_note(&mut self, summary: String, prompt: String) -> Task<Message> {
        self.update_ai(crate::app::features::ai::Message::DiagramNote {
            summary,
            report: self.diagram_report(),
            prompt,
        })
    }

    pub(crate) fn diagram_agent_tick(&mut self) {
        self.workspace
            .diagram
            .diagram_agent_tick(self.ai.chat_follows_diagram_agent);
    }

    pub(crate) fn diagram_agent_animating(&self) -> bool {
        self.workspace.diagram.diagram_agent_animating()
    }

    pub(crate) fn diagram_agent_clear(&mut self) {
        self.ai.chat_diagram_steps = 0;
        self.ai.chat_diagram_prompt = None;
        self.ai.chat_diagram_last = None;
        for tab in &mut self.workspace.diagram.diagram_tabs {
            tab.state.agent = None;
        }
    }

    pub(crate) fn diagram_agent_run(
        &mut self,
        action: DiagramAgentAction,
        prompt: String,
    ) -> Task<Message> {
        if matches!(action, DiagramAgentAction::Open)
            && self.workspace.diagram.diagram_tabs.is_empty()
        {
            self.ai.chat_diagram_prompt = Some(prompt);
            self.ai.chat_activity = Some(crate::i18n::tr("Opening the schema diagram"));
            return self.new_schema_diagram();
        }
        let open = self.workspace.diagram.active_diagram_tab.or((!self
            .workspace
            .diagram
            .diagram_tabs
            .is_empty())
        .then_some(0));
        let Some(open) = open else {
            return self.send_chat_note(
                Some(crate::i18n::tr("No diagram is open")),
                format!("No diagram is open. Send `{AI_CHAT_DIRECTIVE_PREFIX}diagram` first, then answer: {prompt}"),
            );
        };
        if self.ai.chat_diagram_last.as_ref() == Some(&action) {
            self.ai.chat_diagram_last = None;
            return self.send_chat_note(
                Some(crate::i18n::tr("Skipped a repeated diagram step")),
                format!(
                    "That is the same diagram command you just sent and the canvas already shows its result. Do not send it again; answer the user now: {prompt}"
                ),
            );
        }
        self.ai.chat_diagram_last = Some(action.clone());
        let activate = self.activate_diagram_tab(open);
        let follow = self.ai.chat_follows_diagram_agent;

        let output = self
            .workspace
            .diagram
            .apply_agent_action(action, prompt, follow);
        let completed = matches!(
            output,
            crate::app::features::workspace::diagram::Output::AgentStep { .. }
        );
        let task = self.apply_diagram_output(output);
        if completed {
            Task::batch([activate, task])
        } else {
            task
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::workspace::diagram::modal::parse_column_definition;
    use crate::app::features::workspace::diagram::model::{
        DiagramAgent, DiagramCommand, DiagramTab, SchemaChange, SchemaDiagram,
    };
    use crate::app::features::workspace::diagram::sql::diagram_change_sql;
    use crate::model::connection::{DatabaseDriver, TlsMode};
    use crate::model::diagram::{DiagramColumn, DiagramStore, StoredDiagram};
    use iced::Point;

    #[test]
    fn removing_a_saved_diagram_drops_its_store_entry_and_open_tabs() {
        let (mut app, _) = App::new();
        app.connections.current.driver = DatabaseDriver::Sqlite;
        app.connections.current.sqlite_path = String::from("/tmp/diagram-delete-test.db");
        app.connections.current.database = String::from("main");
        let (connection, database) = app.diagram_store_keys().expect("diagram scope");
        app.workspace
            .diagram
            .diagram_store
            .connections
            .entry(connection)
            .or_default()
            .databases
            .entry(database)
            .or_default()
            .diagrams
            .push(StoredDiagram {
                name: String::from("Billing"),
                ..StoredDiagram::default()
            });
        app.workspace.diagram.diagram_tabs = vec![
            DiagramTab {
                id: 1,
                title: String::from("Billing"),
                pinned: false,
                state: DiagramState::default(),
            },
            DiagramTab {
                id: 2,
                title: String::from("Other"),
                pinned: false,
                state: DiagramState::default(),
            },
        ];
        app.workspace.tabs.tab_strip = vec![TabEntry::Diagram(0), TabEntry::Diagram(1)];
        app.workspace.diagram.active_diagram_tab = Some(1);

        assert!(
            app.workspace
                .diagram
                .remove_saved_diagram_from_store(app.diagram_store_keys(), "Billing")
        );
        let _ = app.close_schema_diagram(0);

        assert!(app.saved_diagram_names().is_empty());
        assert_eq!(app.workspace.diagram.diagram_tabs[0].title, "Other");
        assert_eq!(app.workspace.tabs.tab_strip, vec![TabEntry::Diagram(0)]);
        assert_eq!(app.workspace.diagram.active_diagram_tab, Some(0));
    }

    #[test]
    fn diagram_drafts_are_unsaved_and_saved_diagrams_are_scoped() {
        let (mut app, _) = App::new();
        app.connections.current.driver = DatabaseDriver::Sqlite;
        app.connections.current.sqlite_path = String::from("/tmp/diagram-scope-a.db");
        app.connections.current.database = String::from("main");
        let _ = app.new_schema_diagram();

        assert!(app.saved_diagram_names().is_empty());

        let (connection, database) = app.diagram_store_keys().expect("diagram scope");
        app.workspace
            .diagram
            .diagram_store
            .connections
            .entry(connection)
            .or_default()
            .databases
            .entry(database)
            .or_default()
            .diagrams
            .push(StoredDiagram {
                name: String::from("Main"),
                ..StoredDiagram::default()
            });

        assert_eq!(app.saved_diagram_names(), ["Main"]);
        app.connections.current.database = String::from("other");
        assert!(app.saved_diagram_names().is_empty());
        app.connections.current.database = String::from("main");
        app.connections.current.sqlite_path = String::from("/tmp/diagram-scope-b.db");
        assert!(app.saved_diagram_names().is_empty());

        app.connections.current.driver = DatabaseDriver::MySql;
        app.connections.current.host = String::from("localhost");
        app.connections.current.port = String::from("3306");
        app.connections.current.username = String::from("cryo");
        app.connections.current.database = String::from("main");
        app.connections.current.tls_mode = TlsMode::Prefer;
        let preferred = app.diagram_store_keys().expect("MySQL scope").0;
        app.connections.current.tls_mode = TlsMode::Require;
        assert_ne!(preferred, app.diagram_store_keys().expect("MySQL scope").0);
    }

    #[test]
    fn clearing_saved_diagrams_removes_every_scope() {
        let (mut app, _) = App::new();
        app.workspace.diagram.diagram_store = DiagramStore::default();
        for (connection, database, name) in [
            ("sqlite|/tmp/a.db", "main", "One"),
            ("sqlite|/tmp/b.db", "other", "Two"),
        ] {
            app.workspace
                .diagram
                .diagram_store
                .connections
                .entry(String::from(connection))
                .or_default()
                .databases
                .entry(String::from(database))
                .or_default()
                .diagrams
                .push(StoredDiagram {
                    name: String::from(name),
                    ..StoredDiagram::default()
                });
        }

        assert_eq!(app.saved_diagram_summaries(), ["main - One", "other - Two"]);
        assert_eq!(app.workspace.diagram.clear_saved_diagrams_from_store(), 2);
        assert!(app.saved_diagram_summaries().is_empty());
    }

    #[test]
    fn diagram_agent_respects_the_chat_follow_control() {
        let (mut app, _) = App::new();
        let state = DiagramState {
            agent: Some(DiagramAgent::visiting(
                0,
                Point::ORIGIN,
                Point::new(120.0, 80.0),
                String::from("working"),
            )),
            ..DiagramState::default()
        };
        app.workspace.diagram.diagram_tabs.push(DiagramTab {
            id: 1,
            title: String::from("Draft"),
            pinned: false,
            state,
        });
        app.ai.chat_diagram_steps = 1;

        app.diagram_agent_tick();
        assert!(matches!(
            app.workspace.diagram.diagram_tabs[0].state.pending,
            Some(DiagramCommand::Follow { .. })
        ));

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatDiagramFollowToggled,
        ));
        assert!(
            app.workspace.diagram.diagram_tabs[0]
                .state
                .pending
                .is_none()
        );

        let _ = app.update(Message::Ai(
            crate::app::features::ai::Message::ChatDiagramFollowToggled,
        ));
        app.diagram_agent_tick();
        assert!(matches!(
            app.workspace.diagram.diagram_tabs[0].state.pending,
            Some(DiagramCommand::Follow { .. })
        ));
    }

    #[test]
    fn altering_a_column_keeps_what_the_definition_leaves_out() {
        let current = DiagramColumn {
            name: String::from("note"),
            data_type: String::from("TEXT"),
            primary: false,
            foreign: false,
            nullable: true,
            default: Some(String::from("'old'")),
            indexed: false,
            unique: false,
            attributes: String::new(),
        };

        assert_eq!(
            parse_column_definition("VARCHAR(120) NOT NULL", &current),
            (
                String::from("VARCHAR(120)"),
                false,
                Some(String::from("'old'"))
            )
        );
        assert_eq!(
            parse_column_definition("  ", &current),
            (String::from("TEXT"), true, Some(String::from("'old'")))
        );
        assert_eq!(
            parse_column_definition("INTEGER null default 0", &current),
            (String::from("INTEGER"), true, Some(String::from("0")))
        );
    }

    #[test]
    fn a_second_link_extends_the_pending_key_into_a_composite_one() {
        let column = |name: &str, unique: bool| DiagramColumn {
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
            title: String::from("New Diagram #1"),
            pinned: false,
            state: DiagramState {
                diagram: SchemaDiagram::build(
                    vec![
                        (
                            String::from("orders"),
                            vec![column("tenant_id", false), column("customer_id", false)],
                        ),
                        (
                            String::from("customers"),
                            vec![column("tenant_id", true), column("id", true)],
                        ),
                    ],
                    &[],
                ),
                loading: false,
                ..DiagramState::default()
            },
        });
        app.workspace.diagram.active_diagram_tab = Some(0);

        let first = SchemaChange::AddForeignKey {
            table: String::from("orders"),
            columns: vec![String::from("tenant_id")],
            referenced_table: String::from("customers"),
            referenced_columns: vec![String::from("tenant_id")],
        };
        app.workspace.diagram.diagram_apply_local_change(&first);
        app.workspace.diagram.diagram_record_change(first);

        let second = SchemaChange::AddForeignKey {
            table: String::from("orders"),
            columns: vec![String::from("customer_id")],
            referenced_table: String::from("customers"),
            referenced_columns: vec![String::from("id")],
        };
        assert!(app.workspace.diagram.diagram_extend_foreign_key(&second));
        assert!(!app.workspace.diagram.diagram_extend_foreign_key(&second));

        let state = app.active_diagram().expect("diagram");
        assert_eq!(state.changes.len(), 1);
        let SchemaChange::AddForeignKey {
            columns,
            referenced_columns,
            ..
        } = &state.changes[0]
        else {
            panic!("expected one composite key, got {:?}", state.changes);
        };
        assert_eq!(columns, &["tenant_id", "customer_id"]);
        assert_eq!(referenced_columns, &["tenant_id", "id"]);
        assert_eq!(state.diagram.edges.len(), 2);
        assert!(
            state
                .diagram
                .edges
                .iter()
                .all(|edge| edge.constraint_name == "fk_orders_tenant_id_customer_id")
        );

        let change = &state.changes[0];
        assert_eq!(
            diagram_change_sql(change, DatabaseDriver::MySql),
            "ALTER TABLE `orders` ADD CONSTRAINT `fk_orders_tenant_id_customer_id` FOREIGN KEY \
             (`tenant_id`, `customer_id`) REFERENCES `customers` (`tenant_id`, `id`);"
        );
        assert!(
            diagram_change_sql(change, DatabaseDriver::Sqlite)
                .contains("FOREIGN KEY (\"tenant_id\", \"customer_id\")")
        );
    }
}
