pub(crate) use crate::app::types::DiagramAgentAction;
use iced::widget::text_editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatRole {
    User,
    Assistant,
    Note,
}

pub(crate) struct ChatSession {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatSessionOption {
    pub(crate) id: u64,
    pub(crate) label: String,
}

impl std::fmt::Display for ChatSessionOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatDirective {
    Schema(Vec<String>),
    Sample(String),
    Explain(String),
    Search(String),
    OpenTab,
    Diagram(DiagramAgentAction),
    Malformed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatBlockKind {
    Prose,
    Code,
}

pub(crate) struct ChatBlock {
    pub(crate) kind: ChatBlockKind,
    pub(crate) badge: Option<String>,
    pub(crate) text: String,
    pub(crate) editor: text_editor::Content,
    pub(crate) code: Option<iced_code_editor::CodeEditor>,
}

pub(crate) struct ChatMessage {
    pub(crate) role: ChatRole,
    pub(crate) content: String,
    pub(crate) summary: Option<String>,
    pub(crate) blocks: Vec<ChatBlock>,
}

impl ChatMessage {
    pub(crate) fn new(role: ChatRole, content: String) -> Self {
        let blocks = Self::parse_blocks(&content);
        Self {
            role,
            content,
            summary: None,
            blocks,
        }
    }

    pub(crate) fn note(summary: String, content: String) -> Self {
        Self {
            role: ChatRole::Note,
            content,
            summary: Some(summary),
            blocks: Vec::new(),
        }
    }

    fn prose_block(raw: &str) -> Option<ChatBlock> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let (badge, body) = match trimmed.strip_prefix("**") {
            Some(rest) => match rest.split_once(":**") {
                Some((label, tail)) if !label.contains('\n') && label.len() <= 24 => {
                    (Some(label.trim().to_string()), tail.trim_start())
                }
                _ => (None, trimmed),
            },
            None => (None, trimmed),
        };

        let text = body.replace("**", "");
        Some(ChatBlock {
            kind: ChatBlockKind::Prose,
            badge,
            editor: text_editor::Content::with_text(&text),
            code: None,
            text,
        })
    }

    fn code_block(raw: &str) -> Option<ChatBlock> {
        let text = raw.trim().to_string();
        if text.is_empty() {
            return None;
        }
        Some(ChatBlock {
            kind: ChatBlockKind::Code,
            badge: None,
            editor: text_editor::Content::new(),
            code: Some(crate::ui::widgets::code_snippet::unstyled(&text)),
            text,
        })
    }

    pub(crate) fn parse_blocks(content: &str) -> Vec<ChatBlock> {
        let mut blocks = Vec::new();
        let mut rest = content;

        while let Some(start) = rest.find("```") {
            let (before, after_fence) = rest.split_at(start);
            let after_fence = &after_fence[3..];
            let Some(newline) = after_fence.find('\n') else {
                break;
            };
            let Some(end) = after_fence[newline + 1..].find("```") else {
                break;
            };

            if let Some(block) = Self::prose_block(before) {
                blocks.push(block);
            }
            let body = &after_fence[newline + 1..newline + 1 + end];
            if let Some(block) = Self::code_block(body) {
                blocks.push(block);
            }
            rest = &after_fence[newline + 1 + end + 3..];
        }

        if let Some(block) = Self::prose_block(rest) {
            blocks.push(block);
        }
        blocks
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiModalTarget {
    QueryEditor,
    PostgresRoleSql,
}
