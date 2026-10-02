use super::model::{SchemaChange, foreign_key_name};
use crate::model::connection::DatabaseDriver;
use crate::utils::helpers::{sql_quote_identifier, sql_quote_table_reference};
pub(crate) fn diagram_change_sql(change: &SchemaChange, driver: DatabaseDriver) -> String {
    let quote = |name: &str| sql_quote_table_reference(driver, name);
    let column_quote = |name: &str| sql_quote_identifier(driver, name);
    match change {
        SchemaChange::CreateTable { table, columns } => {
            let body = columns
                .iter()
                .map(|(name, data_type, primary)| {
                    format!(
                        "  {} {}{}",
                        column_quote(name),
                        data_type,
                        if *primary { " PRIMARY KEY" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!("CREATE TABLE {} (\n{}\n);", quote(table), body)
        }
        SchemaChange::DropTable { table } => format!("DROP TABLE {};", quote(table)),
        SchemaChange::RenameTable { table, new_table } => {
            let new_table = if driver == DatabaseDriver::PostgreSql {
                new_table
                    .rsplit_once('.')
                    .map_or(new_table.as_str(), |(_, name)| name)
            } else {
                new_table
            };
            format!(
                "ALTER TABLE {} RENAME TO {};",
                quote(table),
                column_quote(new_table.trim_matches('"'))
            )
        }
        SchemaChange::AddColumn {
            table,
            column,
            data_type,
        } => format!(
            "ALTER TABLE {} ADD COLUMN {} {};",
            quote(table),
            column_quote(column),
            data_type
        ),
        SchemaChange::DropColumn { table, column } => format!(
            "ALTER TABLE {} DROP COLUMN {};",
            quote(table),
            column_quote(column)
        ),
        SchemaChange::AlterColumn {
            table,
            column,
            data_type,
            nullable,
            default,
            attributes,
        } => {
            let null = if *nullable { "NULL" } else { "NOT NULL" };
            match driver {
                DatabaseDriver::Sqlite => format!(
                    "-- SQLite will recreate {} to alter {} to {} {}{}.",
                    quote(table),
                    column_quote(column),
                    data_type,
                    null,
                    match default {
                        Some(value) => format!(" DEFAULT {value}"),
                        None => String::new(),
                    }
                ),
                DatabaseDriver::PostgreSql => {
                    let mut sql = format!(
                        "ALTER TABLE {} ALTER COLUMN {} TYPE {};\nALTER TABLE {} ALTER COLUMN {} \
                         {} NOT NULL;",
                        quote(table),
                        column_quote(column),
                        data_type,
                        quote(table),
                        column_quote(column),
                        if *nullable { "DROP" } else { "SET" }
                    );
                    sql.push_str(&match default {
                        Some(value) => format!(
                            "\nALTER TABLE {} ALTER COLUMN {} SET DEFAULT {};",
                            quote(table),
                            column_quote(column),
                            value
                        ),
                        None => format!(
                            "\nALTER TABLE {} ALTER COLUMN {} DROP DEFAULT;",
                            quote(table),
                            column_quote(column)
                        ),
                    });
                    sql
                }
                _ => format!(
                    "ALTER TABLE {} MODIFY COLUMN {} {} {}{}{};",
                    quote(table),
                    column_quote(column),
                    data_type,
                    null,
                    match default {
                        Some(value) => format!(" DEFAULT {value}"),
                        None => String::new(),
                    },
                    if attributes.is_empty() {
                        String::new()
                    } else {
                        format!(" {attributes}")
                    }
                ),
            }
        }
        SchemaChange::AddForeignKey {
            table,
            columns,
            referenced_table,
            referenced_columns,
        } => {
            let list = |names: &[String]| {
                names
                    .iter()
                    .map(|name| column_quote(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            if driver == DatabaseDriver::Sqlite {
                return format!(
                    "-- SQLite will recreate {} to add: FOREIGN KEY ({}) REFERENCES {} ({}).",
                    quote(table),
                    list(columns),
                    quote(referenced_table),
                    list(referenced_columns)
                );
            }
            format!(
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});",
                quote(table),
                column_quote(&foreign_key_name(table, columns)),
                list(columns),
                quote(referenced_table),
                list(referenced_columns)
            )
        }
        SchemaChange::DropForeignKey {
            table,
            column,
            referenced_table,
            constraint_name,
            ..
        } => {
            if driver == DatabaseDriver::Sqlite {
                return format!(
                    "-- SQLite will recreate {} without the foreign key to {}.",
                    quote(table),
                    quote(referenced_table)
                );
            }
            let constraint = if constraint_name.is_empty() {
                format!(
                    "fk_{}_{}",
                    table.rsplit('.').next().unwrap_or(table),
                    column
                )
            } else {
                constraint_name.clone()
            };
            let keyword = if matches!(driver, DatabaseDriver::MySql | DatabaseDriver::MariaDb) {
                "DROP FOREIGN KEY"
            } else {
                "DROP CONSTRAINT"
            };
            format!(
                "ALTER TABLE {} {} {};",
                quote(table),
                keyword,
                column_quote(&constraint)
            )
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_table_sql_matches_each_driver() {
        let change = |table: &str, new_table: &str| SchemaChange::RenameTable {
            table: table.to_string(),
            new_table: new_table.to_string(),
        };

        assert_eq!(
            diagram_change_sql(&change("users", "members"), DatabaseDriver::MySql),
            "ALTER TABLE `users` RENAME TO `members`;"
        );
        assert_eq!(
            diagram_change_sql(&change("users", "members"), DatabaseDriver::MariaDb),
            "ALTER TABLE `users` RENAME TO `members`;"
        );
        assert_eq!(
            diagram_change_sql(&change("users", "members"), DatabaseDriver::Sqlite),
            "ALTER TABLE \"users\" RENAME TO \"members\";"
        );
        assert_eq!(
            diagram_change_sql(
                &change("public.users", "public.members"),
                DatabaseDriver::PostgreSql
            ),
            "ALTER TABLE \"public\".\"users\" RENAME TO \"members\";"
        );
    }
    #[test]
    fn drop_foreign_key_sql_matches_each_driver() {
        let change = SchemaChange::DropForeignKey {
            table: String::from("orders"),
            column: String::from("customer_id"),
            referenced_table: String::from("customers"),
            referenced_column: String::from("id"),
            constraint_name: String::from("orders_customer_fk"),
        };

        assert!(
            diagram_change_sql(&change, DatabaseDriver::MySql)
                .contains("DROP FOREIGN KEY `orders_customer_fk`")
        );
        assert!(
            diagram_change_sql(&change, DatabaseDriver::MariaDb)
                .contains("DROP FOREIGN KEY `orders_customer_fk`")
        );
        assert!(
            diagram_change_sql(&change, DatabaseDriver::PostgreSql)
                .contains("DROP CONSTRAINT \"orders_customer_fk\"")
        );
        assert!(
            diagram_change_sql(&change, DatabaseDriver::Sqlite)
                .contains("will recreate \"orders\"")
        );
    }
    #[test]
    fn alter_column_sql_matches_each_driver() {
        let change = SchemaChange::AlterColumn {
            table: String::from("orders"),
            column: String::from("note"),
            data_type: String::from("VARCHAR(120)"),
            nullable: false,
            default: Some(String::from("'none'")),
            attributes: String::new(),
        };

        assert_eq!(
            diagram_change_sql(&change, DatabaseDriver::MySql),
            "ALTER TABLE `orders` MODIFY COLUMN `note` VARCHAR(120) NOT NULL DEFAULT 'none';"
        );
        assert_eq!(
            diagram_change_sql(&change, DatabaseDriver::MariaDb),
            "ALTER TABLE `orders` MODIFY COLUMN `note` VARCHAR(120) NOT NULL DEFAULT 'none';"
        );
        assert_eq!(
            diagram_change_sql(&change, DatabaseDriver::PostgreSql),
            "ALTER TABLE \"orders\" ALTER COLUMN \"note\" TYPE VARCHAR(120);\n\
             ALTER TABLE \"orders\" ALTER COLUMN \"note\" SET NOT NULL;\n\
             ALTER TABLE \"orders\" ALTER COLUMN \"note\" SET DEFAULT 'none';"
        );
        assert!(
            diagram_change_sql(&change, DatabaseDriver::Sqlite)
                .contains("will recreate \"orders\" to alter \"note\"")
        );
    }
    #[test]
    fn alter_column_sql_keeps_mysql_attributes() {
        let change = SchemaChange::AlterColumn {
            table: String::from("orders"),
            column: String::from("id"),
            data_type: String::from("BIGINT"),
            nullable: false,
            default: None,
            attributes: String::from("auto_increment"),
        };

        assert_eq!(
            diagram_change_sql(&change, DatabaseDriver::MySql),
            "ALTER TABLE `orders` MODIFY COLUMN `id` BIGINT NOT NULL auto_increment;"
        );
        assert_eq!(
            diagram_change_sql(&change, DatabaseDriver::MariaDb),
            "ALTER TABLE `orders` MODIFY COLUMN `id` BIGINT NOT NULL auto_increment;"
        );
    }
}
