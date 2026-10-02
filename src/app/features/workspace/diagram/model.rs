use crate::model::diagram::{
    DiagramColumn, DiagramRelations, SchemaDiagramData, StoredDiagram, StoredDiagramGroup,
    StoredDiagramNote, StoredDiagramTable,
};
#[cfg(test)]
use crate::model::table::SidebarRelationEntry;
use iced::{Point, Rectangle, Size, Vector};
use std::collections::HashMap;

pub(crate) const DIAGRAM_NODE_WIDTH: f32 = 240.0;
pub(crate) const DIAGRAM_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const DIAGRAM_ROW_HEIGHT: f32 = 18.0;
pub(crate) const DIAGRAM_NODE_GAP_X: f32 = 90.0;
pub(crate) const DIAGRAM_NODE_GAP_Y: f32 = 60.0;
pub(crate) const DIAGRAM_CLUSTER_GAP: f32 = 180.0;
pub(crate) const DIAGRAM_CLUSTER_ROW_WIDTH: f32 = 2600.0;
pub(crate) const DIAGRAM_MIN_ZOOM: f32 = 0.15;
pub(crate) const DIAGRAM_MAX_ZOOM: f32 = 3.0;
pub(crate) const DIAGRAM_GRID_STEP: f32 = 20.0;
pub(crate) const DIAGRAM_BUSY_EDGES: usize = 160;
pub(crate) const DIAGRAM_BLOCK_ZOOM: f32 = 0.28;

#[derive(Debug, Clone)]
pub(crate) struct DiagramTable {
    pub(crate) name: String,
    pub(crate) columns: Vec<DiagramColumn>,
    pub(crate) position: Point,
    pub(crate) collapsed: bool,
}

impl DiagramTable {
    pub(crate) fn visible_rows(&self) -> usize {
        if self.collapsed {
            0
        } else {
            self.columns.len()
        }
    }

    pub(crate) fn size(&self) -> Size {
        Size::new(
            DIAGRAM_NODE_WIDTH,
            DIAGRAM_HEADER_HEIGHT + self.visible_rows() as f32 * DIAGRAM_ROW_HEIGHT + 6.0,
        )
    }

    pub(crate) fn bounds(&self) -> Rectangle {
        Rectangle::new(self.position, self.size())
    }

    pub(crate) fn column_center_y(&self, index: usize) -> f32 {
        if self.collapsed {
            return self.position.y + DIAGRAM_HEADER_HEIGHT / 2.0;
        }
        self.position.y
            + DIAGRAM_HEADER_HEIGHT
            + index as f32 * DIAGRAM_ROW_HEIGHT
            + DIAGRAM_ROW_HEIGHT / 2.0
    }

    pub(crate) fn column_at(&self, point: Point) -> Option<usize> {
        if self.collapsed {
            return None;
        }
        let local = point.y - self.position.y - DIAGRAM_HEADER_HEIGHT;
        if local < 0.0 {
            return None;
        }
        let row = (local / DIAGRAM_ROW_HEIGHT) as usize;
        (row < self.columns.len()).then_some(row)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DiagramEdge {
    pub(crate) from: usize,
    pub(crate) from_column: usize,
    pub(crate) to: usize,
    pub(crate) to_column: usize,
    pub(crate) constraint_name: String,
    pub(crate) one_to_one: bool,
}

pub(crate) const DIAGRAM_GROUP_HEADER: f32 = 26.0;
pub(crate) const DIAGRAM_GROUP_HANDLE: f32 = 14.0;
pub(crate) const DIAGRAM_GROUP_MIN: Size = Size::new(220.0, 140.0);
pub(crate) const DIAGRAM_NOTE_WIDTH: f32 = 200.0;
pub(crate) const DIAGRAM_NOTE_HEIGHT: f32 = 74.0;
pub(crate) const DIAGRAM_NOTE_MIN_WIDTH: f32 = 120.0;
pub(crate) const DIAGRAM_NOTE_MIN_HEIGHT: f32 = 50.0;
pub(crate) const DIAGRAM_NOTE_HANDLE: f32 = 14.0;
pub(crate) const DIAGRAM_NOTE_LINE: f32 = 15.0;

#[derive(Debug, Clone)]
pub(crate) struct DiagramGroup {
    pub(crate) name: String,
    pub(crate) color: u8,
    pub(crate) bounds: Rectangle,
    pub(crate) locked: bool,
    pub(crate) contents_locked: bool,
    pub(crate) collapsed: bool,
    pub(crate) members: Vec<String>,
}

impl DiagramGroup {
    pub(crate) fn new(name: String, bounds: Rectangle) -> Self {
        Self {
            name,
            color: 0,
            bounds,
            locked: false,
            contents_locked: false,
            collapsed: false,
            members: Vec::new(),
        }
    }

    pub(crate) fn outline(&self) -> Rectangle {
        if self.collapsed {
            Rectangle::new(
                self.bounds.position(),
                Size::new(self.bounds.width, DIAGRAM_GROUP_HEADER),
            )
        } else {
            self.bounds
        }
    }

    pub(crate) fn header(&self) -> Rectangle {
        Rectangle::new(
            self.bounds.position(),
            Size::new(self.bounds.width, DIAGRAM_GROUP_HEADER),
        )
    }

    pub(crate) fn resize_handle(&self) -> Rectangle {
        Rectangle::new(
            Point::new(
                self.bounds.x + self.bounds.width - DIAGRAM_GROUP_HANDLE,
                self.bounds.y + self.bounds.height - DIAGRAM_GROUP_HANDLE,
            ),
            Size::new(DIAGRAM_GROUP_HANDLE, DIAGRAM_GROUP_HANDLE),
        )
    }

    pub(crate) fn holds(&self, name: &str) -> bool {
        let key = lookup_key(name);
        self.members.iter().any(|member| lookup_key(member) == key)
    }

    pub(crate) fn captures(&self, table: &DiagramTable) -> bool {
        if self.collapsed {
            return self.holds(&table.name);
        }
        let bounds = table.bounds();
        self.bounds.contains(Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        ))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DiagramNote {
    pub(crate) text: String,
    pub(crate) position: Point,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) anchor: Option<String>,
}

impl DiagramNote {
    pub(crate) fn new(text: String, position: Point, anchor: Option<String>) -> Self {
        Self {
            text,
            position,
            width: DIAGRAM_NOTE_WIDTH,
            height: DIAGRAM_NOTE_HEIGHT,
            anchor,
        }
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        let max_chars = ((self.width - 20.0) / 6.0).max(10.0) as usize;
        let mut lines = Vec::new();
        for paragraph in self.text.split('\n') {
            let mut current = String::new();
            for word in paragraph.split_whitespace() {
                if current.is_empty() {
                    current = word.to_string();
                } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    lines.push(std::mem::take(&mut current));
                    current = word.to_string();
                }
            }
            lines.push(current);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    pub(crate) fn bounds(&self) -> Rectangle {
        let rows = self.lines().len().max(1) as f32;
        Rectangle::new(
            self.position,
            Size::new(
                self.width.max(DIAGRAM_NOTE_MIN_WIDTH),
                self.height
                    .max(DIAGRAM_NOTE_MIN_HEIGHT)
                    .max(26.0 + rows * DIAGRAM_NOTE_LINE + 10.0),
            ),
        )
    }

    pub(crate) fn header(&self) -> Rectangle {
        Rectangle::new(self.position, Size::new(self.bounds().width, 22.0))
    }

    pub(crate) fn resize_handle(&self) -> Rectangle {
        let bounds = self.bounds();
        Rectangle::new(
            Point::new(
                bounds.x + bounds.width - DIAGRAM_NOTE_HANDLE,
                bounds.y + bounds.height - DIAGRAM_NOTE_HANDLE,
            ),
            Size::new(DIAGRAM_NOTE_HANDLE, DIAGRAM_NOTE_HANDLE),
        )
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SchemaDiagram {
    pub(crate) tables: Vec<DiagramTable>,
    pub(crate) edges: Vec<DiagramEdge>,
    pub(crate) groups: Vec<DiagramGroup>,
    pub(crate) notes: Vec<DiagramNote>,
}

fn lookup_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

#[cfg(test)]
fn short_key(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase()
}

impl SchemaDiagram {
    pub(crate) fn from_data(data: SchemaDiagramData) -> Self {
        let mut diagram = Self {
            tables: data
                .tables
                .into_iter()
                .map(|table| DiagramTable {
                    name: table.name,
                    columns: table.columns,
                    position: Point::ORIGIN,
                    collapsed: false,
                })
                .collect(),
            edges: data
                .edges
                .into_iter()
                .map(|edge| DiagramEdge {
                    from: edge.from,
                    from_column: edge.from_column,
                    to: edge.to,
                    to_column: edge.to_column,
                    constraint_name: edge.constraint_name,
                    one_to_one: edge.one_to_one,
                })
                .collect(),
            ..Self::default()
        };
        diagram.auto_layout();
        diagram
    }

    #[cfg(test)]
    pub(crate) fn build(
        tables: Vec<(String, Vec<DiagramColumn>)>,
        relations: &[SidebarRelationEntry],
    ) -> Self {
        let mut diagram = Self {
            tables: tables
                .into_iter()
                .map(|(name, columns)| DiagramTable {
                    name,
                    columns,
                    position: Point::ORIGIN,
                    collapsed: false,
                })
                .collect(),
            ..Self::default()
        };

        diagram.rebuild_edges(relations);
        diagram.auto_layout();
        diagram
    }

    #[cfg(test)]
    pub(crate) fn rebuild_edges(&mut self, relations: &[SidebarRelationEntry]) {
        self.edges.clear();
        for table in &mut self.tables {
            for column in &mut table.columns {
                column.foreign = false;
            }
        }

        let mut index_by_name: HashMap<String, usize> = HashMap::new();
        for (index, table) in self.tables.iter().enumerate() {
            index_by_name.insert(lookup_key(&table.name), index);
            index_by_name.entry(short_key(&table.name)).or_insert(index);
        }

        for entry in relations {
            let Some(from) = index_by_name
                .get(&lookup_key(&entry.table))
                .or_else(|| index_by_name.get(&short_key(&entry.table)))
                .copied()
            else {
                continue;
            };
            let Some(to) = index_by_name
                .get(&lookup_key(&entry.relation.referenced_table))
                .or_else(|| index_by_name.get(&short_key(&entry.relation.referenced_table)))
                .copied()
            else {
                continue;
            };
            let column = lookup_key(&entry.relation.column);
            let referenced = lookup_key(&entry.relation.referenced_column);
            let Some(from_column) = self.tables[from]
                .columns
                .iter()
                .position(|item| lookup_key(&item.name) == column)
            else {
                continue;
            };
            let to_column = self.tables[to]
                .columns
                .iter()
                .position(|item| lookup_key(&item.name) == referenced)
                .unwrap_or(0);

            self.tables[from].columns[from_column].foreign = true;
            let one_to_one = self.tables[from].columns[from_column].unique;
            self.edges.push(DiagramEdge {
                from,
                from_column,
                to,
                to_column,
                constraint_name: entry.constraint_name.clone(),
                one_to_one,
            });
        }
    }

    pub(crate) fn auto_layout(&mut self) {
        let all = (0..self.tables.len()).collect::<Vec<_>>();
        self.layout_subset(&all, Point::ORIGIN);
        self.sync_group_members();
    }

    fn layout_subset(&mut self, subset: &[usize], origin: Point) -> Rectangle {
        let count = subset.len();
        if count == 0 {
            return Rectangle::new(origin, Size::new(0.0, 0.0));
        }
        let local: HashMap<usize, usize> = subset
            .iter()
            .enumerate()
            .map(|(local, global)| (*global, local))
            .collect();

        let mut parents: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut degree = vec![0usize; count];
        for edge in &self.edges {
            if edge.from == edge.to {
                continue;
            }
            let (Some(from), Some(to)) = (local.get(&edge.from), local.get(&edge.to)) else {
                continue;
            };
            parents[*from].push(*to);
            degree[*from] += 1;
            degree[*to] += 1;
        }

        let mut level = vec![0usize; count];
        for _ in 0..count.min(24) {
            let mut changed = false;
            for index in 0..count {
                let next = parents[index]
                    .iter()
                    .map(|parent| level[*parent] + 1)
                    .max()
                    .unwrap_or(0);
                if next > level[index] && next < count {
                    level[index] = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut columns: Vec<Vec<usize>> = vec![Vec::new(); level.iter().max().unwrap_or(&0) + 1];
        for (index, level) in level.iter().enumerate() {
            columns[*level].push(index);
        }

        let mut row_of = vec![0.0f32; count];
        for column in columns.iter_mut() {
            column.sort_by(|a, b| {
                let key = |index: usize| {
                    let barycentre = parents[index]
                        .iter()
                        .map(|parent| row_of[*parent])
                        .sum::<f32>()
                        / parents[index].len().max(1) as f32;
                    (barycentre, std::cmp::Reverse(degree[index]))
                };
                let (left, left_degree) = key(*a);
                let (right, right_degree) = key(*b);
                left.total_cmp(&right).then(left_degree.cmp(&right_degree))
            });
            for (row, index) in column.iter().enumerate() {
                row_of[*index] = row as f32;
            }
        }

        let mut x = origin.x;
        let mut height = 0.0f32;
        for column in &columns {
            let mut y = origin.y;
            for index in column {
                self.tables[subset[*index]].position = Point::new(x, y);
                y += self.tables[subset[*index]].size().height + DIAGRAM_NODE_GAP_Y;
            }
            height = height.max(y - origin.y - DIAGRAM_NODE_GAP_Y);
            x += DIAGRAM_NODE_WIDTH + DIAGRAM_NODE_GAP_X;
        }

        Rectangle::new(
            origin,
            Size::new(
                (x - origin.x - DIAGRAM_NODE_GAP_X).max(DIAGRAM_NODE_WIDTH),
                height.max(0.0),
            ),
        )
    }

    pub(crate) fn smart_layout(&mut self) {
        if self.tables.is_empty() {
            return;
        }
        let mut placed = vec![false; self.tables.len()];
        let mut clusters: Vec<Vec<usize>> = Vec::new();

        for group in &self.groups {
            let members: Vec<usize> = group
                .members
                .iter()
                .filter_map(|name| {
                    let index = self.tables.iter().position(|table| table.name == *name)?;
                    (!placed[index]).then_some(index)
                })
                .collect();
            for index in &members {
                placed[*index] = true;
            }
            if !members.is_empty() {
                clusters.push(members);
            }
        }

        let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); self.tables.len()];
        for edge in &self.edges {
            if edge.from != edge.to {
                neighbours[edge.from].push(edge.to);
                neighbours[edge.to].push(edge.from);
            }
        }
        for index in 0..self.tables.len() {
            if placed[index] {
                continue;
            }
            let mut component = vec![index];
            placed[index] = true;
            let mut queue = vec![index];
            while let Some(current) = queue.pop() {
                for next in &neighbours[current] {
                    if !placed[*next] {
                        placed[*next] = true;
                        component.push(*next);
                        queue.push(*next);
                    }
                }
            }
            clusters.push(component);
        }

        clusters.sort_by_key(|cluster| std::cmp::Reverse(cluster.len()));

        let mut cursor = Point::ORIGIN;
        let mut row_height = 0.0f32;
        for cluster in &clusters {
            let bounds = self.layout_subset(cluster, cursor);
            row_height = row_height.max(bounds.height);
            cursor.x += bounds.width + DIAGRAM_CLUSTER_GAP;
            if cursor.x > DIAGRAM_CLUSTER_ROW_WIDTH {
                cursor = Point::new(0.0, cursor.y + row_height + DIAGRAM_CLUSTER_GAP);
                row_height = 0.0;
            }
        }

        for index in 0..self.groups.len() {
            self.fit_group(index);
        }
        self.sync_group_members();
    }

    pub(crate) fn fit_group(&mut self, index: usize) {
        let Some(group) = self.groups.get(index) else {
            return;
        };
        if group.collapsed {
            return;
        }
        let Some(bounds) = group
            .members
            .iter()
            .filter_map(|name| self.tables.iter().find(|table| table.name == *name))
            .map(DiagramTable::bounds)
            .reduce(|left, right| left.union(&right))
        else {
            return;
        };
        self.groups[index].bounds = Rectangle::new(
            Point::new(bounds.x - 24.0, bounds.y - 44.0),
            Size::new(bounds.width + 48.0, bounds.height + 68.0),
        );
    }

    pub(crate) fn sync_group_members(&mut self) {
        for index in 0..self.groups.len() {
            if self.groups[index].collapsed {
                continue;
            }
            let members: Vec<String> = self
                .tables
                .iter()
                .filter(|table| self.groups[index].captures(table))
                .map(|table| table.name.clone())
                .collect();
            self.groups[index].members = members;
        }
    }

    pub(crate) fn free_note_origin(&self, preferred: Point, size: Size) -> Point {
        let taken = |candidate: Point| {
            let rect = Rectangle::new(candidate, size);
            self.tables
                .iter()
                .map(DiagramTable::bounds)
                .chain(self.notes.iter().map(DiagramNote::bounds))
                .any(|other| other.intersects(&rect))
        };
        for column in 0..4 {
            for row in 0..6 {
                let candidate = Point::new(
                    preferred.x + column as f32 * (size.width + 30.0),
                    preferred.y + row as f32 * (size.height + 24.0),
                );
                if !taken(candidate) {
                    return candidate;
                }
            }
        }
        preferred
    }

    pub(crate) fn group_index(&self, name: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|group| group.name.eq_ignore_ascii_case(name.trim()))
    }

    pub(crate) fn plan_group(&mut self, name: String, members: &[usize]) -> Vec<(usize, Point)> {
        if members.is_empty() {
            return Vec::new();
        }
        let origin = Point::new(
            0.0,
            self.content_bounds()
                .map(|bounds| bounds.y + bounds.height + DIAGRAM_CLUSTER_GAP)
                .unwrap_or(0.0),
        );
        let columns = (members.len() as f32).sqrt().ceil().max(1.0) as usize;
        let mut plan = Vec::with_capacity(members.len());
        let mut x = origin.x;
        let mut y = origin.y;
        let mut row_height = 0.0f32;
        for (position, member) in members.iter().enumerate() {
            let Some(table) = self.tables.get(*member) else {
                continue;
            };
            if position % columns == 0 && position > 0 {
                x = origin.x;
                y += row_height + DIAGRAM_NODE_GAP_Y;
                row_height = 0.0;
            }
            plan.push((*member, Point::new(x, y)));
            row_height = row_height.max(table.size().height);
            x += DIAGRAM_NODE_WIDTH + DIAGRAM_NODE_GAP_X;
        }

        let index = match self.group_index(&name) {
            Some(index) => index,
            None => {
                let mut group = DiagramGroup::new(name, Rectangle::default());
                group.color = self.groups.len() as u8;
                self.groups.push(group);
                self.groups.len() - 1
            }
        };
        self.groups[index].collapsed = false;
        self.groups[index].members = plan
            .iter()
            .filter_map(|(member, _)| self.tables.get(*member))
            .map(|table| table.name.clone())
            .collect();
        let box_bounds = plan
            .iter()
            .filter_map(|(member, position)| {
                let size = self.tables.get(*member)?.size();
                Some(Rectangle::new(*position, size))
            })
            .reduce(|left, right| left.union(&right));
        if let Some(bounds) = box_bounds {
            self.groups[index].bounds = Rectangle::new(
                Point::new(bounds.x - 24.0, bounds.y - 44.0),
                Size::new(bounds.width + 48.0, bounds.height + 68.0),
            );
        }
        plan
    }

    pub(crate) fn group_of(&self, table: &str) -> Option<usize> {
        self.groups.iter().position(|group| group.holds(table))
    }

    pub(crate) fn move_group(&mut self, index: usize, position: Point) {
        let Some(group) = self.groups.get(index) else {
            return;
        };
        let delta = position - group.bounds.position();
        let members = group.members.clone();
        for name in members {
            if let Some(table) = self.table_index(&name) {
                self.tables[table].position += delta;
            }
        }
        if let Some(group) = self.groups.get_mut(index) {
            group.bounds.x = position.x;
            group.bounds.y = position.y;
        }
    }

    pub(crate) fn resize_group(&mut self, index: usize, corner: Point) {
        let Some(group) = self.groups.get_mut(index) else {
            return;
        };
        group.bounds.width = (corner.x - group.bounds.x).max(DIAGRAM_GROUP_MIN.width);
        group.bounds.height = (corner.y - group.bounds.y).max(DIAGRAM_GROUP_MIN.height);
    }

    pub(crate) fn table_is_hidden(&self, index: usize) -> bool {
        let Some(table) = self.tables.get(index) else {
            return true;
        };
        self.groups
            .iter()
            .any(|group| group.collapsed && group.holds(&table.name))
    }

    pub(crate) fn content_bounds(&self) -> Option<Rectangle> {
        let boxes = self
            .tables
            .iter()
            .map(DiagramTable::bounds)
            .chain(self.groups.iter().map(|group| group.bounds))
            .chain(self.notes.iter().map(DiagramNote::bounds));
        let mut result: Option<Rectangle> = None;
        for item in boxes {
            result = Some(match result {
                None => item,
                Some(current) => {
                    let min_x = current.x.min(item.x);
                    let min_y = current.y.min(item.y);
                    let max_x = (current.x + current.width).max(item.x + item.width);
                    let max_y = (current.y + current.height).max(item.y + item.height);
                    Rectangle::new(
                        Point::new(min_x, min_y),
                        Size::new(max_x - min_x, max_y - min_y),
                    )
                }
            });
        }
        result
    }

    pub(crate) fn table_index(&self, name: &str) -> Option<usize> {
        let key = lookup_key(name);
        self.tables
            .iter()
            .position(|table| lookup_key(&table.name) == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DiagramCommand {
    Fit,
    Zoom(f32),
    Focus(usize),
    Follow { point: Point, zoom: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DiagramTarget {
    Table(usize),
    Group(usize),
    Note(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct DiagramMenu {
    pub(crate) position: Point,
    pub(crate) target: Option<DiagramTarget>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiagramLink {
    pub(crate) from: usize,
    pub(crate) from_column: usize,
    pub(crate) cursor: Point,
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaScriptError {
    pub(crate) applied: usize,
    pub(crate) message: String,
}

impl SchemaScriptError {
    pub(crate) fn new(applied: usize, message: impl std::fmt::Display) -> Self {
        Self {
            applied,
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SchemaChange {
    CreateTable {
        table: String,
        columns: Vec<(String, String, bool)>,
    },
    DropTable {
        table: String,
    },
    RenameTable {
        table: String,
        new_table: String,
    },
    AddColumn {
        table: String,
        column: String,
        data_type: String,
    },
    DropColumn {
        table: String,
        column: String,
    },
    AlterColumn {
        table: String,
        column: String,
        data_type: String,
        nullable: bool,
        default: Option<String>,
        attributes: String,
    },
    AddForeignKey {
        table: String,
        columns: Vec<String>,
        referenced_table: String,
        referenced_columns: Vec<String>,
    },
    DropForeignKey {
        table: String,
        column: String,
        referenced_table: String,
        referenced_column: String,
        constraint_name: String,
    },
}

pub(crate) fn foreign_key_name(table: &str, columns: &[String]) -> String {
    format!(
        "fk_{}_{}",
        table.rsplit('.').next().unwrap_or(table),
        columns.join("_")
    )
}

impl SchemaChange {
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::CreateTable { table, columns } => crate::i18n::tr_with(
                "Create table {table} with {count} column(s)",
                &[("{table}", table), ("{count}", &columns.len().to_string())],
            ),
            Self::DropTable { table } => {
                crate::i18n::tr_with("Drop table {table}", &[("{table}", table)])
            }
            Self::RenameTable { table, new_table } => crate::i18n::tr_with(
                "Rename table {table} to {new_table}",
                &[("{table}", table), ("{new_table}", new_table)],
            ),
            Self::AddColumn {
                table,
                column,
                data_type,
            } => crate::i18n::tr_with(
                "Add column {column} {type} to {table}",
                &[
                    ("{column}", column),
                    ("{type}", data_type),
                    ("{table}", table),
                ],
            ),
            Self::DropColumn { table, column } => crate::i18n::tr_with(
                "Drop column {column} from {table}",
                &[("{column}", column), ("{table}", table)],
            ),
            Self::AlterColumn {
                table,
                column,
                data_type,
                ..
            } => crate::i18n::tr_with(
                "Alter column {column} on {table} to {type}",
                &[
                    ("{column}", column),
                    ("{table}", table),
                    ("{type}", data_type),
                ],
            ),
            Self::AddForeignKey {
                table,
                columns,
                referenced_table,
                referenced_columns,
            } => crate::i18n::tr_with(
                "Link {table}.{column} to {target}.{target_column}",
                &[
                    ("{table}", table),
                    ("{column}", &columns.join(", ")),
                    ("{target}", referenced_table),
                    ("{target_column}", &referenced_columns.join(", ")),
                ],
            ),
            Self::DropForeignKey { table, column, .. } => crate::i18n::tr_with(
                "Remove the foreign key on {table}.{column}",
                &[("{table}", table), ("{column}", column)],
            ),
        }
    }

    fn uses_table(&self, name: &str) -> bool {
        match self {
            Self::CreateTable { table, .. }
            | Self::DropTable { table }
            | Self::RenameTable { table, .. }
            | Self::AddColumn { table, .. }
            | Self::DropColumn { table, .. }
            | Self::AlterColumn { table, .. } => table.eq_ignore_ascii_case(name),
            Self::AddForeignKey {
                table,
                referenced_table,
                ..
            }
            | Self::DropForeignKey {
                table,
                referenced_table,
                ..
            } => table.eq_ignore_ascii_case(name) || referenced_table.eq_ignore_ascii_case(name),
        }
    }

    fn depends_on(&self, change: &Self) -> bool {
        match change {
            Self::CreateTable { table, .. } => self.uses_table(table),
            Self::RenameTable { new_table, .. } => self.uses_table(new_table),
            Self::AddColumn { table, column, .. } => match self {
                Self::DropColumn {
                    table: next_table,
                    column: next_column,
                }
                | Self::AlterColumn {
                    table: next_table,
                    column: next_column,
                    ..
                } => {
                    next_table.eq_ignore_ascii_case(table)
                        && next_column.eq_ignore_ascii_case(column)
                }
                Self::AddForeignKey {
                    table: next_table,
                    columns: next_columns,
                    ..
                } => {
                    next_table.eq_ignore_ascii_case(table)
                        && next_columns
                            .iter()
                            .any(|next| next.eq_ignore_ascii_case(column))
                }
                _ => false,
            },
            Self::AddForeignKey {
                table,
                columns,
                referenced_table,
                referenced_columns,
            } => matches!(
                self,
                Self::DropForeignKey {
                    table: next_table,
                    column: next_column,
                    referenced_table: next_referenced,
                    referenced_column: next_referenced_column,
                    ..
                } if next_table.eq_ignore_ascii_case(table)
                    && columns.iter().any(|name| name.eq_ignore_ascii_case(next_column))
                    && next_referenced.eq_ignore_ascii_case(referenced_table)
                    && referenced_columns
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(next_referenced_column))
            ),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagramModalKind {
    CreateTable,
    RenameTable,
    AddColumn,
    AddGroup,
    RenameGroup,
    AddNote,
    EditNote,
    DropColumn,
    AlterColumn,
    SaveDiagramAs,
    DeleteSavedDiagram,
}

#[derive(Debug)]
pub(crate) struct DiagramModal {
    pub(crate) kind: DiagramModalKind,
    pub(crate) table: Option<usize>,
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) note: iced::widget::text_editor::Content,
    pub(crate) error: Option<String>,
}

impl DiagramModal {
    pub(crate) fn new(kind: DiagramModalKind, table: Option<usize>) -> Self {
        Self {
            kind,
            table,
            name: String::new(),
            detail: String::new(),
            note: iced::widget::text_editor::Content::new(),
            error: None,
        }
    }

    pub(crate) fn title(&self) -> &'static str {
        match self.kind {
            DiagramModalKind::CreateTable => "New table",
            DiagramModalKind::RenameTable => "Rename table",
            DiagramModalKind::AddColumn => "Add column",
            DiagramModalKind::AddGroup => "New area",
            DiagramModalKind::RenameGroup => "Rename area",
            DiagramModalKind::AddNote => "New note",
            DiagramModalKind::EditNote => "Edit note",
            DiagramModalKind::DropColumn => "Drop a column",
            DiagramModalKind::AlterColumn => "Alter a column",
            DiagramModalKind::SaveDiagramAs => "Save diagram",
            DiagramModalKind::DeleteSavedDiagram => "Delete saved diagram",
        }
    }
}

#[derive(Debug)]
pub(crate) struct DiagramState {
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) diagram: SchemaDiagram,
    pub(crate) offset: Vector,
    pub(crate) zoom: f32,
    pub(crate) selected: Option<usize>,
    pub(crate) hovered_edge: Option<usize>,
    pub(crate) pending: Option<DiagramCommand>,
    pub(crate) snap: bool,
    pub(crate) fullscreen: bool,
    pub(crate) search: String,
    pub(crate) search_open: bool,
    pub(crate) search_anim: f32,
    pub(crate) menu: Option<DiagramMenu>,
    pub(crate) link: Option<DiagramLink>,
    pub(crate) changes: Vec<SchemaChange>,
    pub(crate) undone_changes: Vec<SchemaChange>,
    pub(crate) modal: Option<DiagramModal>,
    pub(crate) ddl_preview: Option<String>,
    pub(crate) applying: bool,
    pub(crate) relations: DiagramRelations,
    pub(crate) matches: Vec<usize>,
    pub(crate) match_index: usize,
    pub(crate) bounds: Option<Rectangle>,
    pub(crate) table_boxes: Vec<Rectangle>,
    pub(crate) edge_boxes: Vec<Rectangle>,
    pub(crate) hidden_tables: Vec<bool>,
    pub(crate) agent: Option<DiagramAgent>,
}

#[derive(Debug, Clone)]
pub(crate) struct DiagramAgent {
    pub(crate) table: usize,
    pub(crate) from: Point,
    pub(crate) to: Point,
    pub(crate) label: String,
    pub(crate) progress: f32,
    pub(crate) dragging: bool,
    pub(crate) target: Option<Point>,
    pub(crate) queue: Vec<(usize, Point)>,
}

impl DiagramAgent {
    pub(crate) fn grab(bounds: Rectangle) -> Point {
        Point::new(bounds.x + bounds.width - 14.0, bounds.y + 14.0)
    }

    pub(crate) fn visiting(table: usize, from: Point, to: Point, label: String) -> Self {
        Self {
            table,
            from,
            to,
            label,
            progress: 0.0,
            dragging: false,
            target: None,
            queue: Vec::new(),
        }
    }
}

impl DiagramAgent {
    pub(crate) fn position(&self) -> Point {
        let t = self.progress.clamp(0.0, 1.0);
        let eased = if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - 2.0 * (1.0 - t) * (1.0 - t)
        };
        Point::new(
            self.from.x + (self.to.x - self.from.x) * eased,
            self.from.y + (self.to.y - self.from.y) * eased,
        )
    }

    pub(crate) fn arrived(&self) -> bool {
        self.progress >= 1.0
    }
}

impl Default for DiagramState {
    fn default() -> Self {
        Self {
            loading: true,
            error: None,
            diagram: SchemaDiagram::default(),
            offset: Vector::new(0.0, 0.0),
            zoom: 1.0,
            selected: None,
            hovered_edge: None,
            pending: None,
            snap: false,
            fullscreen: false,
            search: String::new(),
            search_open: false,
            search_anim: 0.0,
            menu: None,
            link: None,
            changes: Vec::new(),
            undone_changes: Vec::new(),
            modal: None,
            ddl_preview: None,
            applying: false,
            relations: DiagramRelations::Auto,
            matches: Vec::new(),
            match_index: 0,
            bounds: None,
            table_boxes: Vec::new(),
            edge_boxes: Vec::new(),
            hidden_tables: Vec::new(),
            agent: None,
        }
    }
}

impl DiagramState {
    pub(crate) fn record_change(&mut self, change: SchemaChange) {
        if !self.changes.contains(&change) {
            self.changes.push(change);
            self.undone_changes.clear();
        }
    }

    pub(crate) fn undo_change(&mut self) -> bool {
        let Some(change) = self.changes.pop() else {
            return false;
        };
        self.undone_changes.push(change);
        self.ddl_preview = None;
        true
    }

    pub(crate) fn redo_change(&mut self) -> bool {
        let Some(change) = self.undone_changes.pop() else {
            return false;
        };
        self.changes.push(change);
        self.ddl_preview = None;
        true
    }

    pub(crate) fn remove_change(&mut self, index: usize) -> bool {
        let Some(change) = self.changes.get(index) else {
            return false;
        };
        if self.changes[index + 1..]
            .iter()
            .any(|next| next.depends_on(change))
        {
            return false;
        }
        self.changes.remove(index);
        self.undone_changes.clear();
        self.ddl_preview = None;
        true
    }

    pub(crate) fn related_tables(&self, index: usize) -> usize {
        self.diagram
            .edges
            .iter()
            .filter(|edge| edge.from == index || edge.to == index)
            .count()
    }

    pub(crate) fn refresh_derived(&mut self) {
        self.matches = self.search_matches();
        self.match_index = self.match_index.min(self.matches.len().saturating_sub(1));
        self.bounds = self.diagram.content_bounds();
        self.hidden_tables = (0..self.diagram.tables.len())
            .map(|index| self.diagram.table_is_hidden(index))
            .collect();
        self.table_boxes = self
            .diagram
            .tables
            .iter()
            .map(DiagramTable::bounds)
            .collect();
        self.edge_boxes = self
            .diagram
            .edges
            .iter()
            .map(|edge| {
                let (Some(from), Some(to)) = (
                    self.table_boxes.get(edge.from),
                    self.table_boxes.get(edge.to),
                ) else {
                    return Rectangle::new(Point::ORIGIN, Size::ZERO);
                };
                let left = from.x.min(to.x) - 24.0;
                let top = from.y.min(to.y) - 24.0;
                Rectangle::new(
                    Point::new(left, top),
                    Size::new(
                        (from.x + from.width).max(to.x + to.width) - left + 24.0,
                        (from.y + from.height).max(to.y + to.height) - top + 24.0,
                    ),
                )
            })
            .collect();
    }

    pub(crate) fn table_is_hidden(&self, index: usize) -> bool {
        self.hidden_tables.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn shows_all_relations(&self) -> bool {
        match self.relations {
            DiagramRelations::All => true,
            DiagramRelations::Selected => false,
            DiagramRelations::Auto => self.diagram.edges.len() <= DIAGRAM_BUSY_EDGES,
        }
    }

    pub(crate) fn edge_is_visible(&self, index: usize) -> bool {
        let Some(edge) = self.diagram.edges.get(index) else {
            return false;
        };
        if self.table_is_hidden(edge.from) || self.table_is_hidden(edge.to) {
            return false;
        }
        if self.hovered_edge == Some(index) {
            return true;
        }
        match self.selected {
            Some(table) => edge.from == table || edge.to == table,
            None => self.shows_all_relations(),
        }
    }

    fn search_matches(&self) -> Vec<usize> {
        let needle = self.search.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.diagram
            .tables
            .iter()
            .enumerate()
            .filter(|(_, table)| {
                table.name.to_ascii_lowercase().contains(&needle)
                    || table
                        .columns
                        .iter()
                        .any(|column| column.name.to_ascii_lowercase().contains(&needle))
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn snapped(&self, point: Point) -> Point {
        if !self.snap {
            return point;
        }
        Point::new(
            (point.x / DIAGRAM_GRID_STEP).round() * DIAGRAM_GRID_STEP,
            (point.y / DIAGRAM_GRID_STEP).round() * DIAGRAM_GRID_STEP,
        )
    }
}

pub(crate) struct DiagramTab {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) pinned: bool,
    pub(crate) state: DiagramState,
}

impl StoredDiagram {
    pub(crate) fn capture(name: &str, state: &DiagramState) -> Self {
        Self {
            name: name.to_string(),
            open: false,
            offset_x: state.offset.x,
            offset_y: state.offset.y,
            zoom: state.zoom,
            snap: state.snap,
            fullscreen: state.fullscreen,
            relations: state.relations,
            tables: state
                .diagram
                .tables
                .iter()
                .map(|table| StoredDiagramTable {
                    name: table.name.clone(),
                    x: table.position.x,
                    y: table.position.y,
                    collapsed: table.collapsed,
                })
                .collect(),
            groups: state
                .diagram
                .groups
                .iter()
                .map(|group| StoredDiagramGroup {
                    name: group.name.clone(),
                    color: group.color,
                    x: group.bounds.x,
                    y: group.bounds.y,
                    width: group.bounds.width,
                    height: group.bounds.height,
                    locked: group.locked,
                    contents_locked: group.contents_locked,
                    collapsed: group.collapsed,
                    members: group.members.clone(),
                })
                .collect(),
            notes: state
                .diagram
                .notes
                .iter()
                .map(|note| StoredDiagramNote {
                    text: note.text.clone(),
                    x: note.position.x,
                    y: note.position.y,
                    width: note.width,
                    height: note.height,
                    anchor: note.anchor.clone(),
                })
                .collect(),
        }
    }

    pub(crate) fn restore(&self, state: &mut DiagramState) {
        let positions: HashMap<String, &StoredDiagramTable> = self
            .tables
            .iter()
            .map(|table| (lookup_key(&table.name), table))
            .collect();
        for table in &mut state.diagram.tables {
            if let Some(stored) = positions.get(&lookup_key(&table.name)) {
                table.position = Point::new(stored.x, stored.y);
                table.collapsed = stored.collapsed;
            }
        }
        state.diagram.groups = self
            .groups
            .iter()
            .map(|group| DiagramGroup {
                name: group.name.clone(),
                color: group.color,
                bounds: Rectangle::new(
                    Point::new(group.x, group.y),
                    Size::new(group.width, group.height),
                ),
                locked: group.locked,
                contents_locked: group.contents_locked,
                collapsed: group.collapsed,
                members: group.members.clone(),
            })
            .collect();
        state.diagram.notes = self
            .notes
            .iter()
            .map(|note| {
                let mut restored = DiagramNote::new(
                    note.text.clone(),
                    Point::new(note.x, note.y),
                    note.anchor.clone(),
                );
                if note.width >= DIAGRAM_NOTE_MIN_WIDTH {
                    restored.width = note.width;
                }
                if note.height >= DIAGRAM_NOTE_MIN_HEIGHT {
                    restored.height = note.height;
                }
                restored
            })
            .collect();
        state.snap = self.snap;
        state.fullscreen = self.fullscreen;
        state.relations = self.relations;
        if self.zoom >= DIAGRAM_MIN_ZOOM && self.zoom <= DIAGRAM_MAX_ZOOM {
            state.zoom = self.zoom;
            state.offset = Vector::new(self.offset_x, self.offset_y);
            state.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationInfo;

    fn column(name: &str) -> DiagramColumn {
        DiagramColumn {
            name: name.to_string(),
            data_type: String::from("int"),
            primary: name == "id",
            foreign: false,
            nullable: name != "id",
            default: None,
            indexed: name == "id",
            unique: name == "id",
            attributes: String::new(),
        }
    }

    fn sample() -> SchemaDiagram {
        SchemaDiagram::build(
            vec![
                (
                    String::from("public.orders"),
                    vec![column("id"), column("customer_id")],
                ),
                (String::from("public.customers"), vec![column("id")]),
                (String::from("public.audit"), vec![column("id")]),
            ],
            &[SidebarRelationEntry {
                table: String::from("public.orders"),
                relation: RelationInfo {
                    column: String::from("customer_id"),
                    referenced_table: String::from("customers"),
                    referenced_column: String::from("id"),
                },
                constraint_name: String::from("orders_customer_id_fkey"),
            }],
        )
    }

    #[test]
    fn pending_changes_can_be_undone_and_redone() {
        let mut state = DiagramState::default();
        let change = SchemaChange::DropTable {
            table: String::from("users"),
        };

        state.record_change(change.clone());
        assert!(state.undo_change());
        assert!(state.changes.is_empty());
        assert_eq!(state.undone_changes, vec![change.clone()]);
        assert!(state.redo_change());
        assert_eq!(state.changes, vec![change.clone()]);
        assert!(state.undone_changes.is_empty());
        assert!(state.undo_change());
        state.record_change(SchemaChange::DropTable {
            table: String::from("orders"),
        });
        assert!(state.undone_changes.is_empty());
    }

    #[test]
    fn dependent_pending_changes_are_removed_last() {
        let mut state = DiagramState::default();
        state.record_change(SchemaChange::CreateTable {
            table: String::from("users"),
            columns: vec![(String::from("id"), String::from("INTEGER"), true)],
        });
        state.record_change(SchemaChange::AddColumn {
            table: String::from("users"),
            column: String::from("name"),
            data_type: String::from("TEXT"),
        });

        assert!(!state.remove_change(0));
        assert!(state.remove_change(1));
        assert!(state.remove_change(0));
        assert!(state.changes.is_empty());
    }

    #[test]
    fn relations_resolve_to_columns_even_when_the_schema_prefix_is_missing() {
        let diagram = sample();
        assert_eq!(diagram.edges.len(), 1);
        assert_eq!(diagram.edges[0].from, 0);
        assert_eq!(diagram.edges[0].from_column, 1);
        assert_eq!(diagram.edges[0].to, 1);
        assert!(diagram.tables[0].columns[1].foreign);
    }

    #[test]
    fn layout_keeps_every_table_in_its_own_space() {
        let diagram = sample();
        for (index, table) in diagram.tables.iter().enumerate() {
            for other in diagram.tables.iter().skip(index + 1) {
                assert!(!table.bounds().intersects(&other.bounds()));
            }
        }
        assert!(
            diagram
                .content_bounds()
                .is_some_and(|bounds| bounds.width >= DIAGRAM_NODE_WIDTH)
        );
    }

    #[test]
    fn stored_layout_round_trips_positions_collapse_groups_and_notes() {
        let mut state = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        state.diagram.tables[0].position = Point::new(120.0, 340.0);
        state.diagram.tables[0].collapsed = true;
        state.diagram.groups.push(DiagramGroup::new(
            String::from("Billing"),
            Rectangle::new(Point::new(10.0, 20.0), Size::new(400.0, 300.0)),
        ));
        state.diagram.groups[0].color = 3;
        state.diagram.notes.push(DiagramNote::new(
            String::from("check indexes"),
            Point::new(5.0, 6.0),
            None,
        ));
        state.diagram.notes[0].width = 320.0;
        state.diagram.notes[0].height = 180.0;
        state.snap = true;
        state.fullscreen = true;
        state.relations = DiagramRelations::Selected;
        state.zoom = 0.75;
        state.offset = Vector::new(-40.0, 12.0);

        let stored = StoredDiagram::capture("New Diagram #1", &state);

        let mut restored = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        stored.restore(&mut restored);

        assert_eq!(
            restored.diagram.tables[0].position,
            Point::new(120.0, 340.0)
        );
        assert!(restored.diagram.tables[0].collapsed);
        assert_eq!(restored.diagram.groups.len(), 1);
        assert_eq!(restored.diagram.groups[0].color, 3);
        assert_eq!(restored.diagram.notes[0].text, "check indexes");
        assert_eq!(restored.diagram.notes[0].width, 320.0);
        assert_eq!(restored.diagram.notes[0].height, 180.0);
        assert!(restored.snap);
        assert!(restored.fullscreen);
        assert_eq!(restored.relations, DiagramRelations::Selected);
        assert_eq!(restored.zoom, 0.75);
        assert_eq!(restored.offset, Vector::new(-40.0, 12.0));
    }

    fn busy_state(tables: usize) -> DiagramState {
        let specs: Vec<(String, Vec<DiagramColumn>)> = (0..tables)
            .map(|index| (format!("t{index}"), vec![column("id"), column("parent_id")]))
            .collect();
        let relations: Vec<SidebarRelationEntry> = (1..tables)
            .map(|index| SidebarRelationEntry {
                table: format!("t{index}"),
                relation: RelationInfo {
                    column: String::from("parent_id"),
                    referenced_table: format!("t{}", index - 1),
                    referenced_column: String::from("id"),
                },
                constraint_name: format!("fk_t{index}_parent_id"),
            })
            .collect();
        DiagramState {
            diagram: SchemaDiagram::build(specs, &relations),
            ..DiagramState::default()
        }
    }

    #[test]
    fn a_busy_schema_hides_relations_until_something_is_selected() {
        let mut state = busy_state(DIAGRAM_BUSY_EDGES + 20);
        assert!(state.diagram.edges.len() > DIAGRAM_BUSY_EDGES);
        assert!(!state.shows_all_relations());
        assert!(!state.edge_is_visible(0));

        state.selected = Some(0);
        assert!(state.edge_is_visible(0));
        assert!(!state.edge_is_visible(40));

        state.selected = None;
        state.hovered_edge = Some(7);
        assert!(state.edge_is_visible(7));

        state.relations = DiagramRelations::All;
        assert!(state.edge_is_visible(40));
        state.relations = DiagramRelations::Selected;
        assert!(!state.edge_is_visible(40));
    }

    #[test]
    fn a_small_schema_keeps_every_relation_on_screen() {
        let state = busy_state(6);
        assert!(state.shows_all_relations());
        assert!(state.edge_is_visible(0));
    }

    #[test]
    fn derived_state_is_refreshed_instead_of_recomputed_per_frame() {
        let mut state = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        assert!(state.matches.is_empty());
        assert!(state.bounds.is_none());

        state.search = String::from("customer");
        state.refresh_derived();

        assert_eq!(state.matches, vec![0, 1]);
        assert!(state.bounds.is_some_and(|bounds| bounds.width > 0.0));
    }

    #[test]
    fn moving_an_area_carries_its_tables_but_moving_a_table_does_not() {
        let mut diagram = sample();
        diagram.tables[0].position = Point::new(60.0, 60.0);
        diagram.tables[1].position = Point::new(900.0, 900.0);
        diagram.tables[2].position = Point::new(1600.0, 1600.0);
        diagram.groups.push(DiagramGroup::new(
            String::from("Sales"),
            Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 400.0)),
        ));
        diagram.sync_group_members();

        assert_eq!(diagram.groups[0].members.len(), 1);
        assert!(diagram.groups[0].holds("public.orders"));

        diagram.move_group(0, Point::new(100.0, 0.0));
        assert_eq!(diagram.tables[0].position, Point::new(160.0, 60.0));
        assert_eq!(diagram.tables[1].position, Point::new(900.0, 900.0));
    }

    #[test]
    fn a_table_joins_an_area_when_its_centre_lands_inside_and_leaves_when_dragged_out() {
        let mut diagram = sample();
        diagram.groups.push(DiagramGroup::new(
            String::from("Sales"),
            Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 400.0)),
        ));
        diagram.tables[0].position = Point::new(2000.0, 2000.0);
        diagram.tables[1].position = Point::new(2400.0, 2400.0);
        diagram.tables[2].position = Point::new(2800.0, 2800.0);
        diagram.sync_group_members();
        assert!(!diagram.groups[0].holds("public.orders"));

        diagram.tables[0].position = Point::new(40.0, 40.0);
        diagram.sync_group_members();
        assert!(diagram.groups[0].holds("public.orders"));

        diagram.tables[0].position = Point::new(2000.0, 2000.0);
        diagram.sync_group_members();
        assert!(!diagram.groups[0].holds("public.orders"));
    }

    #[test]
    fn resizing_an_area_leaves_its_tables_where_they_are() {
        let mut diagram = sample();
        diagram.tables[0].position = Point::new(40.0, 40.0);
        diagram.tables[1].position = Point::new(2400.0, 2400.0);
        diagram.tables[2].position = Point::new(2800.0, 2800.0);
        diagram.groups.push(DiagramGroup::new(
            String::from("Sales"),
            Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 400.0)),
        ));
        diagram.sync_group_members();

        diagram.resize_group(0, Point::new(700.0, 650.0));

        assert_eq!(diagram.groups[0].bounds.width, 700.0);
        assert_eq!(diagram.groups[0].bounds.height, 650.0);
        assert_eq!(diagram.tables[0].position, Point::new(40.0, 40.0));
    }

    #[test]
    fn a_collapsed_area_hides_its_tables_and_their_relations() {
        let mut state = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        state.diagram.tables[0].position = Point::new(40.0, 40.0);
        state.diagram.tables[1].position = Point::new(2400.0, 2400.0);
        state.diagram.tables[2].position = Point::new(2800.0, 2800.0);
        state.diagram.groups.push(DiagramGroup::new(
            String::from("Sales"),
            Rectangle::new(Point::new(0.0, 0.0), Size::new(400.0, 400.0)),
        ));
        state.diagram.sync_group_members();
        state.refresh_derived();
        assert!(!state.table_is_hidden(0));
        assert!(state.edge_is_visible(0));

        state.diagram.groups[0].collapsed = true;
        state.refresh_derived();
        assert!(state.table_is_hidden(0));
        assert!(!state.edge_is_visible(0));
    }

    #[test]
    fn auto_layout_puts_a_child_to_the_right_of_what_it_references() {
        let mut diagram = sample();
        diagram.auto_layout();

        let orders = diagram.tables[0].position;
        let customers = diagram.tables[1].position;
        assert!(
            customers.x < orders.x,
            "customers is referenced by orders, so it belongs in an earlier column"
        );
    }

    #[test]
    fn smart_layout_packs_clusters_instead_of_one_long_chain() {
        let mut diagram = sample();
        diagram.smart_layout();

        let related = diagram.tables[0]
            .bounds()
            .union(&diagram.tables[1].bounds());
        let loose = diagram.tables[2].bounds();
        assert!(
            loose.x >= related.x + related.width || loose.y >= related.y + related.height,
            "a table with no relations belongs in its own cluster"
        );
    }

    #[test]
    fn an_area_owns_exactly_the_tables_it_was_given() {
        let mut diagram = sample();
        diagram.plan_group(String::from("Billing"), &[0, 1]);
        diagram.smart_layout();

        let group = &diagram.groups[0];
        assert_eq!(group.members.len(), 2);
        assert!(!group.members.iter().any(|name| name.contains("audit")));
        for index in [0, 1] {
            assert!(
                group
                    .bounds
                    .contains(diagram.tables[index].bounds().center())
            );
        }
    }

    #[test]
    fn a_note_keeps_the_table_it_was_written_about() {
        let mut diagram = sample();
        diagram.notes.push(DiagramNote::new(
            String::from("denormalised on purpose"),
            Point::new(10.0, 10.0),
            Some(String::from("public.orders")),
        ));

        let stored = StoredDiagram::capture(
            "New Diagram #1",
            &DiagramState {
                diagram: diagram.clone(),
                ..DiagramState::default()
            },
        );
        let mut restored = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        stored.restore(&mut restored);

        assert_eq!(
            restored.diagram.notes[0].anchor.as_deref(),
            Some("public.orders")
        );
        assert!(
            restored
                .diagram
                .table_index(restored.diagram.notes[0].anchor.as_ref().unwrap())
                .is_some()
        );
    }

    #[test]
    fn a_new_note_lands_on_free_canvas() {
        let mut diagram = sample();
        diagram.auto_layout();
        let size = Size::new(DIAGRAM_NOTE_WIDTH, DIAGRAM_NOTE_HEIGHT);
        let occupied = diagram.tables[1].position;

        let origin = diagram.free_note_origin(occupied, size);
        let placed = Rectangle::new(origin, size);

        assert!(
            !diagram
                .tables
                .iter()
                .any(|table| table.bounds().intersects(&placed))
        );

        diagram.notes.push(DiagramNote::new(
            String::from("first"),
            origin,
            Some(diagram.tables[1].name.clone()),
        ));
        let second = diagram.free_note_origin(occupied, size);
        assert_ne!(second, origin, "a second note cannot sit on the first");
    }

    #[test]
    fn notes_wrap_their_text_and_grow_with_it() {
        let short = DiagramNote::new(String::from("check"), Point::ORIGIN, None);
        let long = DiagramNote::new(
            String::from("this note is long enough that it has to wrap over several lines to fit"),
            Point::ORIGIN,
            None,
        );

        assert_eq!(short.lines().len(), 1);
        assert!(long.lines().len() > 2);
        assert!(long.bounds().height > short.bounds().height);

        let mut resized = short;
        resized.width = 360.0;
        resized.height = 200.0;
        assert_eq!(resized.bounds().size(), Size::new(360.0, 200.0));
        assert!(long.lines().iter().all(|line| line.chars().count() <= 30));
    }

    #[test]
    fn snapping_rounds_to_the_grid_only_when_enabled() {
        let mut state = DiagramState::default();
        assert_eq!(
            state.snapped(Point::new(13.0, 27.0)),
            Point::new(13.0, 27.0)
        );
        state.snap = true;
        assert_eq!(
            state.snapped(Point::new(13.0, 27.0)),
            Point::new(20.0, 20.0)
        );
    }

    #[test]
    fn search_matches_table_and_column_names() {
        let mut state = DiagramState {
            diagram: sample(),
            ..DiagramState::default()
        };
        state.search = String::from("customer");
        state.refresh_derived();
        assert_eq!(state.matches, vec![0, 1]);
        state.search = String::from("audit");
        state.match_index = 4;
        state.refresh_derived();
        assert_eq!(state.matches, vec![2]);
        assert_eq!(state.match_index, 0);
    }
}
