use ratatui::{
    Frame,
    layout::Rect,
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph},
};

use crate::config;

use super::app::ConnectionState;
use super::theme::Theme;

pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    connection: &ConnectionState,
    session_id: &str,
    tool_count: usize,
    theme: &Theme,
    config: &config::Config,
) {
    let (status_text, _status_style) = match connection {
        ConnectionState::Connected => {
            ("● Connected", Style::default().fg(theme.status_connected).bold())
        }
        ConnectionState::Connecting => {
            ("◐ Connecting...", Style::default().fg(theme.accent).bold())
        }
        ConnectionState::Disconnected => {
            ("○ Disconnected", Style::default().fg(theme.status_disconnected).bold())
        }
    };

    let addr = format!("{}:{}", config.gateway.host, config.gateway.port);
    let session_label = if session_id.is_empty() {
        "no session".to_string()
    } else {
        format!("session: {}", &session_id[..session_id.len().min(8)])
    };

    let line = Line::from(vec![
        status_text.into(),
        format!("  {addr}").fg(theme.dim_text),
        format!("  {session_label}").fg(theme.dim_text),
        format!("  tools: {tool_count}").fg(theme.dim_text),
    ]);

    let block = Block::bordered()
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg));

    let paragraph = Paragraph::new(line).block(block).style(Style::default().bg(theme.bg));

    f.render_widget(paragraph, area);
}
