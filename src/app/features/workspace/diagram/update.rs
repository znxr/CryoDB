use super::model::{
    DIAGRAM_MAX_ZOOM, DIAGRAM_MIN_ZOOM, DIAGRAM_NOTE_MIN_HEIGHT, DIAGRAM_NOTE_MIN_WIDTH,
    DiagramCommand, DiagramLink, DiagramMenu, DiagramModal, DiagramModalKind, DiagramTarget,
    SchemaChange, SchemaDiagram, SchemaScriptError,
};
use super::{Context, Output, State};
use crate::app::types::ToastLevel;
use crate::model::diagram::{DiagramRelations, SchemaDiagramData};
use iced::widget::text_editor;
use iced::{Point, Task};

#[derive(Debug, Clone)]
pub(crate) enum Message {
    ModalBlocked,
    Tooltip(Option<String>),
    OpenSchemaDiagram,
    NewSchemaDiagram,
    OpenSavedDiagram(String),
    DeleteSavedDiagram,
    CloseSchemaDiagram(usize),
    SchemaDiagramExported(Option<String>),
    DiagramExport {
        png: bool,
    },
    DiagramModalOpened {
        kind: DiagramModalKind,
        table: Option<usize>,
    },
    DiagramModalSubmitted,
    DiagramPreviewDdl,
    DiagramDiscardChanges,
    DiagramRemoveChange(usize),
    DiagramOpenSelectedTable,

    ReloadSchemaDiagram,
    SchemaDiagramLoaded {
        diagram_id: u64,
        connection_scope: String,
        database: String,
        result: Result<SchemaDiagramData, String>,
    },
    SchemaDiagramApplied {
        diagram_id: u64,
        connection_scope: String,
        result: Result<usize, SchemaScriptError>,
    },
    DiagramApplyChanges,
    DiagramUndoChange,
    DiagramRedoChange,

    DiagramViewChanged {
        offset: iced::Vector,
        zoom: f32,
    },
    DiagramTableMoved {
        index: usize,
        position: Point,
    },
    DiagramGroupMoved {
        index: usize,
        position: Point,
    },
    DiagramGroupResized {
        index: usize,
        corner: Point,
    },
    DiagramGroupLockToggled(usize),
    DiagramGroupContentsLockToggled(usize),
    DiagramGroupCollapseToggled(usize),
    DiagramGroupColorCycled(usize),
    DiagramNoteEdit(usize),
    DiagramNoteMoved {
        index: usize,
        position: Point,
    },
    DiagramNoteResized {
        index: usize,
        corner: Point,
    },
    DiagramLayoutCommitted,
    DiagramTableSelected(Option<usize>),
    DiagramTableCollapseToggled(usize),
    DiagramEdgeHovered(Option<usize>),
    DiagramRelationFollowed(usize),
    DiagramCommandRequested(DiagramCommand),
    DiagramSnapToggled,
    DiagramRelationsSelected(DiagramRelations),
    DiagramFullscreenToggled,
    DiagramSearchToggled,
    DiagramSearchChanged(String),
    DiagramSearchSubmitted,
    DiagramSearchPrevious,
    DiagramSearchNext,
    DiagramAutoLayout,
    DiagramMenuRequested {
        position: Point,
        target: Option<DiagramTarget>,
    },
    DiagramMenuClosed,
    DiagramLinkStarted {
        from: usize,
        from_column: usize,
        cursor: Point,
    },
    DiagramLinkReleased {
        target: Option<(usize, usize)>,
        extend: bool,
    },
    DiagramModalNameChanged(String),
    DiagramModalDetailChanged(String),
    DiagramModalNoteEdited(text_editor::Action),
    DiagramModalClosed,
    DiagramExpandAll(bool),
    DiagramDropTable(usize),
    DiagramDropRelation(usize),
    DiagramDeleteGroup(usize),
    DiagramDeleteNote(usize),
    DiagramClosePreview,
}

impl State {
    pub(crate) fn update(
        &mut self,
        message: Message,
        context: &Context,
    ) -> (Task<Message>, Vec<Output>) {
        let mut outputs = Vec::new();
        let task = self.update_internal(message, context, &mut outputs);
        (task, outputs)
    }
    fn update_internal(
        &mut self,
        message: Message,
        context: &Context,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        match message {
            Message::ModalBlocked => Task::none(),
            Message::Tooltip(text) => {
                outputs.push(Output::Tooltip(text));
                Task::none()
            }
            Message::OpenSchemaDiagram => self.open_schema_diagram(context, outputs),
            Message::NewSchemaDiagram => self.new_schema_diagram(context, outputs),
            Message::OpenSavedDiagram(name) => self.open_saved_diagram(context, outputs, name),
            Message::DeleteSavedDiagram => {
                let Some(index) = self.active_diagram_tab else {
                    return Task::none();
                };
                let name = self.diagram_tab_title(index);
                let mut modal = DiagramModal::new(DiagramModalKind::DeleteSavedDiagram, None);
                modal.name = name;
                if let Some(state) = self.active_diagram_mut() {
                    state.modal = Some(modal);
                }
                Task::none()
            }
            Message::CloseSchemaDiagram(index) => self.close_schema_diagram(outputs, index),
            Message::SchemaDiagramExported(path) => {
                if let Some(path) = path {
                    outputs.push(Output::Toast(
                        ToastLevel::Success,
                        crate::i18n::tr_with("Diagram exported to {path}", &[("{path}", &path)]),
                    ));
                }
                Task::none()
            }
            Message::DiagramExport { png } => self.diagram_export(context, png),
            Message::DiagramModalOpened { kind, table } => {
                let saved_name = (kind == DiagramModalKind::SaveDiagramAs)
                    .then(|| {
                        self.active_diagram_tab
                            .map(|index| self.diagram_tab_title(index))
                    })
                    .flatten()
                    .filter(|name| {
                        self.saved_diagram_names(context.keys.clone())
                            .iter()
                            .any(|saved| saved == name)
                    });
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    let mut modal = DiagramModal::new(kind, table);
                    if let Some(name) = saved_name {
                        modal.name = name;
                    }
                    if let Some(index) = table {
                        if kind == DiagramModalKind::RenameTable
                            && let Some(table) = state.diagram.tables.get(index)
                        {
                            modal.name = table.name.clone();
                        } else if kind == DiagramModalKind::RenameGroup
                            && let Some(group) = state.diagram.groups.get(index)
                        {
                            modal.name = group.name.clone();
                        }
                    }
                    state.modal = Some(modal);
                }
                Task::none()
            }
            Message::DiagramModalSubmitted => self.diagram_submit_modal(context, outputs),
            Message::DiagramPreviewDdl => {
                let ddl = self.diagram_ddl(context);
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    state.ddl_preview = Some(ddl);
                }
                Task::none()
            }
            Message::DiagramDiscardChanges => {
                if let Some(state) = self.active_diagram_mut() {
                    state.changes.clear();
                    state.undone_changes.clear();
                    state.ddl_preview = None;
                }
                self.load_schema_diagram(context)
            }
            Message::DiagramRemoveChange(index) => {
                self.diagram_remove_change(context, outputs, index)
            }
            Message::DiagramOpenSelectedTable => self.diagram_open_selected_table(outputs),

            Message::ReloadSchemaDiagram => self.load_schema_diagram(context),
            Message::SchemaDiagramLoaded {
                diagram_id,
                connection_scope,
                database,
                result,
            } => {
                if context.connection_scope != connection_scope
                    || context.database.as_deref() != Some(database.as_str())
                {
                    return Task::none();
                }
                let Some(index) = self
                    .diagram_tabs
                    .iter()
                    .position(|tab| tab.id == diagram_id)
                else {
                    return Task::none();
                };
                let Some(tab) = self.diagram_tabs.get_mut(index) else {
                    return Task::none();
                };
                tab.state.loading = false;
                tab.state.selected = None;
                match result {
                    Ok(diagram) => {
                        tab.state.diagram = SchemaDiagram::from_data(diagram);
                        tab.state.error = None;
                        tab.state.pending = Some(DiagramCommand::Fit);
                        self.restore_diagram_layout(context.keys.clone(), index);
                        self.diagram_reapply_changes(index);
                        if let Some(tab) = self.diagram_tabs.get_mut(index) {
                            tab.state.refresh_derived();
                        }
                    }
                    Err(error) => tab.state.error = Some(error),
                }
                outputs.push(Output::Loaded);
                Task::none()
            }
            Message::SchemaDiagramApplied {
                diagram_id,
                connection_scope,
                result,
            } => {
                if context.connection_scope != connection_scope {
                    return Task::none();
                }
                if let Some(index) = self
                    .diagram_tabs
                    .iter()
                    .position(|tab| tab.id == diagram_id)
                    && let Some(state) = self.diagram_tabs.get_mut(index).map(|tab| &mut tab.state)
                {
                    state.applying = false;
                    match result {
                        Ok(_) => {
                            state.changes.clear();
                            state.undone_changes.clear();
                            state.error = None;
                        }
                        Err(error) => {
                            let applied = error.applied.min(state.changes.len());
                            if applied > 0 {
                                state.changes.drain(..applied);
                                state.undone_changes.clear();
                            }
                            state.error = Some(if applied > 0 {
                                crate::i18n::tr_with(
                                    "{count} change(s) were already applied and cannot be rolled \
                                     back; the remaining ones are still pending. {error}",
                                    &[
                                        ("{count}", &applied.to_string()),
                                        ("{error}", &error.message),
                                    ],
                                )
                            } else {
                                error.message
                            });
                        }
                    }
                    return self.load_schema_diagram_at(context, index);
                }
                Task::none()
            }
            Message::DiagramApplyChanges => self.diagram_apply_changes(context),
            Message::DiagramUndoChange => self.diagram_undo_change(context),
            Message::DiagramRedoChange => self.diagram_redo_change(context),

            Message::DiagramViewChanged { offset, zoom } => {
                if let Some(state) = self.active_diagram_mut() {
                    state.offset = offset;
                    state.zoom = zoom.clamp(DIAGRAM_MIN_ZOOM, DIAGRAM_MAX_ZOOM);
                    state.pending = None;
                }
                Task::none()
            }
            Message::DiagramTableMoved { index, position } => {
                if let Some(state) = self.active_diagram_mut() {
                    let position = state.snapped(position);
                    if let Some(table) = state.diagram.tables.get_mut(index) {
                        table.position = position;
                    }
                }
                Task::none()
            }
            Message::DiagramGroupMoved { index, position } => {
                if let Some(state) = self.active_diagram_mut() {
                    let position = state.snapped(position);
                    state.diagram.move_group(index, position);
                    state.refresh_derived();
                }
                Task::none()
            }
            Message::DiagramGroupResized { index, corner } => {
                if let Some(state) = self.active_diagram_mut() {
                    let corner = state.snapped(corner);
                    state.diagram.resize_group(index, corner);
                    state.refresh_derived();
                }
                Task::none()
            }
            Message::DiagramGroupLockToggled(index) => {
                if let Some(group) = self
                    .active_diagram_mut()
                    .and_then(|state| state.diagram.groups.get_mut(index))
                {
                    group.locked = !group.locked;
                }
                {
                    if let Some(state) = self.active_diagram_mut() {
                        state.menu = None;
                    }
                    Task::none()
                }
            }
            Message::DiagramGroupContentsLockToggled(index) => {
                if let Some(group) = self
                    .active_diagram_mut()
                    .and_then(|state| state.diagram.groups.get_mut(index))
                {
                    group.contents_locked = !group.contents_locked;
                }
                {
                    if let Some(state) = self.active_diagram_mut() {
                        state.menu = None;
                    }
                    Task::none()
                }
            }
            Message::DiagramGroupCollapseToggled(index) => {
                if let Some(state) = self.active_diagram_mut() {
                    if let Some(group) = state.diagram.groups.get_mut(index) {
                        group.collapsed = !group.collapsed;
                    }
                    state.refresh_derived();
                }
                {
                    if let Some(state) = self.active_diagram_mut() {
                        state.menu = None;
                    }
                    Task::none()
                }
            }
            Message::DiagramGroupColorCycled(index) => {
                if let Some(group) = self
                    .active_diagram_mut()
                    .and_then(|state| state.diagram.groups.get_mut(index))
                {
                    group.color = (group.color + 1) % crate::DIAGRAM_AREA_COLORS.len() as u8;
                }
                {
                    if let Some(state) = self.active_diagram_mut() {
                        state.menu = None;
                    }
                    Task::none()
                }
            }
            Message::DiagramNoteEdit(index) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    let text = state
                        .diagram
                        .notes
                        .get(index)
                        .map(|note| note.text.clone())
                        .unwrap_or_default();
                    let mut modal = DiagramModal::new(DiagramModalKind::EditNote, Some(index));
                    modal.note = text_editor::Content::with_text(&text);
                    state.modal = Some(modal);
                }
                Task::none()
            }
            Message::DiagramNoteMoved { index, position } => {
                if let Some(state) = self.active_diagram_mut() {
                    let position = state.snapped(position);
                    if let Some(note) = state.diagram.notes.get_mut(index) {
                        note.position = position;
                    }
                }
                Task::none()
            }
            Message::DiagramNoteResized { index, corner } => {
                if let Some(state) = self.active_diagram_mut() {
                    let corner = state.snapped(corner);
                    if let Some(note) = state.diagram.notes.get_mut(index) {
                        note.width = (corner.x - note.position.x).max(DIAGRAM_NOTE_MIN_WIDTH);
                        note.height = (corner.y - note.position.y).max(DIAGRAM_NOTE_MIN_HEIGHT);
                    }
                    state.refresh_derived();
                }
                Task::none()
            }
            Message::DiagramLayoutCommitted => {
                if let Some(state) = self.active_diagram_mut() {
                    state.diagram.sync_group_members();
                }
                self.touch_diagram();
                Task::none()
            }
            Message::DiagramTableSelected(index) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    state.selected = index.filter(|index| *index < state.diagram.tables.len());
                }
                Task::none()
            }
            Message::DiagramTableCollapseToggled(index) => {
                if let Some(table) = self
                    .active_diagram_mut()
                    .and_then(|state| state.diagram.tables.get_mut(index))
                {
                    table.collapsed = !table.collapsed;
                }
                self.touch_diagram();
                Task::none()
            }
            Message::DiagramExpandAll(expanded) => {
                if let Some(state) = self.active_diagram_mut() {
                    for table in &mut state.diagram.tables {
                        table.collapsed = !expanded;
                    }
                }
                self.touch_diagram();
                Task::none()
            }
            Message::DiagramEdgeHovered(edge) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.hovered_edge = edge;
                }
                Task::none()
            }
            Message::DiagramRelationFollowed(edge) => {
                self.diagram_follow_relation(edge);
                Task::none()
            }
            Message::DiagramRelationsSelected(relations) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.relations = relations;
                }
                Task::none()
            }
            Message::DiagramCommandRequested(command) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.pending = Some(command);
                }
                Task::none()
            }
            Message::DiagramSnapToggled => {
                if let Some(state) = self.active_diagram_mut() {
                    state.snap = !state.snap;
                }
                Task::none()
            }
            Message::DiagramFullscreenToggled => {
                if let Some(state) = self.active_diagram_mut() {
                    state.fullscreen = !state.fullscreen;
                }
                Task::none()
            }
            Message::DiagramSearchToggled => {
                if let Some(state) = self.active_diagram_mut() {
                    state.search_open = !state.search_open;
                    if !state.search_open {
                        state.search.clear();
                        state.match_index = 0;
                        state.refresh_derived();
                    }
                }
                Task::none()
            }
            Message::DiagramSearchChanged(value) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.search = value;
                    state.match_index = 0;
                    state.refresh_derived();
                }
                Task::none()
            }
            Message::DiagramSearchSubmitted => {
                if let Some(state) = self.active_diagram_mut()
                    && let Some(index) = state.matches.get(state.match_index).copied()
                {
                    state.selected = Some(index);
                    state.pending = Some(DiagramCommand::Focus(index));
                }
                Task::none()
            }
            Message::DiagramSearchPrevious => {
                if let Some(state) = self.active_diagram_mut()
                    && !state.matches.is_empty()
                {
                    state.match_index =
                        (state.match_index + state.matches.len() - 1) % state.matches.len();
                    let index = state.matches[state.match_index];
                    state.selected = Some(index);
                    state.pending = Some(DiagramCommand::Focus(index));
                }
                Task::none()
            }
            Message::DiagramSearchNext => {
                if let Some(state) = self.active_diagram_mut()
                    && !state.matches.is_empty()
                {
                    state.match_index = (state.match_index + 1) % state.matches.len();
                    let index = state.matches[state.match_index];
                    state.selected = Some(index);
                    state.pending = Some(DiagramCommand::Focus(index));
                }
                Task::none()
            }
            Message::DiagramAutoLayout => {
                if let Some(state) = self.active_diagram_mut() {
                    state.diagram.auto_layout();
                    state.pending = Some(DiagramCommand::Fit);
                    state.refresh_derived();
                }
                Task::none()
            }
            Message::DiagramMenuRequested { position, target } => {
                if let Some(state) = self.active_diagram_mut() {
                    if let Some(DiagramTarget::Table(index)) = target {
                        state.selected = Some(index);
                    }
                    state.menu = Some(DiagramMenu { position, target });
                }
                Task::none()
            }
            Message::DiagramMenuClosed => {
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                }
                Task::none()
            }
            Message::DiagramLinkStarted {
                from,
                from_column,
                cursor,
            } => {
                if let Some(state) = self.active_diagram_mut() {
                    state.link = Some(DiagramLink {
                        from,
                        from_column,
                        cursor,
                    });
                }
                Task::none()
            }
            Message::DiagramLinkReleased { target, extend } => {
                let Some(link) = self
                    .active_diagram_mut()
                    .and_then(|state| state.link.take())
                else {
                    return Task::none();
                };
                let Some((to, to_column)) = target else {
                    return Task::none();
                };
                let Some(state) = self.active_diagram() else {
                    return Task::none();
                };
                if to == link.from {
                    return Task::none();
                }
                let (Some(source), Some(target)) = (
                    state.diagram.tables.get(link.from),
                    state.diagram.tables.get(to),
                ) else {
                    return Task::none();
                };
                let Some(column) = source.columns.get(link.from_column) else {
                    return Task::none();
                };
                let Some(referenced) = target.columns.get(to_column) else {
                    return Task::none();
                };
                if !referenced.primary && !referenced.unique {
                    return Task::none();
                }
                let change = SchemaChange::AddForeignKey {
                    table: source.name.clone(),
                    columns: vec![column.name.clone()],
                    referenced_table: target.name.clone(),
                    referenced_columns: vec![referenced.name.clone()],
                };
                if extend && self.diagram_extend_foreign_key(&change) {
                    return Task::none();
                }
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                Task::none()
            }
            Message::DiagramModalNameChanged(value) => {
                if let Some(modal) = self
                    .active_diagram_mut()
                    .and_then(|state| state.modal.as_mut())
                {
                    modal.name = value;
                    modal.error = None;
                }
                Task::none()
            }
            Message::DiagramModalDetailChanged(value) => {
                if let Some(modal) = self
                    .active_diagram_mut()
                    .and_then(|state| state.modal.as_mut())
                {
                    modal.detail = value;
                }
                Task::none()
            }
            Message::DiagramModalNoteEdited(action) => {
                if let Some(modal) = self
                    .active_diagram_mut()
                    .and_then(|state| state.modal.as_mut())
                {
                    modal.note.perform(action);
                    modal.error = None;
                }
                Task::none()
            }
            Message::DiagramModalClosed => {
                if let Some(state) = self.active_diagram_mut() {
                    state.modal = None;
                }
                Task::none()
            }
            Message::DiagramDropTable(index) => {
                let Some(name) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.tables.get(index))
                    .map(|table| table.name.clone())
                else {
                    return Task::none();
                };
                let change = SchemaChange::DropTable { table: name };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                }
                Task::none()
            }
            Message::DiagramDropRelation(edge) => {
                let Some(change) = self.active_diagram().and_then(|state| {
                    let edge = state.diagram.edges.get(edge)?;
                    let source = state.diagram.tables.get(edge.from)?;
                    let target = state.diagram.tables.get(edge.to)?;
                    Some(SchemaChange::DropForeignKey {
                        table: source.name.clone(),
                        column: source.columns.get(edge.from_column)?.name.clone(),
                        referenced_table: target.name.clone(),
                        referenced_column: target.columns.get(edge.to_column)?.name.clone(),
                        constraint_name: edge.constraint_name.clone(),
                    })
                }) else {
                    return Task::none();
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    state.hovered_edge = None;
                }
                Task::none()
            }
            Message::DiagramDeleteGroup(index) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    if index < state.diagram.groups.len() {
                        state.diagram.groups.remove(index);
                    }
                }
                Task::none()
            }
            Message::DiagramDeleteNote(index) => {
                if let Some(state) = self.active_diagram_mut() {
                    state.menu = None;
                    if index < state.diagram.notes.len() {
                        state.diagram.notes.remove(index);
                    }
                }
                Task::none()
            }
            Message::DiagramClosePreview => {
                if let Some(state) = self.active_diagram_mut() {
                    state.ddl_preview = None;
                }
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{DiagramState, DiagramTab};
    use super::*;
    use crate::model::connection::DatabaseDriver;
    use crate::model::diagram::DiagramStore;

    fn fixture() -> (State, Context) {
        (
            State {
                diagram_tabs: [10, 20]
                    .into_iter()
                    .map(|id| DiagramTab {
                        id,
                        title: id.to_string(),
                        pinned: false,
                        state: DiagramState::default(),
                    })
                    .collect(),
                active_diagram_tab: Some(1),
                next_diagram_tab_number: 3,
                next_diagram_tab_id: 21,
                diagram_store: DiagramStore::default(),
                diagram_store_error: None,
            },
            Context {
                theme: iced::Theme::Dark,
                font_family: String::from("sans-serif"),
                pool: None,
                database: Some(String::from("main")),
                connection_scope: String::from("connection-a"),
                driver: DatabaseDriver::Sqlite,
                keys: None,
            },
        )
    }

    #[test]
    fn loading_updates_the_originating_diagram_after_switching_tabs() {
        let (mut state, context) = fixture();
        let (_, output) = state.update(
            Message::SchemaDiagramLoaded {
                diagram_id: 10,
                connection_scope: context.connection_scope.clone(),
                database: String::from("main"),
                result: Err(String::from("load failed")),
            },
            &context,
        );
        assert_eq!(
            state.diagram_tabs[0].state.error.as_deref(),
            Some("load failed")
        );
        assert!(!state.diagram_tabs[0].state.loading);
        assert!(state.diagram_tabs[1].state.loading);
        assert!(state.diagram_tabs[1].state.error.is_none());
        assert!(matches!(output.as_slice(), [Output::Loaded]));
    }

    #[test]
    fn loading_ignores_other_connections_databases_and_closed_diagrams() {
        let (mut state, context) = fixture();
        for (diagram_id, connection_scope, database) in [
            (10, "connection-b", "main"),
            (10, "connection-a", "other"),
            (99, "connection-a", "main"),
        ] {
            let (_, output) = state.update(
                Message::SchemaDiagramLoaded {
                    diagram_id,
                    connection_scope: connection_scope.to_string(),
                    database: database.to_string(),
                    result: Err(String::from("stale")),
                },
                &context,
            );
            assert!(output.is_empty());
            assert!(
                state
                    .diagram_tabs
                    .iter()
                    .all(|tab| tab.state.loading && tab.state.error.is_none())
            );
        }
    }
}
