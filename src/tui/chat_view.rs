use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::Span,
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use super::app::ChatMessage;
use super::theme::Theme;

pub struct ChatView {
    pub list_state: ListState,
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatView {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        messages: &[ChatMessage],
        theme: &Theme,
        is_focused: bool,
    ) {
        let border_style = if is_focused {
            Style::default().fg(theme.focus_border)
        } else {
            Style::default().fg(theme.border)
        };

        let items: Vec<ListItem> = messages
            .iter()
            .map(|msg| {
                let label = match msg.role.as_str() {
                    "user" => Span::styled("You", Style::default().fg(theme.user_msg).bold()),
                    "assistant" => {
                        Span::styled("Assistant", Style::default().fg(theme.assistant_msg).bold())
                    }
                    "system" => Span::styled("System", Style::default().fg(theme.system_msg).bold()),
                    "tool" => Span::styled("Tool", Style::default().fg(theme.tool_msg).bold()),
                    other => Span::styled(other, Style::default().fg(theme.dim_text)),
                };

                let content = Span::styled(&msg.content, Style::default().fg(theme.text));
                let timestamp = Span::styled(
                    format!("  {}", msg.timestamp.format("%H:%M:%S")),
                    Style::default().fg(theme.dim_text),
                );

                ListItem::new(vec![
                    ratatui::text::Line::from(vec![label, timestamp]),
                    ratatui::text::Line::from(content),
                    ratatui::text::Line::from(""),
                ])
            })
            .collect();

        let block = Block::bordered()
            .title(" Chat ")
            .border_style(border_style)
            .style(Style::default().bg(theme.bg));

        if items.is_empty() {
            let empty = List::new(vec![ListItem::new("No messages yet")])
                .block(block)
                .style(Style::default().bg(theme.bg).fg(theme.dim_text));
            f.render_widget(empty, area);
            return;
        }

        self.list_state.select(Some(messages.len().saturating_sub(1)));

        let list = List::new(items)
            .block(block)
            .style(Style::default().bg(theme.bg))
            .highlight_style(Style::default().bg(theme.accent).fg(theme.bg));

        f.render_stateful_widget(list, area, &mut self.list_state);
    }
}
