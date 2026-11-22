use ratatui::style::Color;

pub struct Theme {
    pub base: Color,
    pub text: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focused: Color,
    pub highlight_bg: Color,
    pub blue: Color,
    pub yellow: Color,
    pub green: Color,
    pub red: Color,
}

pub const THEME: Theme = Theme {
    base: Color::Rgb(30, 30, 46),
    text: Color::Rgb(205, 214, 244),
    text_muted: Color::Rgb(108, 112, 134),
    accent: Color::Rgb(137, 180, 250),
    border: Color::Rgb(69, 71, 90),
    border_focused: Color::Rgb(137, 180, 250),
    highlight_bg: Color::Rgb(49, 50, 68),
    blue: Color::Rgb(137, 180, 250),
    yellow: Color::Rgb(249, 226, 175),
    green: Color::Rgb(166, 227, 161),
    red: Color::Rgb(243, 139, 168),
};
