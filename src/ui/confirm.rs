use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use crate::{app::App, theme::THEME, ui::helpers};

fn button(text: &str, focused: bool, accent: Color) -> Span<'static> {
    let label = format!("  {text}  ");
    if focused {
        Span::styled(label, Style::new().fg(THEME.base).bg(accent).bold())
    } else {
        Span::styled(label, Style::new().fg(THEME.text_muted))
    }
}

pub(super) fn render_confirm(frame: &mut Frame, area: Rect, app: &App) {
    let Some(dialog) = &app.confirm_delete else {
        return;
    };

    let popup = helpers::popup_area(
        frame,
        helpers::centered(area, Constraint::Length(52), Constraint::Length(11)),
    );

    let block = helpers::pane_block("Delete issue", true).padding(Padding::new(2, 2, 1, 1));

    let lines = vec![
        Line::from(vec![
            Span::styled("Delete ", Style::new().fg(THEME.text)),
            Span::styled(dialog.key.clone(), Style::new().fg(THEME.accent).bold()),
            Span::styled("?", Style::new().fg(THEME.text)),
        ])
        .centered(),
        Line::default(),
        Line::styled("This cannot be undone.", Style::new().fg(THEME.red).bold()).centered(),
        Line::default(),
        Line::from(vec![
            button("Confirm", dialog.confirm_focused, THEME.red),
            Span::raw("    "),
            button("Cancel", !dialog.confirm_focused, THEME.accent),
        ])
        .centered(),
    ];

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
