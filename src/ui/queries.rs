use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
};

use crate::{
    app::{App, FormFocus, QueryForm},
    jira::search::ALL_ISSUES_LABEL,
    text_input::TextInput,
    theme::THEME,
    ui::{details, helpers},
};

const LABEL_WIDTH: usize = 5;
const VALUE_INDENT: usize = LABEL_WIDTH + 3;
const TITLE_INDENT: usize = 1;

pub(super) fn render_queries(frame: &mut Frame, area: Rect, app: &App) {
    let Some(menu) = &app.query_menu else {
        return;
    };

    let popup = helpers::popup_area(
        frame,
        helpers::centered(area, Constraint::Percentage(60), Constraint::Percentage(60)),
    );

    if let Some(form) = &menu.form {
        return render_form(frame, popup, form);
    }

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

fn render_form(frame: &mut Frame, popup: Rect, form: &QueryForm) {
    let title = if form.editing.is_some() {
        "Edit Query"
    } else {
        "New Query"
    };
    let block = helpers::pane_block(title, true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let name_active = form.focus == FormFocus::Name;
    let name_typing = name_active && form.is_typing();
    let label = Span::styled(
        format!(" {:<LABEL_WIDTH$}  ", "Name"),
        Style::new().fg(if name_active {
            THEME.accent
        } else {
            THEME.text
        }),
    );
    let name_lines = if name_typing {
        details::edit_lines(label, VALUE_INDENT, &form.name, edit_width(inner.width))
    } else {
        vec![Line::from(vec![label, value_span(&form.name)])]
    };

    let error_rows = u16::from(form.error.is_some());
    let room = inner.height.saturating_sub(error_rows + 2).max(1);
    let [name_area, jql_area, error_area] = Layout::vertical([
        Constraint::Length((name_lines.len() as u16).min(room)),
        Constraint::Fill(1),
        Constraint::Length(error_rows),
    ])
    .areas(inner);

    let mut name = ListItem::new(name_lines);
    if name_typing {
        name = name.style(Style::new().bg(THEME.highlight_bg));
    } else if name_active {
        name = name.style(Style::new().bg(THEME.highlight_bg).bold());
    }
    frame.render_widget(List::new([name]), name_area);

    render_jql(frame, jql_area, form);

    if let Some(error) = &form.error {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {error}"),
                Style::new().fg(THEME.red),
            )),
            error_area,
        );
    }
}

fn render_jql(frame: &mut Frame, area: Rect, form: &QueryForm) {
    let active = form.focus == FormFocus::Jql;
    let style = Style::new().fg(if active { THEME.accent } else { THEME.border });
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(style)
        .title(Span::styled(" Query ", style))
        .padding(Padding::left(TITLE_INDENT as u16));

    let width = (area.width as usize)
        .saturating_sub(TITLE_INDENT + 1)
        .max(8);
    let body = if active && form.is_typing() {
        Paragraph::new(details::edit_lines(Span::raw(""), 0, &form.jql, width))
            .style(style)
            .block(block)
    } else if form.jql.buffer.is_empty() {
        Paragraph::new(Line::from(value_span(&form.jql))).block(block)
    } else {
        Paragraph::new(form.jql.buffer.clone())
            .wrap(Wrap { trim: false })
            .block(block)
    };
    frame.render_widget(body, area);
}

fn value_span(input: &TextInput) -> Span<'static> {
    if input.buffer.is_empty() {
        Span::styled("None", Style::new().fg(THEME.text_muted))
    } else {
        Span::raw(input.buffer.clone())
    }
}

fn edit_width(area_width: u16) -> usize {
    (area_width as usize)
        .saturating_sub(VALUE_INDENT + 1)
        .max(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::App;

    // the rows inside the popup's left and right borders, leading spaces kept
    fn rows(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| render_queries(frame, frame.area(), app))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .filter_map(|row| {
                let start = row.find('\u{2502}')? + '\u{2502}'.len_utf8();
                let end = row.rfind('\u{2502}')?;
                Some(row[start..end].trim_end().to_string())
            })
            .collect()
    }

    fn indent(row: &str) -> usize {
        row.len() - row.trim_start().len()
    }

    fn form_of(app: &mut App) -> &mut QueryForm {
        app.query_form_mut().expect("the form is open")
    }

    #[test]
    fn the_query_lines_up_under_its_title() {
        let mut app = App::default();
        app.open_query_menu();
        app.start_query_form(false);
        app.set_form("Maple", "project = MAPLE AND status != Done");

        // a labelled field row, then the query under its own rule
        let resting = rows(&app, 80, 24);
        assert_eq!(resting[0], " Name   Maple", "{resting:?}");
        let rule = &resting[1];
        assert!(rule.starts_with(" Query \u{2500}"), "{resting:?}");
        assert_eq!(
            indent(&resting[2]),
            indent(rule),
            "the query starts under the title: {resting:?}"
        );

        // and it does not shift sideways once it is being typed in
        form_of(&mut app).toggle_focus();
        form_of(&mut app).start_edit();
        let typing = rows(&app, 80, 24);
        assert_eq!(
            typing[2].trim_end_matches('\u{2588}'),
            resting[2],
            "only the cursor is new: {typing:?}"
        );
    }

    #[test]
    fn the_form_grows_for_a_wrapped_name_without_losing_the_query() {
        let mut app = App::default();
        app.open_query_menu();
        app.start_query_form(false);
        app.set_form(&"maple ".repeat(20), "project = MAPLE");
        form_of(&mut app).start_edit();
        form_of(&mut app).error = Some("Name cannot be empty".into());

        let wrapped = rows(&app, 80, 24);
        assert!(wrapped[0].starts_with(" Name   maple"), "{wrapped:?}");
        assert_eq!(
            indent(&wrapped[1]),
            VALUE_INDENT,
            "the name wraps under its label: {wrapped:?}"
        );
        let rule = wrapped
            .iter()
            .position(|row| row.starts_with(" Query \u{2500}"))
            .expect("the query keeps its rule");
        assert!(
            wrapped.len() > rule + 2,
            "and a line to type in, plus the error row: {wrapped:?}"
        );
        assert_eq!(
            wrapped.last().map(String::as_str),
            Some(" Name cannot be empty")
        );

        // in a window too short for the wrapped name, the name gives way first
        let short = rows(&app, 80, 12);
        assert!(
            short.iter().any(|row| row.starts_with(" Query \u{2500}")),
            "{short:?}"
        );
    }
}
