use std::mem;

use chrono::{Datelike, Months, NaiveDate};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use crate::{app::App, theme::THEME, ui::helpers};

pub(super) fn render_date_picker(frame: &mut Frame, area: Rect, today: NaiveDate, app: &App) {
    let Some(picker) = &app.date_picker else {
        return;
    };

    let popup = helpers::popup_area(
        frame,
        helpers::centered(area, Constraint::Length(43), Constraint::Length(15)),
    );

    let block = helpers::pane_block(picker.field.label(), true).padding(Padding::new(2, 2, 1, 1));

    let first = picker.date.with_day(1).unwrap();
    let offset = first.weekday().num_days_from_monday() as usize;
    let days_in_month = first
        .checked_add_months(Months::new(1))
        .and_then(|next| next.pred_opt())
        .map(|last| last.day())
        .unwrap_or(31);

    let header = Line::from(picker.date.format("◀  %B %Y  ▶").to_string()).centered();
    let weekdays =
        Line::from("  Mo   Tu   We   Th   Fr   Sa   Su ").style(Style::new().fg(THEME.text_muted));

    let mut lines = vec![header, Line::default(), weekdays];
    let mut week: Vec<Span> = vec![Span::raw("     ".repeat(offset))];

    for day in 1..=days_in_month {
        let date = first.with_day(day).unwrap();
        let style = if date == picker.date {
            Style::new().fg(THEME.base).bg(THEME.accent).bold()
        } else if date == today {
            Style::new().fg(THEME.accent).underlined()
        } else {
            Style::new().fg(THEME.text)
        };
        week.push(Span::raw(" "));
        week.push(Span::styled(format!(" {day:>2} "), style));

        if (offset + day as usize).is_multiple_of(7) {
            lines.push(Line::from(mem::take(&mut week)));
        }
    }
    if !week.is_empty() {
        lines.push(Line::from(week));
    }

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
