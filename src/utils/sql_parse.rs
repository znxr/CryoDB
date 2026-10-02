pub(crate) fn strip_leading_sql_comments(mut input: &str) -> &str {
    loop {
        let trimmed = input.trim_start();
        if trimmed.starts_with("--") {
            if let Some(pos) = trimmed.find('\n') {
                input = &trimmed[pos + 1..];
                continue;
            } else {
                return "";
            }
        } else if trimmed.starts_with("/*") {
            if let Some(pos) = trimmed.find("*/") {
                input = &trimmed[pos + 2..];
                continue;
            } else {
                return "";
            }
        }
        return trimmed;
    }
}

pub(crate) fn statement_ranges(input: &str) -> Vec<(usize, usize)> {
    let bytes = input.as_bytes();
    let mut ranges = Vec::new();
    let mut stmt_start = 0;
    let mut i = 0;
    let mut block_depth: usize = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == quote {
                        if bytes.get(i + 1) == Some(&quote) {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'$' => {
                let tag_start = i;
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'$' {
                    let tag = &bytes[tag_start..=j];
                    i = j + 1;
                    while i + tag.len() <= bytes.len() {
                        if &bytes[i..i + tag.len()] == tag {
                            i += tag.len();
                            break;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'b' | b'B' => {
                let rest = &input[i..];
                if rest
                    .get(..5)
                    .is_some_and(|s| s.eq_ignore_ascii_case("begin"))
                {
                    let before = if i > 0 { bytes[i - 1] as char } else { ' ' };
                    let after = rest
                        .as_bytes()
                        .get(5)
                        .copied()
                        .map(|b| b as char)
                        .unwrap_or(' ');
                    if !is_sql_identifier_char(before) && !is_sql_identifier_char(after) {
                        let next_part = strip_leading_sql_comments(&rest[5..]);
                        let next_word = next_part
                            .split(|value: char| !is_sql_identifier_char(value))
                            .find(|word| !word.is_empty())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        let is_transaction = next_word.is_empty()
                            || matches!(
                                next_word.as_str(),
                                "transaction"
                                    | "work"
                                    | "deferred"
                                    | "immediate"
                                    | "exclusive"
                                    | "isolation"
                                    | "read"
                                    | "deferrable"
                                    | "not"
                            );
                        if !is_transaction {
                            block_depth += 1;
                        }
                        i += 5;
                        continue;
                    }
                }
                i += 1;
            }
            b'e' | b'E' => {
                let rest = &input[i..];
                if rest.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("end")) {
                    let before = if i > 0 { bytes[i - 1] as char } else { ' ' };
                    let after = rest
                        .as_bytes()
                        .get(3)
                        .copied()
                        .map(|b| b as char)
                        .unwrap_or(' ');
                    if !is_sql_identifier_char(before) && !is_sql_identifier_char(after) {
                        let next_word = strip_leading_sql_comments(&rest[3..])
                            .split(|value: char| !is_sql_identifier_char(value))
                            .find(|word| !word.is_empty())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if block_depth > 0
                            && !matches!(
                                next_word.as_str(),
                                "if" | "case" | "loop" | "while" | "repeat"
                            )
                        {
                            block_depth -= 1;
                        }
                        i += 3;
                        continue;
                    }
                }
                i += 1;
            }
            b';' => {
                if block_depth == 0 {
                    let raw = &input[stmt_start..=i];
                    let trimmed = raw.trim();
                    if !trimmed.is_empty() {
                        let leading = raw.len() - raw.trim_start().len();
                        let trailing = raw.len() - raw.trim_end().len();
                        ranges.push((stmt_start + leading, stmt_start + raw.len() - trailing));
                    }
                    stmt_start = i + 1;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if stmt_start < bytes.len() {
        let raw = &input[stmt_start..];
        if !strip_leading_sql_comments(raw).is_empty() {
            let leading = raw.len() - raw.trim_start().len();
            let trailing = raw.len() - raw.trim_end().len();
            ranges.push((stmt_start + leading, stmt_start + raw.len() - trailing));
        }
    }

    ranges
}

pub(crate) fn split_sql_statements(input: &str) -> Vec<&str> {
    statement_ranges(input)
        .into_iter()
        .map(|(start, end)| &input[start..end])
        .collect()
}

pub(crate) fn statement_range_at_offset(input: &str, offset: usize) -> Option<(usize, usize)> {
    let ranges = statement_ranges(input);
    if ranges.is_empty() {
        return None;
    }
    for &(start, end) in &ranges {
        if offset <= end {
            return Some((start, end));
        }
    }
    ranges.last().copied()
}

pub(crate) fn statement_at_offset(input: &str, offset: usize) -> Option<&str> {
    let (start, end) = statement_range_at_offset(input, offset)?;
    Some(&input[start..end])
}

#[derive(Debug, Clone)]
pub(crate) enum SqlToken {
    Word(String),
    Dot,
    Comma,
    LParen,
    RParen,
}

fn is_sql_identifier_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_' || value == '$'
}

pub(crate) fn sql_tokens(input: &str) -> Vec<SqlToken> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let value = bytes[index] as char;
        if value.is_whitespace() {
            index += 1;
            continue;
        }
        if value == '-' && index + 1 < bytes.len() && bytes[index + 1] == b'-' {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if value == '/' && index + 1 < bytes.len() && bytes[index + 1] == b'*' {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            if index + 1 < bytes.len() {
                index += 2;
            }
            continue;
        }
        if value == '\'' {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if value == '"' {
            index += 1;
            let mut value = String::new();
            while index < bytes.len() {
                if bytes[index] == b'"' {
                    if index + 1 < bytes.len() && bytes[index + 1] == b'"' {
                        value.push('"');
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                value.push(bytes[index] as char);
                index += 1;
            }
            tokens.push(SqlToken::Word(value));
            continue;
        }
        if value == '`' {
            index += 1;
            let mut value = String::new();
            while index < bytes.len() {
                if bytes[index] == b'`' {
                    if index + 1 < bytes.len() && bytes[index + 1] == b'`' {
                        value.push('`');
                        index += 2;
                        continue;
                    }
                    index += 1;
                    break;
                }
                value.push(bytes[index] as char);
                index += 1;
            }
            tokens.push(SqlToken::Word(value));
            continue;
        }
        if is_sql_identifier_char(value) {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                if is_sql_identifier_char(next) {
                    index += 1;
                } else {
                    break;
                }
            }
            let word = &input[start..index];
            tokens.push(SqlToken::Word(word.to_string()));
            continue;
        }
        match value {
            '.' => {
                tokens.push(SqlToken::Dot);
                index += 1;
            }
            ',' => {
                tokens.push(SqlToken::Comma);
                index += 1;
            }
            '(' => {
                tokens.push(SqlToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(SqlToken::RParen);
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }
    tokens
}

fn is_sql_join_keyword(word: &str) -> bool {
    word.eq_ignore_ascii_case("join")
        || word.eq_ignore_ascii_case("inner")
        || word.eq_ignore_ascii_case("left")
        || word.eq_ignore_ascii_case("right")
        || word.eq_ignore_ascii_case("full")
        || word.eq_ignore_ascii_case("cross")
        || word.eq_ignore_ascii_case("straight_join")
        || word.eq_ignore_ascii_case("natural")
}

fn is_sql_clause_keyword(word: &str) -> bool {
    word.eq_ignore_ascii_case("where")
        || word.eq_ignore_ascii_case("group")
        || word.eq_ignore_ascii_case("order")
        || word.eq_ignore_ascii_case("limit")
        || word.eq_ignore_ascii_case("having")
        || word.eq_ignore_ascii_case("union")
        || word.eq_ignore_ascii_case("into")
        || word.eq_ignore_ascii_case("for")
        || word.eq_ignore_ascii_case("lock")
        || word.eq_ignore_ascii_case("procedure")
        || word.eq_ignore_ascii_case("window")
}

pub(crate) fn sql_offset_is_inert(input: &str, offset: usize) -> bool {
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        let open_ended;
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            open_ended = index >= bytes.len();
            index = (index + 1).min(bytes.len());
        } else if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            open_ended = index + 1 >= bytes.len();
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == b'\'' {
            index += 1;
            let mut closed = false;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                if bytes[index] == b'\'' {
                    index += 1;
                    closed = true;
                    break;
                }
                index += 1;
            }
            open_ended = !closed;
        } else {
            index += 1;
            continue;
        }

        if offset > start && (offset < index || open_ended) {
            return true;
        }
    }
    false
}

pub(crate) fn parse_single_table_select(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed = trimmed.trim_end_matches(';').trim_end();
    if trimmed.is_empty() || trimmed.contains(';') {
        return None;
    }

    let tokens = sql_tokens(trimmed);
    let mut iter = tokens.iter().peekable();
    let mut depth = 0usize;
    let mut saw_select = false;

    for token in iter.by_ref() {
        match token {
            SqlToken::LParen => depth = depth.saturating_add(1),
            SqlToken::RParen => depth = depth.saturating_sub(1),
            SqlToken::Word(word) if depth == 0 => {
                if word.eq_ignore_ascii_case("select") {
                    saw_select = true;
                    break;
                }
                return None;
            }
            _ => {}
        }
    }

    if !saw_select {
        return None;
    }

    depth = 0;
    let mut saw_from = false;
    for token in iter.by_ref() {
        match token {
            SqlToken::LParen => depth = depth.saturating_add(1),
            SqlToken::RParen => depth = depth.saturating_sub(1),
            SqlToken::Word(word) if depth == 0 && word.eq_ignore_ascii_case("from") => {
                saw_from = true;
                break;
            }
            _ => {}
        }
    }

    if !saw_from {
        return None;
    }

    let mut table_parts = Vec::new();
    loop {
        let token = iter.next()?;
        match token {
            SqlToken::Word(word) => {
                table_parts.push(word.clone());
                if let Some(SqlToken::Dot) = iter.peek() {
                    iter.next();
                    if table_parts.len() >= 2 {
                        return None;
                    }
                    continue;
                }
                break;
            }
            SqlToken::LParen => return None,
            _ => {}
        }
    }

    let table_name = table_parts.join(".");

    if let Some(SqlToken::Word(word)) = iter.peek() {
        if word.eq_ignore_ascii_case("as") {
            iter.next();
            if let Some(SqlToken::Word(_)) = iter.peek() {
                iter.next();
            }
        } else if !is_sql_clause_keyword(word) && !is_sql_join_keyword(word) {
            iter.next();
        }
    }

    depth = 0;
    for token in iter {
        match token {
            SqlToken::LParen => depth = depth.saturating_add(1),
            SqlToken::RParen => depth = depth.saturating_sub(1),
            SqlToken::Comma if depth == 0 => return None,
            SqlToken::Word(word) if depth == 0 => {
                if is_sql_join_keyword(word.as_str()) {
                    return None;
                }
                if is_sql_clause_keyword(word.as_str()) {
                    break;
                }
            }
            _ => {}
        }
    }

    Some(table_name)
}

pub(crate) fn schema_metadata_refresh_from_sql(query: &str) -> (bool, bool) {
    let mut normalized = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(' ');
        }
    }

    let refresh_databases = normalized.contains("create database")
        || normalized.contains("drop database")
        || normalized.contains("alter database")
        || normalized.contains("rename database");

    let refresh_schemas = normalized.contains("create schema")
        || normalized.contains("drop schema")
        || normalized.contains("alter schema")
        || normalized.contains("rename schema");

    let refresh_extensions = normalized.contains("create extension")
        || normalized.contains("drop extension")
        || normalized.contains("alter extension");

    let refresh_sequences = normalized.contains("create sequence")
        || normalized.contains("drop sequence")
        || normalized.contains("alter sequence");

    let refresh_materialized_views = normalized.contains("create materialized view")
        || normalized.contains("drop materialized view")
        || normalized.contains("alter materialized view")
        || normalized.contains("refresh materialized view");

    let refresh_tables = normalized.contains("create table")
        || normalized.contains("create temporary table")
        || normalized.contains("drop table")
        || normalized.contains("drop temporary table")
        || normalized.contains("delete table")
        || normalized.contains("alter table")
        || normalized.contains("rename table")
        || normalized.contains("truncate table")
        || normalized.contains("create view")
        || normalized.contains("drop view")
        || normalized.contains("alter view")
        || normalized.contains("rename view")
        || refresh_databases
        || refresh_schemas
        || refresh_extensions
        || refresh_sequences
        || refresh_materialized_views;

    (refresh_tables, refresh_databases || refresh_schemas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_offsets_cover_strings_and_comments_but_not_their_edges() {
        let sql = "SELECT 'abc' FROM t -- note\nWHERE x = 1";
        let open = sql.find('\'').expect("quote");
        assert!(!sql_offset_is_inert(sql, open));
        assert!(sql_offset_is_inert(sql, open + 2));
        assert!(!sql_offset_is_inert(sql, open + 5));

        let comment = sql.find("--").expect("comment");
        assert!(sql_offset_is_inert(sql, comment + 4));
        assert!(!sql_offset_is_inert(sql, sql.len()));

        assert!(sql_offset_is_inert("SELECT 'abc", 9));
        assert!(sql_offset_is_inert("SELECT 1 /* wip", 12));
        assert!(!sql_offset_is_inert("SELECT 1 /* done */ ", 20));
        assert!(!sql_offset_is_inert("SELECT \"col\" ", 9));
    }

    #[test]
    fn sql_tokens_capture_postgres_quoted_identifier_words() {
        let tokens = sql_tokens("SELECT * FROM \"public\".\"users\"");
        let words = tokens
            .into_iter()
            .filter_map(|token| match token {
                SqlToken::Word(word) => Some(word),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(words, vec!["SELECT", "FROM", "public", "users"]);
    }

    #[test]
    fn parse_single_table_select_handles_schema_qualified_names() {
        assert_eq!(
            parse_single_table_select("SELECT * FROM public.users"),
            Some(String::from("public.users"))
        );
        assert_eq!(
            parse_single_table_select("SELECT * FROM \"public\".\"users\" AS u"),
            Some(String::from("public.users"))
        );
    }

    #[test]
    fn parse_single_table_select_rejects_join_queries() {
        assert_eq!(
            parse_single_table_select("SELECT * FROM public.users JOIN public.roles ON true"),
            None
        );
    }

    #[test]
    fn schema_refresh_detection_covers_postgres_ddl() {
        let (tables, databases) = schema_metadata_refresh_from_sql("CREATE SCHEMA analytics");
        assert!(tables);
        assert!(databases);

        let (tables, databases) = schema_metadata_refresh_from_sql("CREATE EXTENSION pgvector");
        assert!(tables);
        assert!(!databases);

        let (tables, databases) =
            schema_metadata_refresh_from_sql("REFRESH MATERIALIZED VIEW analytics.mv_users");
        assert!(tables);
        assert!(!databases);
    }

    #[test]
    fn split_sql_statements_handles_multiple_statements() {
        let sql = "CREATE TABLE t (id INT);\nINSERT INTO t VALUES (1);";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("CREATE"));
        assert!(stmts[1].starts_with("INSERT"));
    }

    #[test]
    fn split_sql_statements_ignores_semicolons_in_strings_and_comments() {
        let sql = "SELECT 'a;b' FROM t;\nSELECT 1";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("'a;b'"));
        assert_eq!(stmts[1], "SELECT 1");
    }

    #[test]
    fn statement_at_offset_returns_correct_statement() {
        let sql = "SELECT 1;\nSELECT 2;";
        let s1 = statement_at_offset(sql, 2).unwrap();
        assert_eq!(s1, "SELECT 1;");
        let s2_offset = sql.find("SELECT 2").unwrap();
        let s2 = statement_at_offset(sql, s2_offset + 2).unwrap();
        assert_eq!(s2, "SELECT 2;");
    }

    #[test]
    fn statement_at_offset_without_trailing_semicolon() {
        let sql = "SELECT 1;\nSELECT 2";
        let s2_offset = sql.find("SELECT 2").unwrap();
        let s2 = statement_at_offset(sql, s2_offset + 2).unwrap();
        assert_eq!(s2, "SELECT 2");
    }

    #[test]
    fn split_sql_statements_handles_postgres_dollar_quotes_and_compound_blocks() {
        let sql = "DO $$ BEGIN PERFORM 1; END $$;\nSELECT 2;";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].starts_with("DO $$"));
        assert_eq!(stmts[1], "SELECT 2;");
    }

    #[test]
    fn split_sql_statements_keeps_nested_blocks_and_transaction_modes() {
        let sql = "CREATE PROCEDURE p() BEGIN IF 1 THEN SELECT 1; END IF; SELECT 2; END; SELECT 3;";
        assert_eq!(split_sql_statements(sql).len(), 2);
        assert_eq!(
            split_sql_statements("BEGIN ISOLATION LEVEL SERIALIZABLE; SELECT 1; COMMIT;").len(),
            3
        );
    }

    #[test]
    fn split_sql_statements_ignores_comment_only_tail() {
        assert_eq!(split_sql_statements("SELECT 1; -- note").len(), 1);
    }

    #[test]
    fn strip_leading_sql_comments_removes_leading_comments() {
        let sql = "-- comment line\n/* block comment */\nSELECT * FROM users;";
        let stripped = strip_leading_sql_comments(sql);
        assert_eq!(stripped, "SELECT * FROM users;");
    }
}
