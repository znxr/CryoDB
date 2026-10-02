use super::State;
use super::model::{
    DiagramAgent, DiagramCommand, DiagramEdge, DiagramState, DiagramTable, SchemaChange,
    foreign_key_name,
};
use crate::constants::{
    AI_CHAT_MAX_RELATIONS, AI_CHAT_MAX_TABLE_NAMES, AI_PULSE_INTERVAL_MS, DIAGRAM_SEARCH_ANIM_MS,
};
use crate::constants::{
    DIAGRAM_AGENT_TRAVEL_SECS, DIAGRAM_AGENT_ZOOM_TRAVEL, DIAGRAM_AGENT_ZOOM_WORK,
};
use crate::model::diagram::DiagramColumn;
use iced::Point;
use iced::Rectangle;

impl State {
    pub(crate) fn diagram_agent_tick(&mut self, follow: bool) {
        let step = (AI_PULSE_INTERVAL_MS as f32 / 1000.0) / DIAGRAM_AGENT_TRAVEL_SECS;
        for tab in &mut self.diagram_tabs {
            let state = &mut tab.state;
            let Some(agent) = state.agent.as_ref() else {
                continue;
            };
            if follow {
                state.pending = Some(DiagramCommand::Follow {
                    point: agent.position(),
                    zoom: if agent.arrived() {
                        DIAGRAM_AGENT_ZOOM_WORK
                    } else {
                        DIAGRAM_AGENT_ZOOM_TRAVEL
                    },
                });
            }
            let Some(agent) = state.agent.as_mut() else {
                continue;
            };
            if agent.arrived() && agent.target.is_none() && agent.queue.is_empty() {
                continue;
            }

            if !agent.arrived() {
                agent.progress = (agent.progress + step).min(1.0);
                if agent.dragging {
                    let cursor = agent.position();
                    if let Some(table) = state.diagram.tables.get_mut(agent.table) {
                        let size = table.size();
                        table.position = Point::new(cursor.x - size.width + 14.0, cursor.y - 14.0);
                    }
                    state.refresh_derived();
                }
                continue;
            }

            match (agent.dragging, agent.target) {
                (false, Some(target)) => {
                    let Some(size) = state
                        .diagram
                        .tables
                        .get(agent.table)
                        .map(DiagramTable::size)
                    else {
                        continue;
                    };
                    agent.dragging = true;
                    agent.from = agent.position();
                    agent.to = DiagramAgent::grab(Rectangle::new(target, size));
                    agent.progress = 0.0;
                }
                (true, Some(target)) => {
                    if let Some(table) = state.diagram.tables.get_mut(agent.table) {
                        table.position = target;
                    }
                    agent.dragging = false;
                    agent.target = None;
                    match agent.queue.pop() {
                        Some((next, next_target)) => {
                            let from = agent.position();
                            let to = state
                                .diagram
                                .tables
                                .get(next)
                                .map(|table| DiagramAgent::grab(table.bounds()))
                                .unwrap_or(from);
                            agent.table = next;
                            agent.target = Some(next_target);
                            agent.from = from;
                            agent.to = to;
                            agent.progress = 0.0;
                        }
                        None => {
                            state.diagram.sync_group_members();
                        }
                    }
                    state.refresh_derived();
                }
                _ => {}
            }
        }
    }
    pub(crate) fn active_diagram(&self) -> Option<&DiagramState> {
        let index = self.active_diagram_tab?;
        self.diagram_tabs.get(index).map(|tab| &tab.state)
    }
    pub(crate) fn active_diagram_mut(&mut self) -> Option<&mut DiagramState> {
        let index = self.active_diagram_tab?;
        self.diagram_tabs.get_mut(index).map(|tab| &mut tab.state)
    }
    pub(crate) fn touch_diagram(&mut self) {
        if let Some(state) = self.active_diagram_mut() {
            state.refresh_derived();
        }
    }
    pub(crate) fn is_diagram_fullscreen(&self) -> bool {
        self.active_diagram().is_some_and(|state| state.fullscreen)
    }
    pub(crate) fn is_diagram_active(&self) -> bool {
        self.active_diagram_tab.is_some()
    }
    pub(crate) fn deactivate_diagram(&mut self) {
        self.active_diagram_tab = None;
    }
    pub(crate) fn diagram_tab_title(&self, index: usize) -> String {
        self.diagram_tabs
            .get(index)
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| format!("New Diagram #{}", index.saturating_add(1)))
    }
    pub(crate) fn diagram_search_animating(&self) -> bool {
        self.diagram_tabs
            .iter()
            .any(|tab| tab.state.search_anim != f32::from(u8::from(tab.state.search_open)))
    }
    pub(crate) fn tick_diagram_search_animation(&mut self) {
        let step = AI_PULSE_INTERVAL_MS as f32 / DIAGRAM_SEARCH_ANIM_MS as f32;
        for tab in &mut self.diagram_tabs {
            let target = f32::from(u8::from(tab.state.search_open));
            if tab.state.search_anim < target {
                tab.state.search_anim = (tab.state.search_anim + step).min(target);
            } else if tab.state.search_anim > target {
                tab.state.search_anim = (tab.state.search_anim - step).max(target);
            }
        }
    }
    pub(crate) fn diagram_follow_relation(&mut self, edge: usize) {
        let Some(state) = self.active_diagram_mut() else {
            return;
        };
        let Some((from, to)) = state
            .diagram
            .edges
            .get(edge)
            .map(|edge| (edge.from, edge.to))
        else {
            return;
        };
        let target = if state.selected == Some(to) { from } else { to };
        state.selected = Some(target);
        state.pending = Some(DiagramCommand::Focus(target));
    }
    pub(crate) fn diagram_record_change(&mut self, change: SchemaChange) {
        if let Some(state) = self.active_diagram_mut() {
            state.record_change(change);
        }
    }
    pub(crate) fn diagram_extend_foreign_key(&mut self, change: &SchemaChange) -> bool {
        let SchemaChange::AddForeignKey {
            table,
            columns,
            referenced_table,
            referenced_columns,
        } = change
        else {
            return false;
        };
        let Some(state) = self.active_diagram_mut() else {
            return false;
        };
        let Some(SchemaChange::AddForeignKey {
            columns: pending_columns,
            referenced_columns: pending_referenced_columns,
            ..
        }) = state.changes.iter_mut().rev().find(|pending| {
            matches!(
                pending,
                SchemaChange::AddForeignKey {
                    table: pending_table,
                    referenced_table: pending_referenced,
                    ..
                } if pending_table.eq_ignore_ascii_case(table)
                    && pending_referenced.eq_ignore_ascii_case(referenced_table)
            )
        })
        else {
            return false;
        };
        if pending_columns
            .iter()
            .any(|name| columns.iter().any(|added| added.eq_ignore_ascii_case(name)))
        {
            return false;
        }
        let previous = foreign_key_name(table, pending_columns);
        pending_columns.extend(columns.iter().cloned());
        pending_referenced_columns.extend(referenced_columns.iter().cloned());
        let name = foreign_key_name(table, pending_columns);
        let added = foreign_key_name(table, columns);
        self.diagram_apply_local_change(change);
        if let Some(state) = self.active_diagram_mut() {
            for edge in &mut state.diagram.edges {
                if edge.constraint_name == previous || edge.constraint_name == added {
                    edge.constraint_name = name.clone();
                }
            }
        }
        true
    }
    pub(crate) fn diagram_apply_local_change(&mut self, change: &SchemaChange) {
        let Some(state) = self.active_diagram_mut() else {
            return;
        };
        Self::apply_local_change(state, change);
    }
    fn apply_local_change(state: &mut DiagramState, change: &SchemaChange) {
        match change {
            SchemaChange::CreateTable { table, columns } => {
                let center = Point::new(
                    -state.offset.x / state.zoom + 60.0,
                    -state.offset.y / state.zoom + 60.0,
                );
                state.diagram.tables.push(DiagramTable {
                    name: table.clone(),
                    columns: columns
                        .iter()
                        .map(|(name, data_type, primary)| DiagramColumn {
                            name: name.clone(),
                            data_type: data_type.clone(),
                            primary: *primary,
                            foreign: false,
                            nullable: !*primary,
                            default: None,
                            indexed: *primary,
                            unique: *primary,
                            attributes: String::new(),
                        })
                        .collect(),
                    position: state.snapped(center),
                    collapsed: false,
                });
                let index = state.diagram.tables.len() - 1;
                state.selected = Some(index);
            }
            SchemaChange::DropTable { table } => {
                let Some(index) = state.diagram.table_index(table) else {
                    return;
                };
                state.diagram.tables.remove(index);
                state
                    .diagram
                    .edges
                    .retain(|edge| edge.from != index && edge.to != index);
                for edge in &mut state.diagram.edges {
                    if edge.from > index {
                        edge.from -= 1;
                    }
                    if edge.to > index {
                        edge.to -= 1;
                    }
                }
                state.selected = None;
            }
            SchemaChange::RenameTable { table, new_table } => {
                let Some(index) = state.diagram.table_index(table) else {
                    return;
                };
                for note in &mut state.diagram.notes {
                    if note
                        .anchor
                        .as_deref()
                        .is_some_and(|anchor| anchor.eq_ignore_ascii_case(table))
                    {
                        note.anchor = Some(new_table.clone());
                    }
                }
                state.diagram.tables[index].name = new_table.clone();
            }
            SchemaChange::AddColumn {
                table,
                column,
                data_type,
            } => {
                if let Some(index) = state.diagram.table_index(table) {
                    state.diagram.tables[index].columns.push(DiagramColumn {
                        name: column.clone(),
                        data_type: data_type.clone(),
                        primary: false,
                        foreign: false,
                        nullable: true,
                        default: None,
                        indexed: false,
                        unique: false,
                        attributes: String::new(),
                    });
                }
            }
            SchemaChange::DropColumn { table, column } => {
                let Some(index) = state.diagram.table_index(table) else {
                    return;
                };
                let Some(position) = state.diagram.tables[index]
                    .columns
                    .iter()
                    .position(|item| item.name.eq_ignore_ascii_case(column))
                else {
                    return;
                };
                state.diagram.tables[index].columns.remove(position);
                state
                    .diagram
                    .edges
                    .retain(|edge| !(edge.from == index && edge.from_column == position));
                for edge in &mut state.diagram.edges {
                    if edge.from == index && edge.from_column > position {
                        edge.from_column -= 1;
                    }
                    if edge.to == index && edge.to_column > position {
                        edge.to_column -= 1;
                    }
                }
            }
            SchemaChange::AlterColumn {
                table,
                column,
                data_type,
                nullable,
                default,
                attributes,
            } => {
                if let Some(index) = state.diagram.table_index(table)
                    && let Some(entry) = state.diagram.tables[index]
                        .columns
                        .iter_mut()
                        .find(|item| item.name.eq_ignore_ascii_case(column))
                {
                    entry.data_type = data_type.clone();
                    entry.nullable = *nullable;
                    entry.default = default.clone();
                    entry.attributes = attributes.clone();
                }
            }
            SchemaChange::AddForeignKey {
                table,
                columns,
                referenced_table,
                referenced_columns,
            } => {
                let (Some(from), Some(to)) = (
                    state.diagram.table_index(table),
                    state.diagram.table_index(referenced_table),
                ) else {
                    return;
                };
                let constraint_name = foreign_key_name(table, columns);
                for (column, referenced_column) in columns.iter().zip(referenced_columns) {
                    let (Some(from_column), Some(to_column)) = (
                        state.diagram.tables[from]
                            .columns
                            .iter()
                            .position(|item| item.name.eq_ignore_ascii_case(column)),
                        state.diagram.tables[to]
                            .columns
                            .iter()
                            .position(|item| item.name.eq_ignore_ascii_case(referenced_column)),
                    ) else {
                        continue;
                    };
                    state.diagram.tables[from].columns[from_column].foreign = true;
                    let one_to_one = state.diagram.tables[from].columns[from_column].unique;
                    state.diagram.edges.push(DiagramEdge {
                        from,
                        from_column,
                        to,
                        to_column,
                        constraint_name: constraint_name.clone(),
                        one_to_one,
                    });
                }
            }
            SchemaChange::DropForeignKey {
                table,
                column,
                referenced_table,
                referenced_column,
                ..
            } => {
                let (Some(from), Some(to)) = (
                    state.diagram.table_index(table),
                    state.diagram.table_index(referenced_table),
                ) else {
                    return;
                };
                let (Some(from_column), Some(to_column)) = (
                    state.diagram.tables[from]
                        .columns
                        .iter()
                        .position(|item| item.name.eq_ignore_ascii_case(column)),
                    state.diagram.tables[to]
                        .columns
                        .iter()
                        .position(|item| item.name.eq_ignore_ascii_case(referenced_column)),
                ) else {
                    return;
                };
                state.diagram.edges.retain(|edge| {
                    !(edge.from == from
                        && edge.from_column == from_column
                        && edge.to == to
                        && edge.to_column == to_column)
                });
                state.diagram.tables[from].columns[from_column].foreign = state
                    .diagram
                    .edges
                    .iter()
                    .any(|edge| edge.from == from && edge.from_column == from_column);
            }
        }
    }
    pub(crate) fn diagram_reapply_changes(&mut self, index: usize) {
        let Some(state) = self.diagram_tabs.get_mut(index).map(|tab| &mut tab.state) else {
            return;
        };
        for change in state.changes.clone() {
            Self::apply_local_change(state, &change);
        }
    }
    pub(crate) fn diagram_report(&self) -> String {
        let Some(state) = self.active_diagram() else {
            return String::from("No diagram is open.");
        };
        if let Some(error) = &state.error {
            return format!("The diagram could not be read: {error}");
        }
        let tables = state
            .diagram
            .tables
            .iter()
            .take(AI_CHAT_MAX_TABLE_NAMES)
            .map(|table| {
                format!(
                    "{}({})",
                    table.name,
                    table
                        .columns
                        .iter()
                        .map(|column| column.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let relations = state
            .diagram
            .edges
            .iter()
            .take(AI_CHAT_MAX_RELATIONS)
            .filter_map(|edge| {
                let from = state.diagram.tables.get(edge.from)?;
                let to = state.diagram.tables.get(edge.to)?;
                Some(format!(
                    "{}.{} -> {}.{}",
                    from.name,
                    from.columns.get(edge.from_column)?.name,
                    to.name,
                    to.columns.get(edge.to_column)?.name
                ))
            })
            .collect::<Vec<_>>()
            .join("; ");
        let areas = state
            .diagram
            .groups
            .iter()
            .map(|group| format!("{} [{}]", group.name, group.members.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "Diagram: {} table(s), {} relation(s), {} note(s), {} pending schema change(s). \
             Tables: {tables}. Relations: {}. Areas: {}.",
            state.diagram.tables.len(),
            state.diagram.edges.len(),
            state.diagram.notes.len(),
            state.changes.len(),
            if relations.is_empty() {
                String::from("none")
            } else {
                relations
            },
            if areas.is_empty() {
                String::from("none")
            } else {
                areas
            }
        )
    }
    pub(crate) fn diagram_agent_mark(&mut self, index: usize, label: String) {
        let Some(state) = self.active_diagram_mut() else {
            return;
        };
        let Some(table) = state.diagram.tables.get(index) else {
            return;
        };
        let to = DiagramAgent::grab(table.bounds());
        let from = state
            .agent
            .as_ref()
            .map(DiagramAgent::position)
            .unwrap_or(Point::new(to.x - 260.0, to.y - 180.0));
        state.agent = Some(DiagramAgent::visiting(index, from, to, label));
    }
    pub(crate) fn diagram_agent_drag(&mut self, plan: Vec<(usize, Point)>, label: String) {
        let Some((first, target)) = plan.first().copied() else {
            return;
        };
        self.diagram_agent_mark(first, label);
        if let Some(agent) = self
            .active_diagram_mut()
            .and_then(|state| state.agent.as_mut())
        {
            agent.target = Some(target);
            agent.queue = plan.into_iter().skip(1).rev().collect();
        }
    }
    pub(crate) fn diagram_agent_animating(&self) -> bool {
        self.diagram_tabs.iter().any(|tab| {
            tab.state
                .agent
                .as_ref()
                .is_some_and(|agent| !agent.arrived() || agent.target.is_some())
        })
    }
}
