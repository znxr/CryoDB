use super::State;
use crate::model::connection::DatabaseDriver;
use crate::model::table::PostgresObjectKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SidebarFilterKind {
    Tables,
    Views,
    MaterializedViews,
    ForeignTables,
    Sequences,
    Extensions,
    LoginGroupRoles,
    Relationships,
    Triggers,
}

impl SidebarFilterKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SidebarFilterKind::Tables => "Tables",
            SidebarFilterKind::Views => "Views",
            SidebarFilterKind::MaterializedViews => "Materialized Views",
            SidebarFilterKind::ForeignTables => "Foreign Tables",
            SidebarFilterKind::Sequences => "Sequences",
            SidebarFilterKind::Extensions => "Extensions",
            SidebarFilterKind::LoginGroupRoles => "Login / Group Roles",
            SidebarFilterKind::Relationships => "Relationships",
            SidebarFilterKind::Triggers => "Triggers",
        }
    }

    pub(crate) fn visible_for_driver(self, driver: DatabaseDriver) -> bool {
        if driver == DatabaseDriver::PostgreSql {
            return true;
        }

        matches!(
            self,
            SidebarFilterKind::Tables
                | SidebarFilterKind::Relationships
                | SidebarFilterKind::Triggers
        )
    }
}

impl State {
    pub(crate) fn sidebar_filter_kinds_for_driver(
        driver: DatabaseDriver,
    ) -> &'static [SidebarFilterKind] {
        const POSTGRES_FILTERS: [SidebarFilterKind; 9] = [
            SidebarFilterKind::Tables,
            SidebarFilterKind::Views,
            SidebarFilterKind::MaterializedViews,
            SidebarFilterKind::ForeignTables,
            SidebarFilterKind::Sequences,
            SidebarFilterKind::Extensions,
            SidebarFilterKind::LoginGroupRoles,
            SidebarFilterKind::Relationships,
            SidebarFilterKind::Triggers,
        ];
        const DEFAULT_FILTERS: [SidebarFilterKind; 3] = [
            SidebarFilterKind::Tables,
            SidebarFilterKind::Relationships,
            SidebarFilterKind::Triggers,
        ];

        if driver == DatabaseDriver::PostgreSql {
            &POSTGRES_FILTERS
        } else {
            &DEFAULT_FILTERS
        }
    }
    pub(crate) fn sidebar_object_filter_kinds_for_driver(
        driver: DatabaseDriver,
    ) -> &'static [SidebarFilterKind] {
        const POSTGRES_OBJECT_FILTERS: [SidebarFilterKind; 7] = [
            SidebarFilterKind::Tables,
            SidebarFilterKind::Views,
            SidebarFilterKind::MaterializedViews,
            SidebarFilterKind::ForeignTables,
            SidebarFilterKind::Sequences,
            SidebarFilterKind::Extensions,
            SidebarFilterKind::LoginGroupRoles,
        ];
        const DEFAULT_OBJECT_FILTERS: [SidebarFilterKind; 1] = [SidebarFilterKind::Tables];

        if driver == DatabaseDriver::PostgreSql {
            &POSTGRES_OBJECT_FILTERS
        } else {
            &DEFAULT_OBJECT_FILTERS
        }
    }
    pub(crate) fn sidebar_filter_enabled(&self, filter: SidebarFilterKind) -> bool {
        match filter {
            SidebarFilterKind::Tables => self.sidebar_filter_tables,
            SidebarFilterKind::Views => self.sidebar_filter_views,
            SidebarFilterKind::MaterializedViews => self.sidebar_filter_materialized_views,
            SidebarFilterKind::ForeignTables => self.sidebar_filter_foreign_tables,
            SidebarFilterKind::Sequences => self.sidebar_filter_sequences,
            SidebarFilterKind::Extensions => self.sidebar_filter_extensions,
            SidebarFilterKind::LoginGroupRoles => self.sidebar_filter_login_group_roles,
            SidebarFilterKind::Relationships => self.sidebar_filter_relationships,
            SidebarFilterKind::Triggers => self.sidebar_filter_triggers,
        }
    }
    pub(crate) fn set_sidebar_filter_enabled(&mut self, filter: SidebarFilterKind, enabled: bool) {
        match filter {
            SidebarFilterKind::Tables => self.sidebar_filter_tables = enabled,
            SidebarFilterKind::Views => self.sidebar_filter_views = enabled,
            SidebarFilterKind::MaterializedViews => {
                self.sidebar_filter_materialized_views = enabled
            }
            SidebarFilterKind::ForeignTables => self.sidebar_filter_foreign_tables = enabled,
            SidebarFilterKind::Sequences => self.sidebar_filter_sequences = enabled,
            SidebarFilterKind::Extensions => self.sidebar_filter_extensions = enabled,
            SidebarFilterKind::LoginGroupRoles => self.sidebar_filter_login_group_roles = enabled,
            SidebarFilterKind::Relationships => self.sidebar_filter_relationships = enabled,
            SidebarFilterKind::Triggers => self.sidebar_filter_triggers = enabled,
        }
    }
    pub(crate) fn sidebar_filter_for_postgres_kind(kind: PostgresObjectKind) -> SidebarFilterKind {
        match kind {
            PostgresObjectKind::Table => SidebarFilterKind::Tables,
            PostgresObjectKind::View => SidebarFilterKind::Views,
            PostgresObjectKind::MaterializedView => SidebarFilterKind::MaterializedViews,
            PostgresObjectKind::ForeignTable => SidebarFilterKind::ForeignTables,
            PostgresObjectKind::Sequence => SidebarFilterKind::Sequences,
            PostgresObjectKind::Extension => SidebarFilterKind::Extensions,
            PostgresObjectKind::Role => SidebarFilterKind::LoginGroupRoles,
        }
    }
    pub(crate) fn sidebar_postgres_kind_visible(&self, kind: PostgresObjectKind) -> bool {
        self.sidebar_filter_enabled(Self::sidebar_filter_for_postgres_kind(kind))
    }
}
