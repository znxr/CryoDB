use super::model::{DiagramState, SchemaScriptError};
use super::sql::diagram_change_sql;
use super::{Context, Message, State};
use crate::model::connection::DatabaseDriver;
use iced::Task;
use rfd::AsyncFileDialog;

impl State {
    pub(crate) fn diagram_export(&mut self, context: &Context, png: bool) -> Task<Message> {
        let Some(state) = self.active_diagram() else {
            return Task::none();
        };
        let svg = super::canvas::diagram_svg(state, &context.theme, &context.font_family);
        let title = self
            .active_diagram_tab
            .map(|index| self.diagram_tab_title(index))
            .unwrap_or_else(|| String::from("diagram"));
        let file_name = format!(
            "{}.{}",
            title.replace(' ', "-").replace('#', "").to_lowercase(),
            if png { "png" } else { "svg" }
        );
        let size = state
            .diagram
            .content_bounds()
            .map(|bounds| (bounds.width + 80.0, bounds.height + 80.0))
            .unwrap_or((800.0, 600.0));

        Task::perform(
            async move {
                let handle = AsyncFileDialog::new()
                    .set_file_name(&file_name)
                    .save_file()
                    .await?;
                let path = handle.path().to_path_buf();
                let payload = if png {
                    super::canvas::svg_to_png(&svg, size).ok()?
                } else {
                    svg.into_bytes()
                };
                std::fs::write(&path, payload).ok()?;
                Some(path.display().to_string())
            },
            Message::SchemaDiagramExported,
        )
    }
    pub(crate) fn load_schema_diagram(&mut self, context: &Context) -> Task<Message> {
        let Some(index) = self.active_diagram_tab else {
            return Task::none();
        };
        self.load_schema_diagram_at(context, index)
    }
    pub(crate) fn load_schema_diagram_at(
        &mut self,
        context: &Context,
        index: usize,
    ) -> Task<Message> {
        let pool = context.pool.clone();
        let database = context.database.clone().unwrap_or_default();
        let connection_scope = context.connection_scope.clone();
        let Some(tab) = self.diagram_tabs.get_mut(index) else {
            return Task::none();
        };
        let Some(pool) = pool else {
            tab.state.loading = false;
            tab.state.error = Some(String::from("Connect to a database first."));
            return Task::none();
        };
        tab.state.loading = true;
        tab.state.error = None;
        let diagram_id = tab.id;
        Task::perform(
            crate::db::metadata::fetch_schema_diagram(pool, database.clone()),
            move |result| Message::SchemaDiagramLoaded {
                diagram_id,
                connection_scope,
                database,
                result,
            },
        )
    }
    pub(crate) fn diagram_ddl(&self, context: &Context) -> String {
        let Some(state) = self.active_diagram() else {
            return String::new();
        };
        let driver = context.driver;
        state
            .changes
            .iter()
            .map(|change| diagram_change_sql(change, driver))
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(crate) fn diagram_apply_changes(&mut self, context: &Context) -> Task<Message> {
        let Some(diagram_id) = self
            .active_diagram_tab
            .and_then(|index| self.diagram_tabs.get(index))
            .map(|tab| tab.id)
        else {
            return Task::none();
        };
        let connection_scope = context.connection_scope.clone();
        let driver = context.driver;
        let scripts = self
            .active_diagram()
            .map(|state| {
                state
                    .changes
                    .iter()
                    .map(|change| diagram_change_sql(change, driver))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if scripts.iter().all(|sql| sql.trim().is_empty()) {
            return Task::none();
        }
        let Some(pool) = context.pool.clone() else {
            return Task::none();
        };
        if let Some(state) = self.active_diagram_mut() {
            state.applying = true;
            state.ddl_preview = None;
        }
        let database = context.database.clone().unwrap_or_default();
        if context.driver == DatabaseDriver::Sqlite {
            let changes = self
                .active_diagram()
                .map(|state| {
                    state
                        .changes
                        .iter()
                        .cloned()
                        .map(|change| {
                            let sql = diagram_change_sql(&change, DatabaseDriver::Sqlite);
                            (change, sql)
                        })
                        .collect()
                })
                .unwrap_or_default();
            return Task::perform(
                async move {
                    crate::db::edits::run_sqlite_schema_changes(pool, database, changes)
                        .await
                        .map_err(|error| SchemaScriptError::new(0, error))
                },
                move |result| Message::SchemaDiagramApplied {
                    diagram_id,
                    connection_scope,
                    result,
                },
            );
        }
        Task::perform(
            crate::db::edits::run_schema_script(pool, database, scripts),
            move |result| Message::SchemaDiagramApplied {
                diagram_id,
                connection_scope,
                result,
            },
        )
    }
    pub(crate) fn diagram_undo_change(&mut self, context: &Context) -> Task<Message> {
        if self
            .active_diagram_mut()
            .is_some_and(DiagramState::undo_change)
        {
            self.load_schema_diagram(context)
        } else {
            Task::none()
        }
    }
    pub(crate) fn diagram_redo_change(&mut self, context: &Context) -> Task<Message> {
        if self
            .active_diagram_mut()
            .is_some_and(DiagramState::redo_change)
        {
            self.load_schema_diagram(context)
        } else {
            Task::none()
        }
    }
}
