use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Cell, Row, Table},
};

use crate::{app::App, theme::THEME, ui::helpers};

pub(super) fn render_issues_table(frame: &mut Frame, area: Rect, app: &mut App) {
    let frame_char = helpers::spinner();
    let count = app.issues.items.len();
    let more = if app.next_page_token.is_some() {
        "+"
    } else {
        ""
    };

    let searching = app
        .search_query()
        .map(|query| format!(" · search \"{query}\""));
    let title = match (app.loading, app.loading_more) {
        (true, _) if app.search_editing() || searching.is_some() => {
            format!("{frame_char} Issues · searching...")
        }
        (true, _) => format!("{frame_char} Issues · Loading..."),
        (_, true) => format!(
            "Issues ({count}{more}){} · {frame_char} loading more...",
            searching.unwrap_or_default()
        ),
        _ => format!("Issues ({count}{more}){}", searching.unwrap_or_default()),
    };

    let header = Row::new(["Key", "Summary", "Status", "Assignee"])
        .style(Style::new().fg(THEME.text_muted).bold());

    let rows = app.issues.items.iter().map(|issue| {
        Row::new(vec![
            Cell::from(issue.key.as_str()).style(Style::new().fg(THEME.accent).bold()),
            Cell::from(issue.summary.as_str()),
            Cell::from(helpers::status_span(
                issue.status.as_str(),
                issue.status_category.as_str(),
            )),
            Cell::from(issue.assignee_name()),
        ])
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Fill(2),
        Constraint::Length(13),
        Constraint::Fill(1),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(helpers::pane_block(&title, true))
        .row_highlight_style(Style::new().bg(THEME.text_muted).bold())
        .highlight_symbol("> ");

    frame.render_stateful_widget(table, area, &mut app.issues.state);
}
