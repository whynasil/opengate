use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Style, Stylize},
};
use tokio::sync::mpsc;

use crate::config;
use crate::gateway::ServerMessage;

use super::chat_view::ChatView;
use super::input::InputArea;
use super::status;
use super::theme::Theme;
use super::ws_client::{WsCommand, WsEvent, ws_client_task};

pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<Utc>,
}

#[derive(PartialEq)]
pub enum Focus {
    Chat,
    Input,
    Sidebar,
}

pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

pub struct TuiApp {
    pub theme: Theme,
    pub messages: Vec<ChatMessage>,
    pub input: InputArea,
    pub chat: ChatView,
    pub session_id: String,
    pub connection: ConnectionState,
    pub focus: Focus,
    pub channels: Vec<String>,
    pub current_channel: usize,
    pub tool_count: usize,
    pub should_quit: bool,
    pub config: Arc<config::Config>,
    pub ws_cmd_tx: mpsc::Sender<WsCommand>,
    pub ws_event_rx: mpsc::Receiver<WsEvent>,
}

impl TuiApp {
    fn new(
        config: Arc<config::Config>,
        ws_cmd_tx: mpsc::Sender<WsCommand>,
        ws_event_rx: mpsc::Receiver<WsEvent>,
    ) -> Self {
        Self {
            theme: Theme::default(),
            messages: Vec::new(),
            input: InputArea::new(),
            chat: ChatView::new(),
            session_id: String::new(),
            connection: ConnectionState::Connecting,
            focus: Focus::Input,
            channels: vec!["general".to_string()],
            current_channel: 0,
            tool_count: 0,
            should_quit: false,
            config,
            ws_cmd_tx,
            ws_event_rx,
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Input => Focus::Chat,
                    Focus::Chat => Focus::Sidebar,
                    Focus::Sidebar => Focus::Input,
                };
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.send_message();
            }
            KeyCode::PageUp => {
                self.chat.list_state.scroll_up_by(5);
            }
            KeyCode::PageDown => {
                self.chat.list_state.scroll_down_by(5);
            }
            _ => {
                if self.focus == Focus::Input {
                    self.input.handle_key_event(key);
                }
            }
        }
    }

    fn send_message(&mut self) {
        let content = self.input.get_content();
        if content.trim().is_empty() {
            return;
        }
        self.input.clear();

        let msg = ChatMessage {
            role: "user".to_string(),
            content: content.clone(),
            timestamp: Utc::now(),
        };
        self.messages.push(msg);

        let session = self.session_id.clone();
        let _ = self.ws_cmd_tx.try_send(WsCommand::SendChat { message: content, session });
    }

    fn handle_ws_event(&mut self, event: WsEvent) {
        match event {
            WsEvent::Connected => {
                self.connection = ConnectionState::Connected;
            }
            WsEvent::Disconnected => {
                self.connection = ConnectionState::Disconnected;
            }
            WsEvent::Error(e) => {
                self.connection = ConnectionState::Disconnected;
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Error: {e}"),
                    timestamp: Utc::now(),
                });
            }
            WsEvent::MessageReceived(server_msg) => match server_msg {
                ServerMessage::AuthOk => {}
                ServerMessage::ChatResponse { content, session } => {
                    if self.session_id.is_empty() {
                        self.session_id = session.clone();
                    }
                    self.messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content,
                        timestamp: Utc::now(),
                    });
                }
                ServerMessage::Error { code, message } => {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("[{code}] {message}"),
                        timestamp: Utc::now(),
                    });
                }
                ServerMessage::Pong => {}
            },
        }
    }

    fn render(&mut self, f: &mut ratatui::Frame) {
        let area = f.area();

        let main = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(5)])
            .split(area);

        let sidebar_chat = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(15), Constraint::Min(0)])
            .split(main[1]);

        status::render_status_bar(
            f,
            main[0],
            &self.connection,
            &self.session_id,
            self.tool_count,
            &self.theme,
            &self.config,
        );

        let sidebar_focused = self.focus == Focus::Sidebar;
        let sidebar_items: Vec<ratatui::widgets::ListItem> = self
            .channels
            .iter()
            .enumerate()
            .map(|(i, ch)| {
                let prefix = if i == self.current_channel { "▸ " } else { "  " };
                let style = if i == self.current_channel {
                    Style::default().fg(self.theme.accent).bold()
                } else {
                    Style::default().fg(self.theme.text)
                };
                ratatui::widgets::ListItem::new(format!("{prefix}{ch}")).style(style)
            })
            .collect();

        let sidebar_border = if sidebar_focused {
            Style::default().fg(self.theme.focus_border)
        } else {
            Style::default().fg(self.theme.border)
        };

        let sidebar_block = ratatui::widgets::Block::bordered()
            .title(" Channels ")
            .border_style(sidebar_border)
            .style(Style::default().bg(self.theme.bg));

        let sidebar_list = ratatui::widgets::List::new(sidebar_items)
            .block(sidebar_block)
            .style(Style::default().bg(self.theme.bg));
        f.render_widget(sidebar_list, sidebar_chat[0]);

        self.chat.render(
            f,
            sidebar_chat[1],
            &self.messages,
            &self.theme,
            self.focus == Focus::Chat,
        );

        self.input.render(f, main[2], &self.theme, self.focus == Focus::Input);
    }

    pub async fn run(config: Arc<config::Config>) -> anyhow::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let (ws_cmd_tx, ws_cmd_rx) = mpsc::channel::<WsCommand>(256);
        let (ws_event_tx, ws_event_rx) = mpsc::channel::<WsEvent>(256);

        let ws_url = format!("ws://{}:{}/ws", config.gateway.host, config.gateway.port);
        let auth_token = config.gateway.auth_token.clone();
        tokio::spawn(ws_client_task(ws_url, auth_token, ws_cmd_rx, ws_event_tx));

        let mut app = TuiApp::new(config, ws_cmd_tx, ws_event_rx);

        loop {
            if event::poll(Duration::from_millis(16))?
                && let Event::Key(key) = event::read()?
            {
                app.handle_key(key);
            }

            while let Ok(event) = app.ws_event_rx.try_recv() {
                app.handle_ws_event(event);
            }

            if app.should_quit {
                break;
            }

            terminal.draw(|f| app.render(f))?;
        }

        terminal.clear()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;

        Ok(())
    }
}
