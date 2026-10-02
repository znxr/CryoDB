use super::model::{
    DIAGRAM_NOTE_HEIGHT, DIAGRAM_NOTE_WIDTH, DiagramGroup, DiagramModalKind, DiagramNote,
    SchemaChange,
};
use super::{Context, Message, Output, State};
use crate::app::types::ToastLevel;
use crate::model::diagram::DiagramColumn;
use iced::{Point, Rectangle, Size, Task};
impl State {
    pub(crate) fn diagram_submit_modal(
        &mut self,
        context: &Context,
        outputs: &mut Vec<Output>,
    ) -> Task<Message> {
        let Some((kind, table, raw_name, detail)) = self
            .active_diagram()
            .and_then(|state| state.modal.as_ref())
            .map(|modal| {
                (
                    modal.kind,
                    modal.table,
                    if matches!(
                        modal.kind,
                        DiagramModalKind::AddNote | DiagramModalKind::EditNote
                    ) {
                        modal.note.text()
                    } else {
                        modal.name.clone()
                    },
                    modal.detail.clone(),
                )
            })
        else {
            return Task::none();
        };
        let name = raw_name.trim().to_string();
        if name.is_empty() {
            if let Some(state) = self.active_diagram_mut()
                && let Some(modal) = state.modal.as_mut()
            {
                modal.error = Some(String::from("Name cannot be empty."));
            }
            return Task::none();
        }

        match kind {
            DiagramModalKind::CreateTable => {
                let columns = parse_column_list(&detail);
                let change = SchemaChange::CreateTable {
                    table: name,
                    columns,
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
            }
            DiagramModalKind::RenameTable => {
                if let Some(table) = table
                    .and_then(|index| self.active_diagram()?.diagram.tables.get(index))
                    .map(|table| table.name.clone())
                    && table != name
                {
                    let change = SchemaChange::RenameTable {
                        table,
                        new_table: name,
                    };
                    self.diagram_apply_local_change(&change);
                    self.diagram_record_change(change);
                }
            }
            DiagramModalKind::AddColumn => {
                let Some(table) = table
                    .and_then(|index| self.active_diagram()?.diagram.tables.get(index))
                    .map(|table| table.name.clone())
                else {
                    return Task::none();
                };
                let data_type = if detail.trim().is_empty() {
                    String::from("TEXT")
                } else {
                    detail.trim().to_string()
                };
                let change = SchemaChange::AddColumn {
                    table,
                    column: name,
                    data_type,
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
            }
            DiagramModalKind::AddGroup => {
                if let Some(state) = self.active_diagram_mut() {
                    let origin = Point::new(
                        -state.offset.x / state.zoom + 40.0,
                        -state.offset.y / state.zoom + 40.0,
                    );
                    let bounds = Rectangle::new(state.snapped(origin), Size::new(520.0, 360.0));
                    let mut group = DiagramGroup::new(name, bounds);
                    group.color = state.diagram.groups.len() as u8;
                    state.diagram.groups.push(group);
                    state.diagram.sync_group_members();
                }
            }
            DiagramModalKind::RenameGroup => {
                if let Some(group) =
                    table.and_then(|index| self.active_diagram_mut()?.diagram.groups.get_mut(index))
                {
                    group.name = name;
                }
            }
            DiagramModalKind::DropColumn => {
                let Some((table_name, column_name)) = table.and_then(|index| {
                    let table = self.active_diagram()?.diagram.tables.get(index)?;
                    let column = table
                        .columns
                        .iter()
                        .find(|column| column.name.eq_ignore_ascii_case(&name))?;
                    Some((table.name.clone(), column.name.clone()))
                }) else {
                    if let Some(modal) = self
                        .active_diagram_mut()
                        .and_then(|state| state.modal.as_mut())
                    {
                        modal.error = Some(String::from("Pick a column to drop."));
                    }
                    return Task::none();
                };
                let change = SchemaChange::DropColumn {
                    table: table_name,
                    column: column_name,
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
            }
            DiagramModalKind::AlterColumn => {
                let Some((table_name, column)) = table.and_then(|index| {
                    let table = self.active_diagram()?.diagram.tables.get(index)?;
                    let column = table
                        .columns
                        .iter()
                        .find(|column| column.name.eq_ignore_ascii_case(&name))?;
                    Some((table.name.clone(), column.clone()))
                }) else {
                    if let Some(modal) = self
                        .active_diagram_mut()
                        .and_then(|state| state.modal.as_mut())
                    {
                        modal.error = Some(String::from("Pick a column to alter."));
                    }
                    return Task::none();
                };
                let (data_type, nullable, default) = parse_column_definition(&detail, &column);
                let change = SchemaChange::AlterColumn {
                    table: table_name,
                    column: column.name,
                    data_type,
                    nullable,
                    default,
                    attributes: column.attributes,
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
            }
            DiagramModalKind::SaveDiagramAs => {
                let Some(index) = self.active_diagram_tab else {
                    return Task::none();
                };
                self.persist_diagram_as(context.keys.clone(), index, &name);
                if let Some(tab) = self.diagram_tabs.get_mut(index) {
                    tab.title = name;
                }
            }
            DiagramModalKind::DeleteSavedDiagram => {
                match self.delete_saved_diagram(context, outputs, &name) {
                    Ok(true) => outputs.push(Output::Toast(
                        ToastLevel::Success,
                        crate::i18n::tr_with(
                            "Deleted the saved diagram {name}.",
                            &[("{name}", &name)],
                        ),
                    )),
                    Ok(false) => {
                        if let Some(modal) = self
                            .active_diagram_mut()
                            .and_then(|state| state.modal.as_mut())
                        {
                            modal.error =
                                Some(String::from("This saved diagram no longer exists."));
                        }
                    }
                    Err(error) => {
                        self.diagram_store_error = Some(error.clone());
                        if let Some(modal) = self
                            .active_diagram_mut()
                            .and_then(|state| state.modal.as_mut())
                        {
                            modal.error = Some(error);
                        }
                    }
                }
                return Task::none();
            }
            DiagramModalKind::EditNote => {
                if let Some(state) = self.active_diagram_mut()
                    && let Some(note) = table.and_then(|index| state.diagram.notes.get_mut(index))
                {
                    note.text = name;
                }
            }
            DiagramModalKind::AddNote => {
                if let Some(state) = self.active_diagram_mut() {
                    let origin = Point::new(
                        -state.offset.x / state.zoom + 40.0,
                        -state.offset.y / state.zoom + 40.0,
                    );
                    let anchor = table
                        .and_then(|index| state.diagram.tables.get(index))
                        .map(|table| table.name.clone());
                    let origin = anchor
                        .as_ref()
                        .and_then(|name| state.diagram.table_index(name))
                        .and_then(|index| state.diagram.tables.get(index))
                        .map(|table| {
                            Point::new(
                                table.position.x + table.size().width + 40.0,
                                table.position.y,
                            )
                        })
                        .unwrap_or(origin);
                    let origin = state.diagram.free_note_origin(
                        origin,
                        Size::new(DIAGRAM_NOTE_WIDTH, DIAGRAM_NOTE_HEIGHT),
                    );
                    state
                        .diagram
                        .notes
                        .push(DiagramNote::new(name, state.snapped(origin), anchor));
                }
            }
        }

        if let Some(state) = self.active_diagram_mut() {
            state.modal = None;
        }
        Task::none()
    }
}
pub(crate) fn parse_column_list(input: &str) -> Vec<(String, String, bool)> {
    let mut columns: Vec<(String, String, bool)> = input
        .lines()
        .flat_map(|line| line.split(','))
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let mut parts = entry.split_whitespace();
            let name = parts.next()?.to_string();
            let data_type = parts.collect::<Vec<_>>().join(" ");
            let data_type = if data_type.is_empty() {
                String::from("TEXT")
            } else {
                data_type
            };
            let primary = name.eq_ignore_ascii_case("id");
            Some((name, data_type, primary))
        })
        .collect();
    if columns.is_empty() {
        columns.push((String::from("id"), String::from("INTEGER"), true));
    }
    columns
}
fn find_keyword(value: &str, keyword: &str) -> Option<usize> {
    value
        .as_bytes()
        .windows(keyword.len())
        .position(|window| window.eq_ignore_ascii_case(keyword.as_bytes()))
}
pub(crate) fn parse_column_definition(
    input: &str,
    current: &DiagramColumn,
) -> (String, bool, Option<String>) {
    let mut rest = input.trim().trim_end_matches(';').trim().to_string();
    let mut default = current.default.clone();
    if let Some(position) = find_keyword(&rest, "DEFAULT") {
        default = Some(rest[position + 7..].trim().to_string()).filter(|value| !value.is_empty());
        rest.truncate(position);
    }
    let nullable = if let Some(position) = find_keyword(&rest, "NOT NULL") {
        rest.replace_range(position..position + 8, "");
        false
    } else if let Some(position) = find_keyword(&rest, "NULL") {
        rest.replace_range(position..position + 4, "");
        true
    } else {
        current.nullable
    };
    let data_type = rest.trim().to_string();
    (
        if data_type.is_empty() {
            current.data_type.clone()
        } else {
            data_type
        },
        nullable,
        default,
    )
}
