use crate::model::add_column::AddColumnDraft;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(crate) struct QueryPrefix {
    pub(crate) line: usize,
    pub(crate) start_col: usize,
    pub(crate) end_col: usize,
    pub(crate) prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColumnKind {
    Text,
    Binary,
    Integer,
    Unsigned,
    Float,
    Decimal,
    Bool,
    DateTime,
    Date,
    Time,
    Unknown,
}

impl ColumnKind {
    pub(crate) fn is_editable(self) -> bool {
        !matches!(self, ColumnKind::Binary)
    }

    pub(crate) fn display_label(self) -> &'static str {
        match self {
            ColumnKind::Text => "TEXT",
            ColumnKind::Binary => "BINARY",
            ColumnKind::Integer => "INTEGER",
            ColumnKind::Unsigned => "UNSIGNED",
            ColumnKind::Float => "FLOAT",
            ColumnKind::Decimal => "DECIMAL",
            ColumnKind::Bool => "BOOL",
            ColumnKind::DateTime => "DATETIME",
            ColumnKind::Date => "DATE",
            ColumnKind::Time => "TIME",
            ColumnKind::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ColumnSpec {
    pub(crate) name: String,
    pub(crate) kind: ColumnKind,
}

#[derive(Debug, Clone)]
pub(crate) struct ResultSet {
    pub(crate) columns: Vec<String>,
    pub(crate) column_kinds: Vec<ColumnKind>,
    pub(crate) column_nullable: Vec<bool>,
    pub(crate) rows: Vec<Vec<String>>,
}

impl ResultSet {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        let mut total = 0usize;

        total += self.columns.capacity() * std::mem::size_of::<String>();
        total += self.columns.iter().map(String::capacity).sum::<usize>();

        total += self.column_kinds.capacity() * std::mem::size_of::<ColumnKind>();
        total += self.column_nullable.capacity() * std::mem::size_of::<bool>();

        total += self.rows.capacity() * std::mem::size_of::<Vec<String>>();
        for row in &self.rows {
            total += row.capacity() * std::mem::size_of::<String>();
            total += row.iter().map(String::capacity).sum::<usize>();
        }

        total
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TableCacheEntry {
    pub(crate) results: Arc<ResultSet>,
    pub(crate) has_next_page: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TriggerInfo {
    pub(crate) name: String,
    pub(crate) timing: String,
    pub(crate) event: String,
    pub(crate) statement: String,
    pub(crate) definer: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RelationInfo {
    pub(crate) column: String,
    pub(crate) referenced_table: String,
    pub(crate) referenced_column: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SidebarTriggerEntry {
    pub(crate) table: String,
    pub(crate) trigger: TriggerInfo,
}

#[derive(Debug, Clone)]
pub(crate) struct SidebarRelationEntry {
    pub(crate) table: String,
    pub(crate) relation: RelationInfo,
    pub(crate) constraint_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PostgresObjectKind {
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Sequence,
    Extension,
    Role,
}

impl PostgresObjectKind {
    pub(crate) fn section_label(self) -> &'static str {
        match self {
            PostgresObjectKind::Table => "Tables",
            PostgresObjectKind::View => "Views",
            PostgresObjectKind::MaterializedView => "Materialized Views",
            PostgresObjectKind::ForeignTable => "Foreign Tables",
            PostgresObjectKind::Sequence => "Sequences",
            PostgresObjectKind::Extension => "Extensions",
            PostgresObjectKind::Role => "Login / Group Roles",
        }
    }

    pub(crate) fn searchable_label(self) -> &'static str {
        match self {
            PostgresObjectKind::Table => "table",
            PostgresObjectKind::View => "view",
            PostgresObjectKind::MaterializedView => "materialized view",
            PostgresObjectKind::ForeignTable => "foreign table",
            PostgresObjectKind::Sequence => "sequence",
            PostgresObjectKind::Extension => "extension",
            PostgresObjectKind::Role => "login group role",
        }
    }

    pub(crate) fn is_selectable_table(self) -> bool {
        matches!(
            self,
            PostgresObjectKind::Table
                | PostgresObjectKind::View
                | PostgresObjectKind::MaterializedView
                | PostgresObjectKind::ForeignTable
        )
    }

    pub(crate) const fn ordered() -> [Self; 7] {
        [
            PostgresObjectKind::Table,
            PostgresObjectKind::View,
            PostgresObjectKind::MaterializedView,
            PostgresObjectKind::ForeignTable,
            PostgresObjectKind::Sequence,
            PostgresObjectKind::Extension,
            PostgresObjectKind::Role,
        ]
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PostgresSidebarObject {
    pub(crate) schema: String,
    pub(crate) name: String,
    pub(crate) kind: PostgresObjectKind,
}

impl PostgresSidebarObject {
    pub(crate) fn qualified_name(&self) -> String {
        if self.schema.trim().is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.schema, self.name)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TableRelationFilter {
    pub(crate) column: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ColumnResize {
    pub(crate) column: usize,
    pub(crate) start_x: Option<f32>,
    pub(crate) start_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostgresRoleModalMode {
    Create,
    Properties,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostgresRoleModalTab {
    General,
    Definition,
    Privileges,
    Membership,
    Parameters,
    Security,
    Sql,
}

impl PostgresRoleModalTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PostgresRoleModalTab::General => "General",
            PostgresRoleModalTab::Definition => "Definition",
            PostgresRoleModalTab::Privileges => "Privileges",
            PostgresRoleModalTab::Membership => "Membership",
            PostgresRoleModalTab::Parameters => "Parameters",
            PostgresRoleModalTab::Security => "Security",
            PostgresRoleModalTab::Sql => "SQL",
        }
    }

    pub(crate) const fn ordered() -> [Self; 7] {
        [
            PostgresRoleModalTab::General,
            PostgresRoleModalTab::Definition,
            PostgresRoleModalTab::Privileges,
            PostgresRoleModalTab::Membership,
            PostgresRoleModalTab::Parameters,
            PostgresRoleModalTab::Security,
            PostgresRoleModalTab::Sql,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostgresRoleTextField {
    RoleName,
    ConnectionLimit,
    Password,
    ValidUntil,
    MemberOfRoles,
    Members,
    SearchPath,
    WorkMem,
    MaintenanceWorkMem,
    StatementTimeout,
    LockTimeout,
    IdleInTransactionSessionTimeout,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostgresRoleToggleField {
    CanLogin,
    IsSuperuser,
    InheritPrivileges,
    CanCreateDb,
    CanCreateRole,
    CanReplicate,
    BypassRls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostgresRoleForm {
    pub(crate) role_name: String,
    pub(crate) can_login: bool,
    pub(crate) is_superuser: bool,
    pub(crate) inherit_privileges: bool,
    pub(crate) can_create_db: bool,
    pub(crate) can_create_role: bool,
    pub(crate) can_replicate: bool,
    pub(crate) bypass_rls: bool,
    pub(crate) connection_limit: String,
    pub(crate) password: String,
    pub(crate) valid_until: String,
    pub(crate) member_of_roles: String,
    pub(crate) members: String,
    pub(crate) search_path: String,
    pub(crate) work_mem: String,
    pub(crate) maintenance_work_mem: String,
    pub(crate) statement_timeout: String,
    pub(crate) lock_timeout: String,
    pub(crate) idle_in_transaction_session_timeout: String,
    pub(crate) comment: String,
}

impl Default for PostgresRoleForm {
    fn default() -> Self {
        Self {
            role_name: String::new(),
            can_login: false,
            is_superuser: false,
            inherit_privileges: true,
            can_create_db: false,
            can_create_role: false,
            can_replicate: false,
            bypass_rls: false,
            connection_limit: String::from("-1"),
            password: String::new(),
            valid_until: String::new(),
            member_of_roles: String::new(),
            members: String::new(),
            search_path: String::new(),
            work_mem: String::new(),
            maintenance_work_mem: String::new(),
            statement_timeout: String::new(),
            lock_timeout: String::new(),
            idle_in_transaction_session_timeout: String::new(),
            comment: String::new(),
        }
    }
}

impl PostgresRoleForm {
    pub(crate) fn with_role_name(role_name: impl Into<String>) -> Self {
        Self {
            role_name: role_name.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableFolder {
    pub(crate) name: String,
    pub(crate) is_open: bool,
    pub(crate) pin_to_top: bool,
    pub(crate) auto_generated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FolderStore {
    pub(crate) connections: HashMap<String, ConnectionFolderStore>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ConnectionFolderStore {
    pub(crate) databases: HashMap<String, FolderProfileState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FolderProfileState {
    pub(crate) folders: Vec<StoredFolderEntry>,
    pub(crate) table_folder_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredFolderEntry {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) is_open: bool,
    #[serde(default)]
    pub(crate) pin_to_top: bool,
    #[serde(default, alias = "smart_group")]
    pub(crate) auto_generated: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum TableModalState {
    MakeFolder {
        name: String,
        open_by_default: bool,
        pin_to_top: bool,
        source_folder: Option<String>,
    },
    AlterTable {
        table: String,
        clause: String,
    },
    AddColumn {
        table: String,
        draft: AddColumnDraft,
    },
    DuplicateTable {
        table: String,
        new_name: String,
    },
    RenameTable {
        table: String,
        new_name: String,
    },
    RenameQueryTab {
        index: usize,
        name: String,
    },
    MoveToFolder {
        table: String,
        folder: String,
    },
    ConfirmFoldersImport {
        path: PathBuf,
        groups: Vec<GeneratedFolder>,
        existing_groups: usize,
        existing_tables: usize,
    },
    Confirm {
        title: String,
        summary: String,
        command: TableCommand,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum TableCommand {
    AlterTable { table: String, clause: String },
    AddColumn { table: String, definition: String },
    Truncate { table: String },
    Drop { table: String },
    Duplicate { table: String, new_table: String },
    Rename { table: String, new_table: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedFolder {
    #[serde(default)]
    pub(crate) folder: String,
    #[serde(default)]
    pub(crate) tables: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FolderGroupsToml {
    pub(crate) version: u32,
    pub(crate) groups: Vec<GeneratedFolder>,
}

impl Default for FolderGroupsToml {
    fn default() -> Self {
        Self {
            version: 1,
            groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub(crate) fn toggled(self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }

    pub(crate) fn as_sql(self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TableSortState {
    pub(crate) column: String,
    pub(crate) direction: SortDirection,
}

#[derive(Debug, Clone)]
pub(crate) struct TableInfoDetails {
    pub(crate) engine: Option<String>,
    pub(crate) rows: Option<u64>,
    pub(crate) data_size: Option<u64>,
    pub(crate) index_size: Option<u64>,
    pub(crate) free_size: Option<u64>,
    pub(crate) avg_row_length: Option<u64>,
    pub(crate) auto_increment: Option<u64>,
    pub(crate) row_format: Option<String>,
    pub(crate) collation: Option<String>,
    pub(crate) charset: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) checked_at: Option<String>,
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum QueryOutput {
    Rows(ResultSet),
    Affected(u64),
}
