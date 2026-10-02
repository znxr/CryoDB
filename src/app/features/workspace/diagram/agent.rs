use super::modal::parse_column_list;
use super::model::{
    DIAGRAM_NOTE_HEIGHT, DIAGRAM_NOTE_WIDTH, DiagramCommand, DiagramNote, SchemaChange,
};
use super::{Output, State};
use crate::app::types::DiagramAgentAction;
use crate::constants::AI_CHAT_DIRECTIVE_PREFIX;
use iced::{Point, Size};
impl State {
    pub(crate) fn apply_agent_action(
        &mut self,
        action: DiagramAgentAction,
        prompt: String,
        follow: bool,
    ) -> Output {
        let step = match action {
            DiagramAgentAction::Open => crate::i18n::tr("Opened the schema diagram"),
            DiagramAgentAction::Layout => {
                if let Some(state) = self.active_diagram_mut() {
                    state.diagram.smart_layout();
                    if follow {
                        state.pending = Some(DiagramCommand::Fit);
                    }
                    state.refresh_derived();
                }
                crate::i18n::tr("Rearranged the diagram by clusters")
            }
            DiagramAgentAction::Lock { name, locked } => {
                let Some(index) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.group_index(&name))
                else {
                    return self.diagram_agent_missing_area(&name, prompt);
                };
                if let Some(state) = self.active_diagram_mut()
                    && let Some(group) = state.diagram.groups.get_mut(index)
                {
                    group.locked = locked;
                    group.contents_locked = locked;
                }
                if locked {
                    crate::i18n::tr_with("Locked the area {name}", &[("{name}", &name)])
                } else {
                    crate::i18n::tr_with("Unlocked the area {name}", &[("{name}", &name)])
                }
            }
            DiagramAgentAction::Focus(table) => {
                let Some(index) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.table_index(&table))
                else {
                    return self.diagram_agent_missing(&table, prompt);
                };
                if let Some(state) = self.active_diagram_mut() {
                    state.selected = Some(index);
                    if follow {
                        state.pending = Some(DiagramCommand::Focus(index));
                    }
                    state.refresh_derived();
                }
                self.diagram_agent_mark(index, crate::i18n::tr("looking here"));
                crate::i18n::tr_with("Focused {table} on the diagram", &[("{table}", &table)])
            }
            DiagramAgentAction::Area { name, tables } => {
                let members = self
                    .active_diagram()
                    .map(|state| {
                        tables
                            .iter()
                            .filter_map(|table| state.diagram.table_index(table))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if members.is_empty() {
                    return self.diagram_agent_missing(&tables.join(", "), prompt);
                }
                let existing = self
                    .active_diagram()
                    .and_then(|state| state.diagram.group_index(&name));
                let plan = self
                    .active_diagram_mut()
                    .map(|state| {
                        let plan = state.diagram.plan_group(name.clone(), &members);
                        state.refresh_derived();
                        plan
                    })
                    .unwrap_or_default();
                self.diagram_agent_drag(plan, crate::i18n::tr("moving it in"));
                if existing.is_some() {
                    crate::i18n::tr_with(
                        "Redrew the area {name} around exactly those tables",
                        &[("{name}", &name)],
                    )
                } else {
                    crate::i18n::tr_with("Drew the area {name}", &[("{name}", &name)])
                }
            }
            DiagramAgentAction::Note { table, text } => {
                let Some(index) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.table_index(&table))
                else {
                    return self.diagram_agent_missing(&table, prompt);
                };
                if let Some(state) = self.active_diagram_mut() {
                    let (name, origin) = state
                        .diagram
                        .tables
                        .get(index)
                        .map(|table| {
                            (
                                table.name.clone(),
                                Point::new(
                                    table.position.x + table.size().width + 40.0,
                                    table.position.y,
                                ),
                            )
                        })
                        .unwrap_or((table.clone(), Point::ORIGIN));
                    let origin = state.diagram.free_note_origin(
                        origin,
                        Size::new(DIAGRAM_NOTE_WIDTH, DIAGRAM_NOTE_HEIGHT),
                    );
                    let origin = state.snapped(origin);
                    state
                        .diagram
                        .notes
                        .push(DiagramNote::new(text, origin, Some(name)));
                    state.refresh_derived();
                }
                self.diagram_agent_mark(index, crate::i18n::tr("writing a note"));
                crate::i18n::tr_with("Pinned a note to {table}", &[("{table}", &table)])
            }
            DiagramAgentAction::AddTable { table, columns } => {
                if self
                    .active_diagram()
                    .is_some_and(|state| state.diagram.table_index(&table).is_some())
                {
                    return Output::ChatNote(
                        Some(crate::i18n::tr_with(
                            "{table} is already on the diagram",
                            &[("{table}", &table)],
                        )),
                        format!(
                            "The diagram already has a table called {table}. Add columns to it with `{AI_CHAT_DIRECTIVE_PREFIX}diagram-add-column {table}.column_name: TYPE`, then answer: {prompt}"
                        ),
                    );
                }
                let change = SchemaChange::CreateTable {
                    table: table.clone(),
                    columns: parse_column_list(&columns),
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                if let Some(index) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.table_index(&table))
                {
                    self.diagram_agent_mark(index, crate::i18n::tr("drawing a table"));
                }
                crate::i18n::tr_with(
                    "Queued the new table {table}. The user still has to apply the pending changes.",
                    &[("{table}", &table)],
                )
            }
            DiagramAgentAction::AddColumn {
                table,
                column,
                data_type,
            } => {
                let Some(index) = self
                    .active_diagram()
                    .and_then(|state| state.diagram.table_index(&table))
                else {
                    return self.diagram_agent_missing(&table, prompt);
                };
                let change = SchemaChange::AddColumn {
                    table: table.clone(),
                    column: column.clone(),
                    data_type: data_type.clone(),
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                self.diagram_agent_mark(index, crate::i18n::tr("adding a column"));
                crate::i18n::tr_with(
                    "Queued {column} {type} on {table}. The user still has to apply the pending changes.",
                    &[
                        ("{column}", &column),
                        ("{type}", &data_type),
                        ("{table}", &table),
                    ],
                )
            }
            DiagramAgentAction::Link {
                table,
                column,
                referenced_table,
                referenced_column,
            } => {
                let Some(state) = self.active_diagram() else {
                    return self.diagram_agent_missing(&table, prompt);
                };
                let (Some(index), Some(target)) = (
                    state.diagram.table_index(&table),
                    state.diagram.table_index(&referenced_table),
                ) else {
                    let missing = if state.diagram.table_index(&table).is_none() {
                        table
                    } else {
                        referenced_table
                    };
                    return self.diagram_agent_missing(&missing, prompt);
                };
                let known = |index: usize, name: &str| {
                    self.active_diagram().is_some_and(|state| {
                        state.diagram.tables[index]
                            .columns
                            .iter()
                            .any(|item| item.name.eq_ignore_ascii_case(name))
                    })
                };
                if !known(index, &column) || !known(target, &referenced_column) {
                    return self.diagram_agent_missing(
                        &format!("{table}.{column} or {referenced_table}.{referenced_column}"),
                        prompt,
                    );
                }
                let change = SchemaChange::AddForeignKey {
                    table: table.clone(),
                    columns: vec![column.clone()],
                    referenced_table: referenced_table.clone(),
                    referenced_columns: vec![referenced_column.clone()],
                };
                self.diagram_apply_local_change(&change);
                self.diagram_record_change(change);
                self.diagram_agent_mark(index, crate::i18n::tr("drawing a relation"));
                crate::i18n::tr_with(
                    "Queued a foreign key from {table}.{column} to {target}.{target_column}. The user still has to apply the pending changes.",
                    &[
                        ("{table}", &table),
                        ("{column}", &column),
                        ("{target}", &referenced_table),
                        ("{target_column}", &referenced_column),
                    ],
                )
            }
        };

        Output::AgentStep {
            summary: step,
            prompt,
        }
    }
    fn diagram_agent_missing_area(&mut self, name: &str, prompt: String) -> Output {
        Output::ChatNote(
            Some(crate::i18n::tr_with(
                "No area named {name} on the diagram",
                &[("{name}", name)],
            )),
            format!(
                "The diagram has no area called {name}. Draw it first with `{AI_CHAT_DIRECTIVE_PREFIX}diagram-area {name}: table_one, table_two`, then answer: {prompt}"
            ),
        )
    }
    fn diagram_agent_missing(&mut self, table: &str, prompt: String) -> Output {
        Output::ChatNote(
            Some(crate::i18n::tr_with(
                "No table named {names} on the diagram",
                &[("{names}", table)],
            )),
            format!(
                "The diagram has no table called {table}. Use one of the tables it lists, then answer: {prompt}"
            ),
        )
    }
}
