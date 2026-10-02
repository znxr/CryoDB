use crate::ai::AiProvider;
use crate::app::types::OmniBarMode;
use crate::model::settings::{ResultGridDensity, UiDensity};
use iced::Color;
use std::sync::atomic::AtomicU64;

pub(crate) const SIDEBAR_CONTAINER_PADDING: u16 = 6;

pub(crate) const fn color_hex(hex: u32) -> Color {
    Color::from_rgb8(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub(crate) const RESPONSIVE_BREAKPOINT: f32 = 980.0;
pub(crate) const HEADER_PICKER_HIDE_WIDTH: f32 = 1320.0;
pub(crate) const PANE_MIN_SIZE: f32 = 170.0;
pub(crate) const QUERY_EDITOR_MIN_HEIGHT: f32 = 120.0;
pub(crate) const DEFAULT_EDITOR_HEIGHT: f32 = 140.0;
pub(crate) const TRIGGER_EDITOR_EXTRA_HEIGHT: f32 = 120.0;
pub(crate) const BASE_HISTORY_HEIGHT_WIDE: f32 = 120.0;
pub(crate) const BASE_HISTORY_HEIGHT_COMPACT: f32 = 100.0;
pub(crate) const BASE_HISTORY_HEIGHT_EXPANDED_WIDE: f32 = 320.0;
pub(crate) const BASE_HISTORY_HEIGHT_EXPANDED_COMPACT: f32 = 280.0;
pub(crate) const BASE_EDITOR_HEIGHT_COMPACT: f32 = 350.0;
pub(crate) const ROW_HEADER_WIDTH: f32 = 44.0;
pub(crate) const COLUMN_RESIZE_HANDLE_WIDTH: f32 = 8.0;
pub(crate) const COLUMN_RESIZE_HANDLE_HEIGHT: f32 = 16.0;
pub(crate) const DEFAULT_TABLE_QUERY_LIMIT: usize = 50;
pub(crate) const COLUMN_WIDTH_MIN: f32 = 110.0;
pub(crate) const COLUMN_WIDTH_COMPACT_SCALE: f32 = 0.78;
pub(crate) const RESULT_ROW_OVERSCAN: usize = 6;
pub(crate) const RESULT_COLUMN_OVERSCAN: usize = 2;
pub(crate) const BASE_RESULTS_VIEWPORT_HEIGHT: f32 = 640.0;
pub(crate) const DEFAULT_HISTORY_LIMIT: usize = 5;
pub(crate) const DEFAULT_TABLE_CACHE_LIMIT_ENTRIES: usize = 24;
pub(crate) const DEFAULT_TABLE_CACHE_LIMIT_MB: usize = 64;
pub(crate) const DEFAULT_TABLE_INFO_CACHE_LIMIT_ENTRIES: usize = 48;
pub(crate) const TABLE_INFO_CACHE_TTL_SECS: u64 = 300;
pub(crate) const DEFAULT_INACTIVE_TAB_RELEASE_IDLE_SECS: u64 = 90;
pub(crate) const INACTIVE_TAB_RELEASE_TICK_SECS: u64 = 3;
pub(crate) const TRANSFER_PROGRESS_MIN_STEP: f32 = 0.005;
pub(crate) const TRANSFER_PROGRESS_MIN_INTERVAL_MS: u128 = 200;
pub(crate) const EXPORT_INSERT_BATCH_SIZE: usize = 250;
pub(crate) const MAX_RECENT_CONNECTIONS: usize = 6;
pub(crate) const MYSQL_DEFAULT_PORT: &str = "3306";
pub(crate) const POSTGRES_DEFAULT_PORT: &str = "5432";

pub(crate) static ACTIVE_CONNECTION_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(CONNECT_TIMEOUT_SECS);

pub(crate) static ACTIVE_QUERY_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(0);

pub(crate) const CONNECT_TIMEOUT_SECS: u64 = 6;
pub(crate) const METADATA_TIMEOUT_SECS: u64 = 12;
pub(crate) const DEFAULT_POOL_MAX_CONNECTIONS: u32 = 2;
pub(crate) const DEFAULT_POOL_MIN_CONNECTIONS: u32 = 0;
pub(crate) const DEFAULT_POOL_IDLE_TIMEOUT_SECS: u64 = 60;
pub(crate) const DEFAULT_POOL_MAX_LIFETIME_SECS: u64 = 600;
pub(crate) const TAB_MIN_WIDTH: f32 = 120.0;
pub(crate) const TAB_MAX_WIDTH: f32 = 220.0;
pub(crate) const TAB_DRAG_THRESHOLD: f32 = 6.0;
pub(crate) const TAB_DRAG_HOLD_THRESHOLD: f32 = 2.0;
pub(crate) const TAB_DRAG_HOLD_MS: u64 = 160;
pub(crate) const TABLE_INFO_SIDEBAR_RATIO: f32 = 0.30;
pub(crate) const TABLE_INFO_SIDEBAR_MIN_WIDTH: f32 = 300.0;
pub(crate) const TABLE_INFO_SIDEBAR_MAX_WIDTH: f32 = 560.0;
pub(crate) const CHAT_SIDEBAR_MIN_WIDTH: f32 = 340.0;
pub(crate) const CHAT_SIDEBAR_MAX_WIDTH: f32 = 620.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 300.0;
pub(crate) const AI_CHAT_MAX_HISTORY: usize = 12;
pub(crate) const AI_CHAT_MAX_SESSIONS: usize = 12;
pub(crate) const AI_CHAT_RESULT_SAMPLE_ROWS: usize = 5;
pub(crate) const AI_CHAT_RESULT_CELL_MAX_CHARS: usize = 60;
pub(crate) const AI_CHAT_EDITOR_QUERY_MAX_CHARS: usize = 1200;
pub(crate) const AI_CHAT_TITLE_MAX_CHARS: usize = 34;
pub(crate) const AI_CHAT_MAX_TABLE_NAMES: usize = 60;
pub(crate) const AI_CHAT_MAX_CONTEXT_TABLES: usize = 10;
pub(crate) const AI_CHAT_SMALL_SCHEMA_TABLES: usize = 20;
pub(crate) const AI_CHAT_MAX_RELATIONS: usize = 40;
pub(crate) const AI_CHAT_MAX_SCHEMA_ROUNDS: u8 = 2;
pub(crate) const AI_CHAT_MAX_DIAGRAM_STEPS: u8 = 10;
pub(crate) const AI_CHAT_DIAGRAM_NOTE_MARK: &str = "[diagram step]";
pub(crate) const AI_CHAT_DIRECTIVE_PREFIX: &str = "@cryodb ";
pub(crate) const AI_CHAT_SAMPLE_ROWS: usize = 5;
pub(crate) const AI_CHAT_NOTE_MAX_CHARS: usize = 1500;
pub(crate) const AI_CHAT_MAX_AUTO_RETRIES: u8 = 2;
pub(crate) const RESULTS_SCROLLBAR_GUTTER: f32 = 6.0;
pub(crate) const MODAL_SCROLLBAR_GUTTER: f32 = 8.0;
pub(crate) const SAVED_DIAGRAM_PAGE_ROWS: usize = 60;
pub(crate) const FONT_PICKER_MAX_VISIBLE: usize = 120;
pub(crate) const SIDEBAR_SCROLLBAR_GUTTER: f32 = 2.0;
pub(crate) const SIDEBAR_SCROLLBAR_WIDTH: f32 = 8.0;
pub(crate) const SIDEBAR_LIST_RIGHT_PADDING: f32 = 2.0;
pub(crate) const SIDEBAR_TRIGGER_INDENT: f32 = 14.0;
pub(crate) const LOGIN_SIDEBAR_WIDTH: f32 = 240.0;
pub(crate) const UI_RADIUS: f32 = 6.0;
pub(crate) const MODERN_QUERY_STRIP_HEIGHT: f32 = 40.0;
pub(crate) const TAB_UNDERLINE_HEIGHT: f32 = 1.0;
pub(crate) const MODERN_COLUMNS_RATIO: f32 = 0.78;
pub(crate) const DEFAULT_FONT_SIZE: u32 = 16;
pub(crate) const FONT_SCALE_BASELINE: u32 = 14;
pub(crate) const MIN_FONT_SIZE: u32 = 6;
pub(crate) const DEFAULT_MODAL_BACKDROP_DIM: f32 = 0.78;
pub(crate) const DEFAULT_TAB_SIZE: u32 = 4;
pub(crate) const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 30;
pub(crate) const DEFAULT_QUERY_TIMEOUT_SECS: u64 = 60;
pub(crate) const FAVORITES_STORE_FILE: &str = "favorites.toml";
pub(crate) const SETTINGS_STORE_FILE: &str = "settings.toml";

pub(crate) const APPEARANCE_REVISION: u32 = 1;
pub(crate) const FOLDERS_STORE_FILE: &str = "folders.toml";
pub(crate) const DIAGRAMS_STORE_FILE: &str = "diagrams.toml";
pub(crate) const AI_PULSE_INTERVAL_MS: u64 = 50;
pub(crate) const AI_PULSE_PERIOD_SECS: f32 = 2.6;
pub(crate) const DIAGRAM_SEARCH_ANIM_MS: u64 = 90;
pub(crate) const LOADING_SPINNER_PERIOD_SECS: f32 = 0.8;
pub(crate) const TOAST_TIMEOUT_SECS: u64 = 5;
pub(crate) const CHANGELOG_TOAST_TIMEOUT_SECS: u64 = 12;
pub(crate) const TOAST_TICK_INTERVAL_MS: u64 = 250;
pub(crate) const TOOLTIP_TIMEOUT_MS: u64 = 2000;
pub(crate) const TOOLTIP_TICK_INTERVAL_MS: u64 = 250;
pub(crate) const READONLY_CELL_CHAR_LIMIT: usize = 160;
pub(crate) const BINARY_PREVIEW_BYTES: usize = 24;
pub(crate) const FONT_SIZES: [u32; 6] = [11, 12, 13, 14, 16, 18];
pub(crate) const DENSITY_CHOICES: [UiDensity; 2] = [UiDensity::Normal, UiDensity::Compact];
pub(crate) const RESULT_GRID_DENSITY_CHOICES: [ResultGridDensity; 2] =
    [ResultGridDensity::Comfortable, ResultGridDensity::Compact];
pub(crate) const AI_PROVIDERS: [AiProvider; 5] = [
    AiProvider::OpenAI,
    AiProvider::Anthropic,
    AiProvider::Ollama,
    AiProvider::LocalCli,
    AiProvider::Custom,
];
pub(crate) const AI_REQUEST_TIMEOUT_SECS: u64 = 60;
pub(crate) const AI_DEFAULT_TEMPERATURE: f32 = 0.7;
pub(crate) const ANTHROPIC_API_VERSION: &str = "2023-06-01";
pub(crate) const ANTHROPIC_MAX_TOKENS: u32 = 4096;
pub(crate) const AI_CLI_TIMEOUT_GRACE_SECS: u64 = 30;
pub(crate) const AI_CLI_PRESETS: [(&str, &str); 3] = [
    ("Claude", "claude -p"),
    ("OpenCode", "opencode run"),
    ("Codex", "codex exec"),
];
pub(crate) const AI_TEST_GREETING_MAX_CHARS: usize = 120;
pub(crate) const AI_TEST_OUTPUT_MAX_CHARS: usize = 700;
pub(crate) const AI_CHAT_TITLE_PROMPT: &str = "Summarize the request below as a chat title of at most five words. Reply with the title only: no quotes, no punctuation at the end, no explanation.\n\nRequest:\n";
pub(crate) const AI_TEST_PROMPT: &str = "Reply with exactly one short sentence in this form and nothing else: Hi, I'm <the name of the model answering>.";
pub(crate) const REMIX_ICON_FONT_FAMILY: &str = "remixicon";
pub(crate) const DEVICON_FONT_FAMILY: &str = "devicon";

pub(crate) const ICON_DRIVER_MYSQL: char = '\u{EAFD}';
pub(crate) const ICON_DRIVER_MARIADB: char = '\u{EAD9}';
pub(crate) const ICON_DRIVER_POSTGRESQL: char = '\u{EB79}';
pub(crate) const ICON_DRIVER_SQLITE: char = '\u{EC1E}';

pub(crate) const ICON_MODERN_DATABASE: char = '\u{EC16}';
pub(crate) const ICON_MODERN_PLUG: char = '\u{F019}';
pub(crate) const ICON_MODERN_FOLDER_CLOSED: char = '\u{ED54}';
pub(crate) const ICON_MODERN_FOLDER_OPEN: char = '\u{ED70}';
pub(crate) const ICON_MODERN_TABLE: char = '\u{F1DE}';
pub(crate) const ICON_MODERN_SEARCH: char = '\u{F0D1}';
pub(crate) const ICON_MODERN_SETTINGS: char = '\u{F0E6}';
pub(crate) const ICON_MODERN_ADD: char = '\u{EA13}';
pub(crate) const ICON_KEY_COMBO_PLUS: char = '\u{EA12}';
pub(crate) const ICON_MODERN_CLOSE: char = '\u{EB99}';
pub(crate) const ICON_MODERN_PLAY: char = '\u{F00A}';
pub(crate) const ICON_MODERN_HISTORY: char = '\u{EE17}';
pub(crate) const ICON_MODERN_STAR: char = '\u{F18B}';
pub(crate) const ICON_MODERN_TRANSFER: char = '\u{EA74}';
pub(crate) const ICON_SETTINGS: char = '\u{F0E9}';
pub(crate) const ICON_MORE_OPTIONS: char = '\u{EF78}';
pub(crate) const ICON_MORE_CONNECTIONS: char = '\u{EA12}';
pub(crate) const ICON_FILTER_LINE: char = '\u{ED24}';
pub(crate) const ICON_CHECK_FILL: char = '\u{EB7A}';
pub(crate) const ICON_CLIPBOARD_LINE: char = '\u{EB90}';
pub(crate) const ICON_COPY: char = '\u{EB87}';
pub(crate) const ICON_CLOSE_FILL: char = '\u{EB98}';
pub(crate) const ICON_CLOSE_LINE: char = '\u{EB99}';
pub(crate) const ICON_PLAY_LINE: char = '\u{F508}';
pub(crate) const ICON_STRUCTURE: char = '\u{EEBD}';
pub(crate) const ICON_BOTTOM_SIDEBAR: char = '\u{EE94}';
pub(crate) const ICON_BOTTOM_CHAT: char = '\u{EE9B}';
pub(crate) const ICON_FILE_ADD_LINE: char = '\u{F049}';
pub(crate) const ICON_DELETE_BIN_2_LINE: char = '\u{EC25}';
pub(crate) const ICON_TRASH_LINE: char = '\u{EC19}';
pub(crate) const ICON_DATABASE_LINE: char = '\u{EC15}';
pub(crate) const ICON_REFRESH_LINE: char = '\u{F064}';
pub(crate) const ICON_HEART_LINE: char = '\u{EE0C}';
pub(crate) const ICON_GOTO: char = '\u{F301}';
pub(crate) const ICON_LOGOUT_BOX_R_LINE: char = '\u{EEDE}';
pub(crate) const ICON_ARROW_DOWN_S_LINE: char = '\u{EA4E}';
pub(crate) const ICON_ARROW_LEFT_S_LINE: char = '\u{EA64}';
pub(crate) const ICON_ARROW_RIGHT_S_LINE: char = '\u{EA6E}';
pub(crate) const ICON_ARROW_UP_S_LINE: char = '\u{EA78}';
pub(crate) const ICON_UNDO: char = '\u{EA58}';
pub(crate) const ICON_REDO: char = '\u{EA5A}';
pub(crate) const ICON_ARROW_LEFT_DOUBLE_LINE: char = '\u{F2E3}';
pub(crate) const ICON_ARROW_RIGHT_DOUBLE_LINE: char = '\u{F2E5}';
pub(crate) const ICON_STAR_LINE: char = '\u{F18B}';
pub(crate) const ICON_ARROW_RIGHT_UP_LINE: char = '\u{EA70}';
pub(crate) const ICON_LOADER_LINE: char = '\u{EEC1}';
pub(crate) const ICON_RESIZE_VERTICAL: char = '\u{F326}';
pub(crate) const ICON_RESIZE_HORIZONTAL: char = '\u{F322}';
pub(crate) const ICON_TRIGGER: char = '\u{ED3C}';
pub(crate) const ICON_RELATIONS: char = '\u{EE6F}';
pub(crate) const ICON_DIAGRAM_RELATIONS: char = '\u{EEB8}';
pub(crate) const ICON_AI: char = '\u{F2EC}';
pub(crate) const ICON_CHAT_AI: char = '\u{F36A}';
pub(crate) const ICON_CHAT_CLEAR: char = '\u{EC29}';
pub(crate) const ICON_BRAND_CLAUDE: char = '\u{F5B2}';
pub(crate) const ICON_BRAND_CHATGPT: char = '\u{F34A}';
pub(crate) const BRAND_CLAUDE_COLOR: Color = color_hex(0xD97757);
pub(crate) const ICON_TAB_INACTIVE: char = '\u{F2DF}';
pub(crate) const ICON_INFO_LINE: char = '\u{EE59}';
pub(crate) const ICON_TERMINAL: char = '\u{F1F7}';
pub(crate) const ICON_PIN: char = '\u{F038}';
pub(crate) const ICON_DIAGRAM: char = '\u{F185}';
pub(crate) const ICON_AGENT: char = '\u{EF88}';
pub(crate) const ICON_AGENT_FOLLOW: char = '\u{F3B3}';
pub(crate) const DIAGRAM_AGENT_TRAVEL_SECS: f32 = 1.35;
pub(crate) const DIAGRAM_AGENT_COLOR: Color = color_hex(0x35D07F);
pub(crate) const DIAGRAM_AGENT_ZOOM_TRAVEL: f32 = 0.68;
pub(crate) const DIAGRAM_AGENT_ZOOM_WORK: f32 = 0.82;
pub(crate) const DIAGRAM_PRIMARY_KEY_COLOR: Color = color_hex(0xE0A82E);
pub(crate) const DIAGRAM_FOREIGN_KEY_COLOR: Color = color_hex(0x4C90D9);
pub(crate) const DIAGRAM_AREA_COLORS: [Color; 7] = [
    color_hex(0x4C90D9),
    color_hex(0x3FB950),
    color_hex(0xD29145),
    color_hex(0xC46BD8),
    color_hex(0x2FB2AD),
    color_hex(0xE05C6E),
    color_hex(0x8B7BE8),
];
pub(crate) const ICON_DIAGRAM_KEY: char = '\u{EE6F}';
pub(crate) const ICON_DIAGRAM_FK: char = '\u{EEAF}';
pub(crate) const ICON_DIAGRAM_NOTE: char = '\u{F19B}';
pub(crate) const ICON_DIAGRAM_LOCK: char = '\u{EECB}';
pub(crate) const ICON_DIAGRAM_LOCK_CONTENTS: char = '\u{EECE}';
pub(crate) const ICON_DIAGRAM_SAVED: char = '\u{EAE5}';
pub(crate) const ICON_TAB_QUERY: char = '\u{EBAD}';
pub(crate) const ICON_TAB_TABLE: char = '\u{F1DE}';
pub(crate) const ICON_DIAGRAM_FIT: char = '\u{EA80}';
pub(crate) const ICON_DIAGRAM_ZOOM_IN: char = '\u{F2DB}';
pub(crate) const ICON_DIAGRAM_ZOOM_OUT: char = '\u{F2DD}';
pub(crate) const ICON_DIAGRAM_GRID: char = '\u{EDDF}';
pub(crate) const ICON_DIAGRAM_FULLSCREEN: char = '\u{ED9C}';
pub(crate) const ICON_DIAGRAM_FULLSCREEN_EXIT: char = '\u{ED9A}';
pub(crate) const ICON_DIAGRAM_EXPAND: char = '\u{F4D2}';
pub(crate) const ICON_DIAGRAM_COLLAPSE: char = '\u{F4CC}';
pub(crate) const ICON_DIAGRAM_EXPORT: char = '\u{EC5A}';
pub(crate) const ICON_DIAGRAM_IMAGE: char = '\u{EE45}';
pub(crate) const ICON_DIAGRAM_CODE: char = '\u{ECD1}';
pub(crate) const ICON_DIAGRAM_APPLY: char = '\u{EB7B}';
pub(crate) const ICON_FOLDER_CLOSED: char = '\u{ED61}';
pub(crate) const ICON_FOLDER_OPEN: char = '\u{ED6F}';
pub(crate) const ICON_FOLDER_IMPORT: char = '\u{ED71}';
pub(crate) const ICON_POSTGRES_SCHEMA: char = '\u{F181}';
pub(crate) const ICON_POSTGRES_TABLE: char = '\u{F1DA}';
pub(crate) const ICON_POSTGRES_VIEW: char = '\u{ECB5}';
pub(crate) const ICON_POSTGRES_MATERIALIZED_VIEW: char = '\u{EC16}';
pub(crate) const ICON_POSTGRES_SEQUENCE: char = '\u{F074}';
pub(crate) const ICON_POSTGRES_EXTENSION: char = '\u{F452}';
pub(crate) const ICON_POSTGRES_FOREIGN_TABLE: char = '\u{EEB8}';
pub(crate) const ICON_POSTGRES_LOGIN_GROUP_ROLE: char = '\u{EDE3}';
pub(crate) const ICON_POSTGRES_VECTOR: char = '\u{F2F7}';
pub(crate) const AI_SYSTEM_PROMPT: &str = "You are a SQL query generator for MySQL. Respond ONLY with SQL queries. Do not include explanations, markdown, or code fences.";
pub(crate) const AI_INLINE_COMPLETION_SYSTEM_PROMPT: &str = "You complete SQL inside a code editor, like an IDE inline completion. Return only the exact text to insert at the cursor. Never repeat text that already appears before the cursor, never restate the whole statement, and never add explanations, markdown or code fences. Keep the completion to a single line. Return nothing at all when no useful completion exists.";
pub(crate) const AI_CHAT_SYSTEM_PROMPT: &str = "You are a SQL assistant embedded in CryoDB, a desktop database client. Answer questions about the connected database and help the user write queries. Keep answers short. When the context below already answers the question, answer it directly in prose and do not write a query. Put every SQL statement you write in a ```sql fenced block. Never claim to have run a query or to know its results unless the client gave them to you.\n\nYou can ask the client to do one thing for you. To do that, reply with ONE line and nothing else, exactly in this form:\n@cryodb schema table_one, table_two   (get the columns of tables not listed below)\n@cryodb sample table_one              (see 5 example rows, useful for value formats)\n@cryodb search term                   (find tables and columns whose name matches a term)\n@cryodb explain SELECT ...            (get the query plan for a statement)\n@cryodb open-tab                      (open your SQL in a new query tab; include the ```sql block in the same reply)\n@cryodb diagram                       (open the schema diagram and read back what is on it)\n@cryodb diagram-focus table_one       (select and centre a table on the diagram)\n@cryodb diagram-layout                (lay the diagram out again)\n@cryodb diagram-area name: t1, t2     (move exactly those tables into a named area; sending it again with a different list redraws that same area)\n@cryodb diagram-lock name             (lock an area and its contents; diagram-unlock releases it)\n@cryodb diagram-note table_one: text  (pin a note to a table)\n@cryodb diagram-add-table name: id INTEGER, label TEXT   (queue a new table as a pending schema change)\n@cryodb diagram-add-column table_one.column_name: TYPE   (queue a new column as a pending schema change)\n@cryodb diagram-link table_one.column: table_two.column  (queue a foreign key as a pending schema change)\nThe two schema commands only queue a pending change; the user reviews the diff and applies it, so say that instead of claiming the schema changed. The diagram commands change what the user sees, so use them one step at a time and say what you did afterwards. Never repeat a diagram command you already sent: it worked, so answer the user instead. Never invent column names and never ask the user to open tables: use these commands instead. Only SELECT-style statements can be run; the client blocks writes.";
pub(crate) const AI_SQL_FIX_SYSTEM_PROMPT: &str = r#"You are an advanced SQL assistant embedded inside a professional database client called CryoDB.

Your task is to analyze a failed SQL query and propose a corrected version based ONLY on the provided database schema, driver rules, and error message.

STRICT RULES:
1. NEVER invent tables, columns, or schema elements that are not present in the provided schema snapshot.
2. Respect the SQL dialect of the specified driver.
3. Prefer minimal fixes over rewriting the entire query.
4. If the query cannot be safely fixed, return a clear explanation instead of guessing.
5. Do NOT add comments inside the SQL output.
6. Preserve the original query structure as much as possible.
7. If multiple fixes are possible, choose the safest and most conventional one.

OUTPUT FORMAT (STRICT JSON ONLY):
{
"explanation": "Short technical explanation of the problem.",
"confidence": 0.0,
"fix_type": "syntax | schema | alias | join | driver | unknown",
"fixed_query": "Corrected SQL query or null if no safe fix exists",
"diff_summary": [
"Describe the minimal changes made to the query"
]
}

GUIDELINES:
- confidence must be between 0 and 1.
- Never fabricate schema data.
- Never output markdown.
- Never include extra fields.
- The response MUST be valid JSON."#;
pub(crate) const AI_FOLDER_GROUPING_SYSTEM_PROMPT: &str =
    "You group MySQL tables into folders. Respond ONLY with valid JSON.";
pub(crate) const FOLDER_GROUPING_FAST_MIN_TABLES: usize = 2;
pub(crate) const FOLDER_GROUPING_AI_TIMEOUT_SECS: u64 = 24;
pub(crate) const AI_SQL_FIX_MAX_TABLES: usize = 8;
pub(crate) const AI_SQL_FIX_MAX_COLUMNS_PER_TABLE: usize = 48;
pub(crate) const QUERY_UNDO_LIMIT: usize = 200;
pub(crate) const QUERY_SUGGESTION_LIMIT: usize = 24;
pub(crate) const QUERY_SUGGESTION_VISIBLE_ROWS: usize = 8;
pub(crate) const QUERY_INLINE_AI_MIN_CHARS: usize = 4;
pub(crate) const DEFAULT_QUERY_INLINE_SUGGESTION_DELAY_MS: u64 = 500;
pub(crate) const MIN_QUERY_INLINE_SUGGESTION_DELAY_MS: u64 = 100;
pub(crate) const MAX_QUERY_INLINE_SUGGESTION_DELAY_MS: u64 = 5_000;
pub(crate) const QUERY_INLINE_AI_MAX_CHARS: usize = 240;
pub(crate) const QUERY_INLINE_AI_MIN_INTERVAL_MS: u128 = 900;
pub(crate) const AI_INLINE_COMPLETION_TIMEOUT_SECS: u64 = 10;
pub(crate) const QUERY_INLINE_AI_CONTEXT_MAX_TABLES: usize = 3;
pub(crate) const QUERY_INLINE_AI_CONTEXT_MAX_COLUMNS: usize = 24;
pub(crate) const QUERY_INLINE_AI_CONTEXT_MAX_TABLE_NAMES: usize = 40;
pub(crate) const QUERY_INLINE_AI_CONTEXT_MAX_BYTES: usize = 2400;
pub(crate) const ADHOC_QUERY_MAX_ROWS: usize = 10_000;
pub(crate) const AI_REQUEST_MAX_RETRIES: usize = 2;
pub(crate) const OMNIBAR_RESULT_ROW_BASE_HEIGHT: f32 = 54.0;
pub(crate) const OMNIBAR_TOP_OFFSET_RATIO: f32 = 0.10;
pub(crate) const MODE_TABLE_SEARCH: OmniBarMode = OmniBarMode::TableSearch;
pub(crate) const MODE_COMMAND: OmniBarMode = OmniBarMode::Command;
