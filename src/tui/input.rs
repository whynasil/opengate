use crossterm::event::KeyEvent;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::Block,
    Frame,
};
use tui_textarea::TextArea;

use super::theme::Theme;

pub struct InputArea {
    pub textarea: TextArea<'static>,
}

impl Default for InputArea {
    fn default() -> Self {
        Self::new()
    }
}

impl InputArea {
    pub fn new() -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_style(Style::default().bg(Theme::default().accent));
        textarea.set_placeholder_text("Type a message...");
        Self { textarea }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        self.textarea.input(key)
    }

    pub fn get_content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    pub fn clear(&mut self) {
        self.textarea.select_all();
        self.textarea.cut();
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme, is_focused: bool) {
        let border_style = if is_focused {
            Style::default().fg(theme.focus_border)
        } else {
            Style::default().fg(theme.border)
        };

        let block = Block::bordered()
            .title(" Input ")
            .border_style(border_style)
            .style(Style::default().bg(theme.bg));

        let mut textarea = self.textarea.clone();
        textarea.set_block(block);
        f.render_widget(&textarea, area);
    }
}
