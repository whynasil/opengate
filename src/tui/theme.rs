use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub accent: Color,
    pub user_msg: Color,
    pub assistant_msg: Color,
    pub system_msg: Color,
    pub tool_msg: Color,
    pub text: Color,
    pub dim_text: Color,
    pub border: Color,
    pub focus_border: Color,
    pub status_connected: Color,
    pub status_disconnected: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(30, 30, 30),
            accent: Color::Rgb(0, 255, 255),
            user_msg: Color::Rgb(0, 200, 100),
            assistant_msg: Color::Rgb(200, 80, 200),
            system_msg: Color::Rgb(200, 200, 0),
            tool_msg: Color::Rgb(150, 150, 150),
            text: Color::Rgb(220, 220, 220),
            dim_text: Color::Rgb(120, 120, 120),
            border: Color::Rgb(80, 80, 80),
            focus_border: Color::Rgb(0, 255, 255),
            status_connected: Color::Rgb(0, 200, 100),
            status_disconnected: Color::Rgb(200, 50, 50),
        }
    }
}
