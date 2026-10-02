use super::State;
use super::types::ChatDirective;
use crate::app::types::DiagramAgentAction;
use crate::constants::{
    AI_CHAT_DIRECTIVE_PREFIX, AI_CHAT_MAX_CONTEXT_TABLES, AI_CHAT_TITLE_MAX_CHARS,
    QUERY_INLINE_AI_MAX_CHARS,
};

impl State {
    pub(crate) fn shorten_chat_title(value: &str) -> String {
        let title = value
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '.' | '`'))
            .replace('\n', " ");
        if title.chars().count() > AI_CHAT_TITLE_MAX_CHARS {
            format!(
                "{}...",
                title
                    .chars()
                    .take(AI_CHAT_TITLE_MAX_CHARS)
                    .collect::<String>()
                    .trim_end()
            )
        } else {
            title
        }
    }
    pub(crate) fn chat_directive(reply: &str) -> Option<ChatDirective> {
        let line = reply.lines().map(str::trim).find_map(|line| {
            let line = line.trim_matches(|ch| matches!(ch, '`' | '*' | ' '));
            line.strip_prefix(AI_CHAT_DIRECTIVE_PREFIX)
                .map(|rest| rest.trim())
        })?;

        let (tool, args) = match line.split_once(char::is_whitespace) {
            Some((tool, args)) => (tool, args.trim()),
            None => (line, ""),
        };

        let clean = |value: &str| {
            value
                .trim()
                .trim_matches(|ch| matches!(ch, '`' | '"' | '\'' | ';' | ','))
                .trim()
                .to_string()
        };

        let tool = tool.to_ascii_lowercase();
        match tool.as_str() {
            "schema" => {
                let tables = args
                    .split(',')
                    .map(clean)
                    .filter(|table| !table.is_empty())
                    .take(AI_CHAT_MAX_CONTEXT_TABLES)
                    .collect::<Vec<_>>();
                if tables.is_empty() {
                    Some(ChatDirective::Malformed(String::from(
                        "schema needs at least one table name",
                    )))
                } else {
                    Some(ChatDirective::Schema(tables))
                }
            }
            "sample" => {
                let table = clean(args);
                if table.is_empty() {
                    Some(ChatDirective::Malformed(String::from(
                        "sample needs one table name",
                    )))
                } else {
                    Some(ChatDirective::Sample(table))
                }
            }
            "explain" => {
                let sql = clean(args);
                if sql.is_empty() {
                    Some(ChatDirective::Malformed(String::from(
                        "explain needs a SQL statement",
                    )))
                } else {
                    Some(ChatDirective::Explain(sql))
                }
            }
            "search" => {
                let term = clean(args);
                if term.len() < 2 {
                    Some(ChatDirective::Malformed(String::from(
                        "search needs a term of at least two characters",
                    )))
                } else {
                    Some(ChatDirective::Search(term))
                }
            }
            "open-tab" | "open_tab" => Some(ChatDirective::OpenTab),
            "diagram" => Some(ChatDirective::Diagram(DiagramAgentAction::Open)),
            "diagram-layout" | "diagram_layout" => {
                Some(ChatDirective::Diagram(DiagramAgentAction::Layout))
            }
            "diagram-focus" | "diagram_focus" => {
                let table = clean(args);
                if table.is_empty() {
                    Some(ChatDirective::Malformed(String::from(
                        "diagram-focus needs one table name",
                    )))
                } else {
                    Some(ChatDirective::Diagram(DiagramAgentAction::Focus(table)))
                }
            }
            "diagram-area" | "diagram_area" => match args.split_once(':') {
                Some((name, tables)) => {
                    let name = clean(name);
                    let tables = tables
                        .split(',')
                        .map(clean)
                        .filter(|table| !table.is_empty())
                        .take(AI_CHAT_MAX_CONTEXT_TABLES)
                        .collect::<Vec<_>>();
                    if name.is_empty() || tables.is_empty() {
                        Some(ChatDirective::Malformed(String::from(
                            "diagram-area needs a name and at least one table",
                        )))
                    } else {
                        Some(ChatDirective::Diagram(DiagramAgentAction::Area {
                            name,
                            tables,
                        }))
                    }
                }
                None => Some(ChatDirective::Malformed(String::from(
                    "diagram-area needs `name: table_one, table_two`",
                ))),
            },
            "diagram-lock" | "diagram_lock" | "diagram-unlock" | "diagram_unlock" => {
                let name = clean(args);
                if name.is_empty() {
                    Some(ChatDirective::Malformed(String::from(
                        "diagram-lock needs the area name",
                    )))
                } else {
                    Some(ChatDirective::Diagram(DiagramAgentAction::Lock {
                        name,
                        locked: !tool.contains("unlock"),
                    }))
                }
            }
            "diagram-note" | "diagram_note" => match args.split_once(':') {
                Some((table, text)) => {
                    let table = clean(table);
                    let text = text.trim().to_string();
                    if table.is_empty() || text.is_empty() {
                        Some(ChatDirective::Malformed(String::from(
                            "diagram-note needs a table and the note text",
                        )))
                    } else {
                        Some(ChatDirective::Diagram(DiagramAgentAction::Note {
                            table,
                            text,
                        }))
                    }
                }
                None => Some(ChatDirective::Malformed(String::from(
                    "diagram-note needs `table_one: the note text`",
                ))),
            },
            "diagram-add-table" | "diagram_add_table" => match args.split_once(':') {
                Some((table, columns)) => {
                    let table = clean(table);
                    let columns = columns.trim().to_string();
                    if table.is_empty() || columns.is_empty() {
                        Some(ChatDirective::Malformed(String::from(
                            "diagram-add-table needs `name: column TYPE, column TYPE`",
                        )))
                    } else {
                        Some(ChatDirective::Diagram(DiagramAgentAction::AddTable {
                            table,
                            columns,
                        }))
                    }
                }
                None => Some(ChatDirective::Malformed(String::from(
                    "diagram-add-table needs `name: column TYPE, column TYPE`",
                ))),
            },
            "diagram-add-column" | "diagram_add_column" => match args.split_once(':') {
                Some((target, data_type)) => {
                    let data_type = clean(data_type);
                    match target.split_once('.') {
                        Some((table, column)) if !data_type.is_empty() => {
                            let (table, column) = (clean(table), clean(column));
                            if table.is_empty() || column.is_empty() {
                                Some(ChatDirective::Malformed(String::from(
                                    "diagram-add-column needs `table.column: TYPE`",
                                )))
                            } else {
                                Some(ChatDirective::Diagram(DiagramAgentAction::AddColumn {
                                    table,
                                    column,
                                    data_type,
                                }))
                            }
                        }
                        _ => Some(ChatDirective::Malformed(String::from(
                            "diagram-add-column needs `table.column: TYPE`",
                        ))),
                    }
                }
                None => Some(ChatDirective::Malformed(String::from(
                    "diagram-add-column needs `table.column: TYPE`",
                ))),
            },
            "diagram-link" | "diagram_link" => {
                let pair = args.split_once(':').and_then(|(source, target)| {
                    let (table, column) = source.split_once('.')?;
                    let (referenced_table, referenced_column) = target.split_once('.')?;
                    Some((
                        clean(table),
                        clean(column),
                        clean(referenced_table),
                        clean(referenced_column),
                    ))
                });
                match pair {
                    Some((table, column, referenced_table, referenced_column))
                        if !table.is_empty()
                            && !column.is_empty()
                            && !referenced_table.is_empty()
                            && !referenced_column.is_empty() =>
                    {
                        Some(ChatDirective::Diagram(DiagramAgentAction::Link {
                            table,
                            column,
                            referenced_table,
                            referenced_column,
                        }))
                    }
                    _ => Some(ChatDirective::Malformed(String::from(
                        "diagram-link needs `table.column: other_table.other_column`",
                    ))),
                }
            }
            other => Some(ChatDirective::Malformed(format!("unknown tool `{other}`"))),
        }
    }
    pub(crate) fn chat_sql_is_read_only(sql: &str) -> bool {
        let mut statements = 0;
        for statement in sql.split(';') {
            let cleaned = statement
                .lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("--") && !line.starts_with('#'))
                .collect::<Vec<_>>()
                .join(" ");
            let cleaned = cleaned.trim().to_ascii_lowercase();
            if cleaned.is_empty() {
                continue;
            }
            statements += 1;

            let keyword = cleaned.split_whitespace().next().unwrap_or_default();
            let allowed = matches!(
                keyword,
                "select" | "show" | "explain" | "describe" | "desc" | "pragma" | "values" | "with"
            );
            if !allowed {
                return false;
            }

            let words = cleaned
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>();
            if words
                .windows(2)
                .any(|pair| pair[0] == "for" && matches!(pair[1], "update" | "share" | "no"))
                || words.contains(&"into")
                || (keyword == "pragma" && cleaned.contains('='))
            {
                return false;
            }
            if words.iter().any(|word| {
                matches!(
                    *word,
                    "insert"
                        | "update"
                        | "delete"
                        | "merge"
                        | "truncate"
                        | "drop"
                        | "alter"
                        | "create"
                        | "grant"
                        | "revoke"
                        | "replace"
                        | "call"
                        | "do"
                        | "lock"
                        | "vacuum"
                        | "attach"
                        | "copy"
                        | "setval"
                        | "nextval"
                        | "load_extension"
                        | "writefile"
                        | "dblink"
                        | "dblink_exec"
                        | "lo_import"
                        | "lo_export"
                        | "pg_read_file"
                        | "pg_ls_dir"
                        | "pg_sleep"
                        | "sleep"
                        | "benchmark"
                )
            }) {
                return false;
            }
        }

        statements == 1
    }
    pub(crate) fn chat_sql_blocks(content: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        let mut rest = content;
        while let Some(start) = rest.find("```") {
            let after = &rest[start + 3..];
            let Some(newline) = after.find('\n') else {
                break;
            };
            let language = after[..newline].trim().to_ascii_lowercase();
            let body = &after[newline + 1..];
            let Some(end) = body.find("```") else {
                break;
            };
            if language.is_empty() || language == "sql" {
                let block = body[..end].trim();
                if !block.is_empty() {
                    blocks.push(block.to_string());
                }
            }
            rest = &body[end + 3..];
        }
        blocks
    }
    pub(crate) fn normalize_inline_ai_suggestion(anchor: &str, raw: &str) -> Option<String> {
        let mut suggestion = raw.trim_end().to_string();

        if let Some((_, tail)) = suggestion.split_once("<CURSOR>") {
            suggestion = tail.to_string();
        }

        if let Some(rest) = suggestion.strip_prefix(anchor) {
            suggestion = rest.to_string();
        }

        let line = suggestion.trim_start_matches(['\n', '\r']).lines().next()?;
        if line.trim().is_empty() {
            return None;
        }

        Some(line.chars().take(QUERY_INLINE_AI_MAX_CHARS).collect())
    }
}
