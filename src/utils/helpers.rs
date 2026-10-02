use crate::*;
use fontdb::Database;
use std::collections::BTreeSet;
use std::fmt::Write;

pub(crate) fn default_database_driver() -> DatabaseDriver {
    DatabaseDriver::MySql
}

pub(crate) fn lerp_color(start: Color, end: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: start.r + (end.r - start.r) * t,
        g: start.g + (end.g - start.g) * t,
        b: start.b + (end.b - start.b) * t,
        a: start.a + (end.a - start.a) * t,
    }
}

pub(crate) fn build_font_choices() -> Vec<FontChoice> {
    let mut choices = vec![FontChoice::Sans, FontChoice::Monospace];
    let mut database = Database::new();
    database.load_system_fonts();

    let mut families = BTreeSet::new();
    for face in database.faces() {
        if let Some((family, _)) = face.families.first() {
            let family = family.trim();
            if family.is_empty()
                || family.eq_ignore_ascii_case("sans")
                || family.eq_ignore_ascii_case("system font")
                || family.eq_ignore_ascii_case("monospace")
            {
                continue;
            }
            families.insert(family.to_string());
        }
    }

    choices.extend(families.into_iter().map(FontChoice::System));
    choices
}

pub(crate) fn column_kind_from_type_name(name: &str) -> ColumnKind {
    let upper = name.trim().to_ascii_uppercase();
    let base = upper.split_whitespace().next().unwrap_or("");
    let base = base.split('(').next().unwrap_or(base);

    if base.starts_with('_') {
        return ColumnKind::Text;
    }

    match base {
        "BOOLEAN" | "BOOL" => ColumnKind::Bool,
        "TINYINT" | "SMALLINT" | "SMALLSERIAL" | "INT" | "INT2" | "INT4" | "INT8" | "INTEGER"
        | "SERIAL" | "BIGSERIAL" | "MEDIUMINT" | "BIGINT" | "YEAR" => {
            if upper.contains("UNSIGNED") {
                ColumnKind::Unsigned
            } else {
                ColumnKind::Integer
            }
        }
        "BIT" => ColumnKind::Unsigned,
        "FLOAT" | "FLOAT4" | "FLOAT8" | "DOUBLE" | "DOUBLE PRECISION" | "REAL" => ColumnKind::Float,
        "DECIMAL" | "DEC" | "NUMERIC" | "FIXED" => ColumnKind::Decimal,
        "DATE" => ColumnKind::Date,
        "TIME" | "TIMETZ" => ColumnKind::Time,
        "DATETIME" | "TIMESTAMP" | "TIMESTAMPTZ" => ColumnKind::DateTime,
        "JSON" | "JSONB" | "UUID" | "CHAR" | "BPCHAR" | "VARCHAR" | "TEXT" | "TINYTEXT"
        | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" | "CLOB" | "NCHAR" | "NVARCHAR" | "NAME"
        | "VECTOR" | "HALFVEC" | "SPARSEVEC" => ColumnKind::Text,
        "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "GEOMETRY"
        | "BYTEA" => ColumnKind::Binary,
        _ => ColumnKind::Unknown,
    }
}

pub(crate) fn binary_preview(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("<0 bytes>");
    }

    let mut output = String::with_capacity(2 + BINARY_PREVIEW_BYTES * 2 + 24);
    output.push_str("0x");

    let show = bytes.len().min(BINARY_PREVIEW_BYTES);
    for byte in &bytes[..show] {
        let _ = write!(&mut output, "{:02x}", byte);
    }

    if bytes.len() > show {
        output.push_str("...");
    }

    let _ = write!(&mut output, " ({} bytes)", bytes.len());
    output
}

pub(crate) fn binary_text_value(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return None;
    }
    Some(text.to_string())
}

pub(crate) fn is_binary_preview_value(value: &str) -> bool {
    value == "<0 bytes>" || (value.starts_with("0x") && value.ends_with(" bytes)"))
}

pub(crate) fn looks_like_json(value: &str) -> bool {
    let trimmed_start = value.trim_start();
    if !(trimmed_start.starts_with('{') || trimmed_start.starts_with('[')) {
        return false;
    }
    let trimmed_end = value.trim_end();
    trimmed_end.ends_with('}') || trimmed_end.ends_with(']')
}

pub(crate) fn is_valid_json(value: &str) -> bool {
    if !looks_like_json(value) {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(value).is_ok()
}

pub(crate) fn register_command(
    registry: &mut Vec<OmniCommandEntry>,
    id: &'static str,
    title: &'static str,
    handler: OmniCommandHandler,
    category: &'static str,
    keywords: &'static [&'static str],
) {
    registry.push(OmniCommandEntry {
        id,
        title,
        handler,
        category,
        keywords,
    });
}

pub(crate) fn strip_ascii_prefix_case_insensitive<'a>(
    value: &'a str,
    prefix: &str,
) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        value.get(prefix.len()..)
    } else {
        None
    }
}

pub(crate) fn parse_omni_setting_value(query: &str, prefix: &str) -> Option<String> {
    let value = strip_ascii_prefix_case_insensitive(query.trim(), prefix)?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub(crate) fn parse_omni_setting_usize(query: &str, prefix: &str) -> Option<usize> {
    let value = strip_ascii_prefix_case_insensitive(query.trim(), prefix)?;
    value.trim().parse::<usize>().ok()
}

pub(crate) fn normalize_omni_prefix(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        String::from(":")
    } else {
        trimmed.chars().take(8).collect()
    }
}

pub(crate) fn resolve_theme_choice(value: &str) -> Option<ThemeChoice> {
    let normalized = normalize_omni_lookup(value);
    ThemeChoice::ALL
        .iter()
        .copied()
        .find(|theme| normalize_omni_lookup(&theme.to_string()) == normalized)
}

pub(crate) fn resolve_ui_density(value: &str) -> Option<UiDensity> {
    let normalized = normalize_omni_lookup(value);
    DENSITY_CHOICES
        .iter()
        .copied()
        .find(|density| normalize_omni_lookup(&density.to_string()) == normalized)
}

pub(crate) fn resolve_ai_provider(value: &str) -> Option<AiProvider> {
    let normalized = normalize_omni_lookup(value);
    AI_PROVIDERS
        .iter()
        .copied()
        .find(|provider| normalize_omni_lookup(&provider.to_string()) == normalized)
}

pub(crate) fn parse_shortcut_binding(value: &str) -> Option<ShortcutBinding> {
    ShortcutBinding::parse(value)
}

pub(crate) fn format_sql_for_palette(query: &str) -> String {
    let keywords = [
        "select", "from", "where", "join", "inner", "left", "right", "outer", "on", "group", "by",
        "order", "limit", "offset", "insert", "into", "values", "update", "set", "delete",
        "create", "drop", "alter", "table", "and", "or", "not", "as", "having", "union", "all",
        "distinct", "case", "when", "then", "else", "end",
    ];

    query
        .split_whitespace()
        .map(|token| {
            let normalized = token.to_ascii_lowercase();
            if keywords.contains(&normalized.as_str()) {
                normalized.to_ascii_uppercase()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_query_identifier_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || value == '_'
}

pub(crate) fn column_to_byte_offset(value: &str, column: usize) -> usize {
    if column == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or_else(|| value.len())
}

pub(crate) fn normalize_query(query: &str) -> String {
    query
        .trim()
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

pub(crate) fn escape_mysql_identifier(input: &str) -> String {
    input.replace('`', "``")
}

pub(crate) fn escape_sqlite_identifier(input: &str) -> String {
    input.replace('"', "\"\"")
}

pub(crate) fn escape_postgres_identifier(input: &str) -> String {
    input.replace('"', "\"\"")
}

pub(crate) fn sql_quote_identifier(driver: DatabaseDriver, input: &str) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
            format!("`{}`", escape_mysql_identifier(input))
        }
        DatabaseDriver::Sqlite => format!("\"{}\"", escape_sqlite_identifier(input)),
        DatabaseDriver::PostgreSql => {
            format!("\"{}\"", escape_postgres_identifier(input))
        }
    }
}

pub(crate) fn sql_quote_identifier_path(driver: DatabaseDriver, segments: &[&str]) -> String {
    segments
        .iter()
        .map(|segment| sql_quote_identifier(driver, segment))
        .collect::<Vec<_>>()
        .join(".")
}

pub(crate) fn escape_sqlite_string_literal(input: &str) -> String {
    input.replace('\'', "''")
}

pub(crate) fn escape_mysql_string_literal(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\'', "''")
}

pub(crate) fn escape_postgres_string_literal(input: &str) -> String {
    input.replace('\'', "''")
}

pub(crate) fn sql_escape_string_literal(driver: DatabaseDriver, input: &str) -> String {
    match driver {
        DatabaseDriver::MySql | DatabaseDriver::MariaDb => escape_mysql_string_literal(input),
        DatabaseDriver::Sqlite => escape_sqlite_string_literal(input),
        DatabaseDriver::PostgreSql => escape_postgres_string_literal(input),
    }
}

pub(crate) fn sql_quote_string_literal(driver: DatabaseDriver, input: &str) -> String {
    format!("'{}'", sql_escape_string_literal(driver, input))
}

pub(crate) fn sql_bind_placeholder(driver: DatabaseDriver, bind_index: usize) -> String {
    match driver {
        DatabaseDriver::PostgreSql => format!("${}", bind_index.max(1)),
        _ => String::from("?"),
    }
}

fn split_postgres_qualified_name(input: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    let mut chars = input.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '"' {
            if in_quotes {
                if let Some((_, next)) = chars.peek().copied()
                    && next == '"'
                {
                    let _ = chars.next();
                    continue;
                }
                in_quotes = false;
            } else {
                in_quotes = true;
            }
            continue;
        }

        if ch == '.' && !in_quotes {
            let left = input[..index].trim();
            let right = input[index + ch.len_utf8()..].trim();
            if !left.is_empty() && !right.is_empty() {
                return Some((left, right));
            }
            return None;
        }
    }

    None
}

pub(crate) fn is_postgres_vector_type_name(type_name: &str) -> bool {
    let normalized = type_name.trim().trim_matches('"').to_ascii_lowercase();
    let leaf = normalized.rsplit('.').next().unwrap_or(normalized.as_str());
    matches!(leaf, "vector" | "halfvec" | "sparsevec")
}

pub(crate) fn sql_quote_table_reference(driver: DatabaseDriver, table: &str) -> String {
    let trimmed = table.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if driver != DatabaseDriver::PostgreSql {
        return sql_quote_identifier(driver, trimmed);
    }

    if let Some((schema, table_name)) = split_postgres_qualified_name(trimmed) {
        let schema = schema.trim().trim_matches('"');
        let table_name = table_name.trim().trim_matches('"');
        if !schema.is_empty() && !table_name.is_empty() {
            return sql_quote_identifier_path(driver, &[schema, table_name]);
        }
    }

    sql_quote_identifier(driver, trimmed.trim_matches('"'))
}

fn normalize_postgres_segment(segment: &str) -> String {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].replace("\"\"", "\"")
    } else {
        trimmed.to_ascii_lowercase()
    }
}

pub(crate) fn canonical_table_key(driver: DatabaseDriver, table: &str) -> String {
    let trimmed = table.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if driver != DatabaseDriver::PostgreSql {
        return trimmed.to_string();
    }

    let (schema_raw, table_raw) =
        if let Some((schema, table_name)) = split_postgres_qualified_name(trimmed) {
            (Some(schema), table_name)
        } else {
            (None, trimmed)
        };

    let table_name = normalize_postgres_segment(table_raw);
    if table_name.is_empty() {
        return String::new();
    }

    let schema_name = schema_raw
        .map(normalize_postgres_segment)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("public"));

    format!("{schema_name}.{table_name}")
}

pub(crate) fn format_definer(definer: &str) -> Option<String> {
    let trimmed = definer.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (user, host) = match trimmed.rsplit_once('@') {
        Some((user, host)) => (user.trim(), host.trim()),
        None => (trimmed, "%"),
    };
    if user.is_empty() {
        return None;
    }
    let host = if host.is_empty() { "%" } else { host };
    Some(format!(
        "DEFINER = `{}`@`{}`",
        escape_mysql_identifier(user),
        escape_mysql_identifier(host)
    ))
}

pub(crate) fn is_null_value(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("null")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_kind_from_type_name_maps_postgres_types() {
        assert_eq!(
            column_kind_from_type_name("TIMESTAMPTZ"),
            ColumnKind::DateTime
        );
        assert_eq!(column_kind_from_type_name("BYTEA"), ColumnKind::Binary);
        assert_eq!(column_kind_from_type_name("UUID"), ColumnKind::Text);
        assert_eq!(column_kind_from_type_name("vector"), ColumnKind::Text);
        assert_eq!(column_kind_from_type_name("_int4"), ColumnKind::Text);
        assert_eq!(
            column_kind_from_type_name("BIGINT UNSIGNED"),
            ColumnKind::Unsigned
        );
    }

    #[test]
    fn postgres_vector_type_name_detection_handles_schema_prefix() {
        assert!(is_postgres_vector_type_name("vector"));
        assert!(is_postgres_vector_type_name("public.vector"));
        assert!(is_postgres_vector_type_name("pgvector.sparsevec"));
        assert!(!is_postgres_vector_type_name("jsonb"));
    }

    #[test]
    fn sql_quote_table_reference_quotes_postgres_schema_and_table() {
        assert_eq!(
            sql_quote_table_reference(DatabaseDriver::PostgreSql, "public.users"),
            "\"public\".\"users\""
        );
        assert_eq!(
            sql_quote_table_reference(DatabaseDriver::PostgreSql, "\"Mixed\".\"Users\""),
            "\"Mixed\".\"Users\""
        );
    }

    #[test]
    fn canonical_table_key_normalizes_postgres_names() {
        assert_eq!(
            canonical_table_key(DatabaseDriver::PostgreSql, "Users"),
            "public.users"
        );
        assert_eq!(
            canonical_table_key(DatabaseDriver::PostgreSql, "\"Camel\".\"Users\""),
            "Camel.Users"
        );
        assert_eq!(canonical_table_key(DatabaseDriver::MySql, "users"), "users");
    }

    #[test]
    fn bind_placeholders_use_postgres_indexes_only_for_postgres() {
        assert_eq!(sql_bind_placeholder(DatabaseDriver::PostgreSql, 1), "$1");
        assert_eq!(sql_bind_placeholder(DatabaseDriver::PostgreSql, 3), "$3");
        assert_eq!(sql_bind_placeholder(DatabaseDriver::MySql, 2), "?");
        assert_eq!(sql_bind_placeholder(DatabaseDriver::Sqlite, 2), "?");
    }
}
