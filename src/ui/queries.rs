use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::{app::App, jira::search::ALL_ISSUES_LABEL, theme::THEME, ui::helpers};

pub(super) fn render_queries(frame: &mut Frame, area: Rect, app: &App) {
    let Some(menu) = &app.query_menu else {
        return;
    };

    let popup = helpers::popup_area(
        frame,
        helpers::centered(area, Constraint::Percentage(60), Constraint::Percentage(60)),
    );

    let block = helpers::pane_block("Saved Queries", true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let names =
        std::iter::once(ALL_ISSUES_LABEL).chain(app.queries.iter().map(|q| q.name.as_str()));
    let active_row = app.current_query.map_or(0, |i| i + 1);

    let items: Vec<ListItem> = names
        .enumerate()
        .map(|(row, name)| {
            let mark = if row == active_row { "●" } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {mark} "), Style::new().fg(THEME.green)),
                Span::styled(name, Style::new().fg(THEME.text)),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_style(Style::new().bg(THEME.highlight_bg).bold());
    let mut state = ListState::default().with_selected(Some(menu.cursor));
    frame.render_stateful_widget(list, inner, &mut state);
}
