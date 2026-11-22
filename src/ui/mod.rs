mod confirm;
mod date_picker;
mod details;
mod help;
mod helpers;
mod issues;
mod picker;
pub mod setup;

use chrono::{Local, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, Notice, Search, Tab, View};
use crate::theme::THEME;

pub fn render(frame: &mut Frame, app: &mut App) {
    let now = Utc::now();
    let today = now.with_timezone(&Local).date_naive();

    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(frame.area());
    render_header(frame, header_area, app);

    let [sidebar_area, main_area] =
        Layout::horizontal([Constraint::Length(16), Constraint::Fill(1)]).areas(body_area);
    render_sidebar(frame, sidebar_area, app);

    match app.active_tab {
        Tab::Issues => match app.view {
            View::List => {
                let [content_area, details_area] =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Percentage(40)])
                        .areas(main_area);
                render_content(frame, content_area, app);
                details::render_details(frame, details_area, now, app);
            }
            View::Inspector => details::render_details(frame, main_area, now, app),
        },
    }

    picker::render_picker(frame, main_area, app);
    date_picker::render_date_picker(frame, main_area, today, app);
    confirm::render_confirm(frame, main_area, app);
    help::render_help(frame, frame.area(), app);
}

fn search_line(search: &Search) -> Line<'static> {
    let icon = Span::styled(
        format!(" {} ", helpers::SEARCH_ICON),
        Style::new().fg(THEME.accent),
    );
    if search.editing {
        helpers::input_line(icon, &search.input)
    } else {
        Line::from(vec![
            icon,
            Span::styled(
                search.input.buffer.clone(),
                Style::new().fg(THEME.text_muted),
            ),
        ])
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let area = match &app.search {
        Some(search) => {
            let line = search_line(search);
            let [search_area, rest] =
                Layout::horizontal([Constraint::Length(line.width() as u16), Constraint::Fill(1)])
                    .areas(area);
            frame.render_widget(line, search_area);
            rest
        }
        None => area,
    };

    let (text, color) = match &app.notice {
        Some((Notice::Info(message), _)) => (message.as_str(), THEME.green),
        Some((Notice::Error(message), _)) => (message.as_str(), THEME.red),
        None => (app.display_name(), THEME.text_muted),
    };
    let right = Line::from(format!(" {text} "))
        .style(Style::new().fg(color))
        .right_aligned();
    frame.render_widget(right, area);
}

fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = Tab::ALL
        .iter()
        .map(|&tab| {
            let title = format!(" {} ", tab.title().to_uppercase());
            if tab == app.active_tab {
                ListItem::new(title).style(Style::new().bg(THEME.accent).fg(THEME.base).bold())
            } else {
                ListItem::new(title).style(Style::new().fg(THEME.text_muted))
            }
        })
        .collect();

    let list = List::new(items).block(helpers::pane_block("Views", false));
    frame.render_widget(list, area);
}

fn render_content(frame: &mut Frame, area: Rect, app: &mut App) {
    match app.active_tab {
        Tab::Issues => {
            if let Some(error) = &app.error {
                let body = Paragraph::new(error.as_str())
                    .style(Style::new().fg(THEME.red))
                    .wrap(Wrap { trim: false })
                    .block(helpers::pane_block("Issues", true));
                frame.render_widget(body, area);
            } else {
                issues::render_issues_table(frame, area, app);
            }
        }
    }
}
