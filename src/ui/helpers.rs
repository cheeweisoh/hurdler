use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear},
};

use crate::{domain::issue::DueState, text_input::TextInput, theme::THEME};

pub(super) const SEARCH_ICON: &str = "\u{f002}";

pub(super) fn spinner() -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    FRAMES[(millis / 100) as usize % FRAMES.len()]
}

pub(super) fn input_line(label: Span<'static>, input: &TextInput) -> Line<'static> {
    let cursor_style = Style::new()
        .fg(THEME.base)
        .bg(THEME.text)
        .add_modifier(Modifier::SLOW_BLINK);

    let chars: Vec<char> = input.buffer.chars().collect();
    let (before, after) = chars.split_at(input.cursor.min(chars.len()));

    let mut spans = vec![label, Span::raw(before.iter().collect::<String>())];
    match after.split_first() {
        Some((c, rest)) => {
            spans.push(Span::styled(c.to_string(), cursor_style));
            spans.push(Span::raw(rest.iter().collect::<String>()));
        }
        None => spans.push(Span::styled(" ", cursor_style)),
    }
    Line::from(spans)
}

pub(super) fn pane_block(title: &str, focused: bool) -> Block<'static> {
    let color = if focused {
        THEME.border_focused
    } else {
        THEME.border
    };
    Block::bordered()
        .border_type(BorderType::Plain)
        .border_style(Style::new().fg(color))
        .title(format!(" {title} "))
}

pub(super) fn status_span(status: &str, status_category: &str) -> Span<'static> {
    const BADGE_WIDTH: usize = 11;

    let color = match status_category {
        "new" => THEME.blue,
        "indeterminate" => THEME.yellow,
        "done" => THEME.green,
        _ => THEME.text_muted,
    };

    Span::styled(
        format!(" {:^BADGE_WIDTH$.BADGE_WIDTH$} ", status.to_uppercase()),
        Style::new().fg(THEME.base).bg(color).bold(),
    )
}

pub(super) fn due_style(state: Option<DueState>) -> Style {
    match state {
        Some(DueState::Overdue) => Style::new().fg(THEME.red).bold(),
        Some(DueState::DueSoon) => Style::new().fg(THEME.yellow),
        Some(DueState::OnTrack) | None => Style::new().fg(THEME.text),
    }
}

pub(super) fn centered(area: Rect, width: Constraint, height: Constraint) -> Rect {
    let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);
    area
}

pub(super) fn clear_area(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
    frame.render_widget(Block::new().style(Style::new().bg(THEME.base)), area);
}

pub(super) fn popup_area(frame: &mut Frame, outer: Rect) -> Rect {
    clear_area(frame, outer);
    outer.inner(Margin::new(1, 1))
}
