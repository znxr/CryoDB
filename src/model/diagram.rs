use crate::model::table::SidebarRelationEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct DiagramColumn {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) primary: bool,
    pub(crate) foreign: bool,
    pub(crate) nullable: bool,
    pub(crate) default: Option<String>,
    pub(crate) indexed: bool,
    pub(crate) unique: bool,
    pub(crate) attributes: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaDiagramData {
    pub(crate) tables: Vec<SchemaDiagramTable>,
    pub(crate) edges: Vec<SchemaDiagramEdge>,
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaDiagramTable {
    pub(crate) name: String,
    pub(crate) columns: Vec<DiagramColumn>,
}

#[derive(Debug, Clone)]
pub(crate) struct SchemaDiagramEdge {
    pub(crate) from: usize,
    pub(crate) from_column: usize,
    pub(crate) to: usize,
    pub(crate) to_column: usize,
    pub(crate) constraint_name: String,
    pub(crate) one_to_one: bool,
}

impl SchemaDiagramData {
    pub(crate) fn build(
        tables: Vec<(String, Vec<DiagramColumn>)>,
        relations: Vec<SidebarRelationEntry>,
    ) -> Self {
        let mut tables = tables
            .into_iter()
            .map(|(name, columns)| SchemaDiagramTable { name, columns })
            .collect::<Vec<_>>();
        let mut index_by_name = HashMap::new();
        for (index, table) in tables.iter().enumerate() {
            index_by_name.insert(table.name.to_ascii_lowercase(), index);
            index_by_name
                .entry(
                    table
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&table.name)
                        .to_ascii_lowercase(),
                )
                .or_insert(index);
        }
        let key = |name: &str| name.to_ascii_lowercase();
        let short = |name: &str| name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
        let mut edges = Vec::new();
        for entry in &relations {
            let Some(from) = index_by_name
                .get(&key(&entry.table))
                .or_else(|| index_by_name.get(&short(&entry.table)))
                .copied()
            else {
                continue;
            };
            let Some(to) = index_by_name
                .get(&key(&entry.relation.referenced_table))
                .or_else(|| index_by_name.get(&short(&entry.relation.referenced_table)))
                .copied()
            else {
                continue;
            };
            let Some(from_column) = tables[from]
                .columns
                .iter()
                .position(|column| key(&column.name) == key(&entry.relation.column))
            else {
                continue;
            };
            let to_column = tables[to]
                .columns
                .iter()
                .position(|column| key(&column.name) == key(&entry.relation.referenced_column))
                .unwrap_or(0);
            tables[from].columns[from_column].foreign = true;
            edges.push(SchemaDiagramEdge {
                from,
                from_column,
                to,
                to_column,
                constraint_name: entry.constraint_name.clone(),
                one_to_one: tables[from].columns[from_column].unique,
            });
        }
        Self { tables, edges }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DiagramRelations {
    #[default]
    Auto,
    All,
    Selected,
}

impl DiagramRelations {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Relations: automatic",
            Self::All => "Relations: always shown",
            Self::Selected => "Relations: selection only",
        }
    }
}

impl std::fmt::Display for DiagramRelations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::tr(self.label()))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DiagramStore {
    pub(crate) connections: HashMap<String, ConnectionDiagramStore>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConnectionDiagramStore {
    pub(crate) databases: HashMap<String, DatabaseDiagramStore>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DatabaseDiagramStore {
    pub(crate) diagrams: Vec<StoredDiagram>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredDiagram {
    pub(crate) name: String,
    pub(crate) open: bool,
    pub(crate) offset_x: f32,
    pub(crate) offset_y: f32,
    pub(crate) zoom: f32,
    pub(crate) snap: bool,
    pub(crate) fullscreen: bool,
    pub(crate) relations: DiagramRelations,
    pub(crate) tables: Vec<StoredDiagramTable>,
    pub(crate) groups: Vec<StoredDiagramGroup>,
    pub(crate) notes: Vec<StoredDiagramNote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredDiagramTable {
    pub(crate) name: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) collapsed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredDiagramGroup {
    pub(crate) name: String,
    pub(crate) color: u8,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) locked: bool,
    pub(crate) contents_locked: bool,
    pub(crate) collapsed: bool,
    pub(crate) members: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredDiagramNote {
    pub(crate) text: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) anchor: Option<String>,
}
