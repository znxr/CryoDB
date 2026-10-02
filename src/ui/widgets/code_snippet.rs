use iced::{Color, Font, Theme};
use iced_code_editor::{CodeEditor, Message};

pub(crate) fn style(theme: &Theme) -> iced_code_editor::theme::Style {
    let mut style = iced_code_editor::theme::from_iced_theme(theme);
    style.gutter_background = Color::TRANSPARENT;
    style
}

pub(crate) fn restyle(editor: &mut CodeEditor, theme: &Theme, font: Font, size: f32) {
    editor.set_theme(style(theme));
    editor.set_font(font);
    editor.set_font_size(size, false);
    editor.set_line_height(size * 1.3);
}

pub(crate) fn unstyled(text: &str) -> CodeEditor {
    let mut editor = CodeEditor::new(text, "sql");
    editor.set_line_numbers_enabled(false);
    editor.set_folding_enabled(false);
    editor.set_command_palette_enabled(false);
    editor.set_sticky_scroll_enabled(false);
    editor.set_show_whitespace(false);
    editor.set_show_indent_guides(false);
    editor.set_wrap_enabled(false);
    editor
}

pub(crate) fn new(text: &str, theme: &Theme, font: Font, size: f32) -> CodeEditor {
    let mut editor = unstyled(text);
    restyle(&mut editor, theme, font, size);
    editor
}

pub(crate) fn is_edit(message: &Message) -> bool {
    matches!(
        message,
        Message::CharacterInput(_)
            | Message::VimKey(_)
            | Message::Backspace
            | Message::Delete
            | Message::Enter
            | Message::Tab
            | Message::Cut
            | Message::Paste(_)
            | Message::DeleteSelection
            | Message::Undo
            | Message::Redo
            | Message::ReplaceNext
            | Message::ReplaceAll
            | Message::WriteRequested
    )
}
