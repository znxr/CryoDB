use crate::ai::sql_fix::fetch_ai_sql_fix_schema_snapshot;
use crate::ai::{AiMessage, AiRequestConfig};
use crate::db::metadata::fetch_sidebar_relations;
use crate::{AI_CHAT_MAX_RELATIONS, AI_CHAT_MAX_TABLE_NAMES, AI_CHAT_SYSTEM_PROMPT, DatabasePool};

pub(crate) struct ChatRequest {
    pub(crate) config: AiRequestConfig,
    pub(crate) pool: Option<DatabasePool>,
    pub(crate) database: Option<String>,
    pub(crate) driver: &'static str,
    pub(crate) tables: Vec<String>,
    pub(crate) detail_tables: Vec<String>,
    pub(crate) selected_table: Option<String>,
    pub(crate) editor_query: Option<String>,
    pub(crate) last_query: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) history: Vec<AiMessage>,
}

pub(crate) async fn chat_messages(request: &ChatRequest) -> Vec<AiMessage> {
    let mut snapshot = Vec::new();
    let mut relations = Vec::new();
    if let Some(pool) = request.pool.clone() {
        snapshot = fetch_ai_sql_fix_schema_snapshot(
            pool.clone(),
            request.database.clone(),
            &request.detail_tables,
        )
        .await
        .unwrap_or_default();

        if let Some(database) = request.database.clone() {
            let scope = request
                .detail_tables
                .iter()
                .map(|table| {
                    table
                        .rsplit('.')
                        .next()
                        .unwrap_or(table)
                        .to_ascii_lowercase()
                })
                .collect::<std::collections::HashSet<_>>();

            relations = fetch_sidebar_relations(pool, database)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|entry| {
                    let child = entry
                        .table
                        .rsplit('.')
                        .next()
                        .unwrap_or(&entry.table)
                        .to_ascii_lowercase();
                    let parent = entry
                        .relation
                        .referenced_table
                        .rsplit('.')
                        .next()
                        .unwrap_or(&entry.relation.referenced_table)
                        .to_ascii_lowercase();
                    scope.contains(&child) || scope.contains(&parent)
                })
                .collect();
            relations.sort_by(|a, b| {
                (&a.table, &a.relation.column).cmp(&(&b.table, &b.relation.column))
            });
        }
    }

    let mut system = format!("{AI_CHAT_SYSTEM_PROMPT}\n\nDriver: {}.", request.driver);
    if let Some(database) = &request.database {
        system.push_str(&format!("\nDatabase: {database}."));
    }
    if let Some(table) = &request.selected_table {
        system.push_str(&format!("\nTable open in the client: {table}."));
    }

    if !request.tables.is_empty() {
        let names = request
            .tables
            .iter()
            .take(AI_CHAT_MAX_TABLE_NAMES)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        system.push_str(&format!("\n\nTables in this database: {names}"));
        if request.tables.len() > AI_CHAT_MAX_TABLE_NAMES {
            system.push_str(&format!(
                " (+{} more)",
                request.tables.len() - AI_CHAT_MAX_TABLE_NAMES
            ));
        }
    }

    if snapshot.is_empty() {
        system.push_str(
            "\n\nYou have no column information. Ask the user which table they mean, or tell them to open the table, instead of guessing column names.",
        );
    } else {
        system.push_str("\n\nColumns of the relevant tables:");
        for table in &snapshot {
            let columns = table
                .columns
                .iter()
                .map(|column| {
                    let mut rendered = format!("{} {}", column.name, column.data_type);
                    if column.primary_key {
                        rendered.push_str(" PK");
                    }
                    if !column.nullable {
                        rendered.push_str(" NOT NULL");
                    }
                    rendered
                })
                .collect::<Vec<_>>()
                .join(", ");
            system.push_str(&format!("\n{}: {}", table.table, columns));
        }
        system.push_str(
            "\n\nUse only these column names. If the user asks for something that no listed column supports, say so instead of inventing a column.",
        );
    }

    if !relations.is_empty() {
        system.push_str("\n\nForeign keys (use these for joins):");
        for entry in relations.iter().take(AI_CHAT_MAX_RELATIONS) {
            system.push_str(&format!(
                "\n{}.{} -> {}.{}",
                entry.table,
                entry.relation.column,
                entry.relation.referenced_table,
                entry.relation.referenced_column
            ));
        }
        if relations.len() > AI_CHAT_MAX_RELATIONS {
            system.push_str(&format!(
                "\n(+{} more foreign keys)",
                relations.len() - AI_CHAT_MAX_RELATIONS
            ));
        }
    }

    if let Some(query) = &request.editor_query {
        system.push_str(&format!(
            "\n\nSQL currently open in the editor:\n{}",
            query.trim()
        ));
    }

    if let Some(error) = &request.last_error {
        system.push_str("\n\nThe last query run in the editor failed.");
        if let Some(query) = &request.last_query {
            system.push_str(&format!("\nQuery:\n{}", query.trim()));
        }
        system.push_str(&format!("\nError: {error}"));
        system.push_str("\nTake this failure into account and correct it.");
    }

    let mut messages = vec![AiMessage::system(&system)];
    messages.extend(request.history.iter().cloned());
    messages
}

pub(crate) async fn chat_prompt(request: ChatRequest) -> (AiRequestConfig, Vec<AiMessage>) {
    let messages = chat_messages(&request).await;
    (request.config, messages)
}
