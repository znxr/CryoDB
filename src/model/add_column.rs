use crate::model::connection::DatabaseDriver;
use crate::utils::helpers::{sql_quote_identifier, sql_quote_string_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ColumnDefaultKind {
    #[default]
    None,
    Null,
    Value,
    Expression,
}

impl ColumnDefaultKind {
    pub(crate) const ALL: [Self; 4] = [Self::None, Self::Null, Self::Value, Self::Expression];
}

impl std::fmt::Display for ColumnDefaultKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::tr(match self {
            ColumnDefaultKind::None => "No default",
            ColumnDefaultKind::Null => "NULL",
            ColumnDefaultKind::Value => "Literal value",
            ColumnDefaultKind::Expression => "Expression",
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ColumnPosition {
    #[default]
    Last,
    First,
    After,
}

impl ColumnPosition {
    pub(crate) const ALL: [Self; 3] = [Self::Last, Self::First, Self::After];

    pub(crate) fn supported_by(driver: DatabaseDriver) -> bool {
        matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb)
    }
}

impl std::fmt::Display for ColumnPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&crate::i18n::tr(match self {
            ColumnPosition::Last => "At the end",
            ColumnPosition::First => "First column",
            ColumnPosition::After => "After a column",
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddColumnField {
    Name,
    DataType,
    Length,
    DefaultValue,
    Comment,
    AfterColumn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddColumnFlag {
    Nullable,
    Unique,
    AutoIncrement,
}

#[derive(Debug, Clone)]
pub(crate) struct AddColumnDraft {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) length: String,
    pub(crate) nullable: bool,
    pub(crate) default_kind: ColumnDefaultKind,
    pub(crate) default_value: String,
    pub(crate) unique: bool,
    pub(crate) auto_increment: bool,
    pub(crate) comment: String,
    pub(crate) position: ColumnPosition,
    pub(crate) after_column: String,
}

impl AddColumnDraft {
    pub(crate) fn new(driver: DatabaseDriver) -> Self {
        Self {
            name: String::new(),
            data_type: String::from(default_data_type(driver)),
            length: String::new(),
            nullable: true,
            default_kind: ColumnDefaultKind::None,
            default_value: String::new(),
            unique: false,
            auto_increment: false,
            comment: String::new(),
            position: ColumnPosition::Last,
            after_column: String::new(),
        }
    }

    pub(crate) fn set_field(&mut self, field: AddColumnField, value: String) {
        match field {
            AddColumnField::Name => self.name = value,
            AddColumnField::DataType => self.data_type = value,
            AddColumnField::Length => self.length = value,
            AddColumnField::DefaultValue => self.default_value = value,
            AddColumnField::Comment => self.comment = value,
            AddColumnField::AfterColumn => self.after_column = value,
        }
    }

    pub(crate) fn set_flag(&mut self, flag: AddColumnFlag, enabled: bool) {
        match flag {
            AddColumnFlag::Nullable => self.nullable = enabled,
            AddColumnFlag::Unique => self.unique = enabled,
            AddColumnFlag::AutoIncrement => self.auto_increment = enabled,
        }
    }

    pub(crate) fn rendered_type(&self) -> String {
        let data_type = self.data_type.trim();
        let length = self.length.trim();
        if length.is_empty() || !type_takes_length(data_type) || data_type.contains('(') {
            return data_type.to_string();
        }
        format!("{data_type}({length})")
    }

    pub(crate) fn definition(&self, driver: DatabaseDriver) -> Result<String, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(String::from("Give the column a name."));
        }
        let data_type = self.rendered_type();
        if data_type.is_empty() {
            return Err(String::from("Choose a data type for the column."));
        }
        if type_takes_length(self.data_type.trim())
            && requires_length(self.data_type.trim())
            && self.length.trim().is_empty()
            && !self.data_type.contains('(')
        {
            return Err(format!(
                "{} needs a length, for example 255.",
                self.data_type.trim()
            ));
        }
        if self.unique && driver == DatabaseDriver::Sqlite {
            return Err(String::from(
                "SQLite cannot add a UNIQUE column — add the column first, then create a unique index.",
            ));
        }
        if self.auto_increment && !matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb)
        {
            return Err(String::from(
                "AUTO_INCREMENT is MySQL/MariaDB only — on PostgreSQL use a serial or identity type.",
            ));
        }
        if !self.comment.trim().is_empty()
            && !matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb)
        {
            return Err(String::from(
                "Inline column comments are MySQL/MariaDB only — elsewhere run COMMENT ON COLUMN separately.",
            ));
        }
        if self.position != ColumnPosition::Last && !ColumnPosition::supported_by(driver) {
            return Err(String::from(
                "Only MySQL and MariaDB can place a new column — PostgreSQL and SQLite always append.",
            ));
        }
        if self.position == ColumnPosition::After && self.after_column.trim().is_empty() {
            return Err(String::from("Pick the column to insert after."));
        }
        if !self.nullable
            && self.default_kind == ColumnDefaultKind::None
            && driver == DatabaseDriver::Sqlite
        {
            return Err(String::from(
                "SQLite needs a default when adding a NOT NULL column to an existing table.",
            ));
        }
        if matches!(
            self.default_kind,
            ColumnDefaultKind::Value | ColumnDefaultKind::Expression
        ) && self.default_value.trim().is_empty()
        {
            return Err(String::from(
                "Enter the default, or switch to \"No default\".",
            ));
        }
        if !self.nullable && self.default_kind == ColumnDefaultKind::Null {
            return Err(String::from("A NOT NULL column cannot default to NULL."));
        }

        let mut sql = format!("{} {data_type}", sql_quote_identifier(driver, name));
        sql.push_str(if self.nullable { " NULL" } else { " NOT NULL" });

        match self.default_kind {
            ColumnDefaultKind::None => {}
            ColumnDefaultKind::Null => sql.push_str(" DEFAULT NULL"),
            ColumnDefaultKind::Value => {
                let value = self.default_value.trim();
                let literal = if is_bare_literal(value) {
                    value.to_string()
                } else {
                    sql_quote_string_literal(driver, value)
                };
                sql.push_str(&format!(" DEFAULT {literal}"));
            }
            ColumnDefaultKind::Expression => {
                sql.push_str(&format!(" DEFAULT {}", self.default_value.trim()));
            }
        }

        if self.auto_increment {
            sql.push_str(" AUTO_INCREMENT");
        }
        if self.unique {
            sql.push_str(" UNIQUE");
        }
        if !self.comment.trim().is_empty() {
            sql.push_str(&format!(
                " COMMENT {}",
                sql_quote_string_literal(driver, self.comment.trim())
            ));
        }

        match self.position {
            ColumnPosition::Last => {}
            ColumnPosition::First => sql.push_str(" FIRST"),
            ColumnPosition::After => sql.push_str(&format!(
                " AFTER {}",
                sql_quote_identifier(driver, self.after_column.trim())
            )),
        }

        Ok(sql)
    }
}

pub(crate) fn default_data_type(driver: DatabaseDriver) -> &'static str {
    match driver {
        DatabaseDriver::PostgreSql => "text",
        DatabaseDriver::Sqlite => "TEXT",
        _ => "VARCHAR",
    }
}

pub(crate) fn data_type_options(driver: DatabaseDriver) -> &'static [&'static str] {
    match driver {
        DatabaseDriver::PostgreSql => &[
            "text",
            "varchar",
            "char",
            "integer",
            "bigint",
            "smallint",
            "serial",
            "bigserial",
            "numeric",
            "real",
            "double precision",
            "boolean",
            "date",
            "timestamp",
            "timestamptz",
            "time",
            "uuid",
            "json",
            "jsonb",
            "bytea",
        ],
        DatabaseDriver::Sqlite => &["TEXT", "INTEGER", "REAL", "NUMERIC", "BLOB"],
        _ => &[
            "VARCHAR",
            "CHAR",
            "TEXT",
            "LONGTEXT",
            "TINYINT",
            "SMALLINT",
            "INT",
            "BIGINT",
            "DECIMAL",
            "FLOAT",
            "DOUBLE",
            "BOOLEAN",
            "DATE",
            "DATETIME",
            "TIMESTAMP",
            "TIME",
            "JSON",
            "BLOB",
        ],
    }
}

pub(crate) fn type_takes_length(data_type: &str) -> bool {
    let lowered = data_type.trim().to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "varchar"
            | "char"
            | "varbinary"
            | "binary"
            | "decimal"
            | "numeric"
            | "float"
            | "double"
            | "bit"
            | "int"
            | "integer"
            | "bigint"
            | "smallint"
            | "tinyint"
            | "time"
            | "datetime"
            | "timestamp"
            | "timestamptz"
    )
}

fn requires_length(data_type: &str) -> bool {
    let lowered = data_type.trim().to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "varchar" | "char" | "varbinary" | "binary"
    )
}

fn is_bare_literal(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        return true;
    }
    trimmed.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(driver: DatabaseDriver) -> AddColumnDraft {
        let mut draft = AddColumnDraft::new(driver);
        draft.name = String::from("nickname");
        draft.data_type = String::from("VARCHAR");
        draft.length = String::from("255");
        draft
    }

    #[test]
    fn mysql_definition_carries_length_default_and_position() {
        let mut draft = draft(DatabaseDriver::MySql);
        draft.nullable = false;
        draft.default_kind = ColumnDefaultKind::Value;
        draft.default_value = String::from("guest");
        draft.comment = String::from("display name");
        draft.position = ColumnPosition::After;
        draft.after_column = String::from("email");

        assert_eq!(
            draft.definition(DatabaseDriver::MySql).unwrap(),
            "`nickname` VARCHAR(255) NOT NULL DEFAULT 'guest' COMMENT 'display name' AFTER `email`"
        );
    }

    #[test]
    fn numeric_defaults_are_not_quoted() {
        let mut draft = draft(DatabaseDriver::MySql);
        draft.data_type = String::from("INT");
        draft.length = String::new();
        draft.default_kind = ColumnDefaultKind::Value;
        draft.default_value = String::from("0");

        assert_eq!(
            draft.definition(DatabaseDriver::MySql).unwrap(),
            "`nickname` INT NULL DEFAULT 0"
        );
    }

    #[test]
    fn expressions_pass_through_unquoted() {
        let mut draft = draft(DatabaseDriver::PostgreSql);
        draft.data_type = String::from("timestamptz");
        draft.length = String::new();
        draft.default_kind = ColumnDefaultKind::Expression;
        draft.default_value = String::from("now()");

        assert_eq!(
            draft.definition(DatabaseDriver::PostgreSql).unwrap(),
            "\"nickname\" timestamptz NULL DEFAULT now()"
        );
    }

    #[test]
    fn driver_limits_are_reported_before_the_statement_runs() {
        let mut unique = draft(DatabaseDriver::Sqlite);
        unique.data_type = String::from("TEXT");
        unique.length = String::new();
        unique.unique = true;
        assert!(
            unique
                .definition(DatabaseDriver::Sqlite)
                .unwrap_err()
                .contains("unique index")
        );

        let mut positioned = draft(DatabaseDriver::PostgreSql);
        positioned.position = ColumnPosition::First;
        assert!(
            positioned
                .definition(DatabaseDriver::PostgreSql)
                .unwrap_err()
                .contains("always append")
        );

        let mut auto = draft(DatabaseDriver::PostgreSql);
        auto.auto_increment = true;
        assert!(
            auto.definition(DatabaseDriver::PostgreSql)
                .unwrap_err()
                .contains("identity")
        );
    }

    #[test]
    fn incomplete_drafts_explain_what_is_missing() {
        let empty = AddColumnDraft::new(DatabaseDriver::MySql);
        assert_eq!(
            empty.definition(DatabaseDriver::MySql).unwrap_err(),
            "Give the column a name."
        );

        let mut no_length = AddColumnDraft::new(DatabaseDriver::MySql);
        no_length.name = String::from("nickname");
        assert!(
            no_length
                .definition(DatabaseDriver::MySql)
                .unwrap_err()
                .contains("needs a length")
        );

        let mut after = draft(DatabaseDriver::MySql);
        after.position = ColumnPosition::After;
        assert_eq!(
            after.definition(DatabaseDriver::MySql).unwrap_err(),
            "Pick the column to insert after."
        );

        let mut null_default = draft(DatabaseDriver::MySql);
        null_default.nullable = false;
        null_default.default_kind = ColumnDefaultKind::Null;
        assert!(
            null_default
                .definition(DatabaseDriver::MySql)
                .unwrap_err()
                .contains("cannot default to NULL")
        );
    }

    #[test]
    fn a_type_that_already_carries_its_length_is_left_alone() {
        let mut draft = draft(DatabaseDriver::MySql);
        draft.data_type = String::from("DECIMAL(10,2)");
        draft.length = String::from("255");
        assert_eq!(draft.rendered_type(), "DECIMAL(10,2)");
    }
}
