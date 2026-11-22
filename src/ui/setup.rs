use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, Padding},
};

use crate::{
    app::Notice,
    setup::{Setup, SetupField},
    theme::THEME,
    ui::helpers,
};

fn label_width() -> usize {
    SetupField::ALL
        .iter()
        .map(|field| field.label().chars().count())
        .max()
        .unwrap_or(0)
}

pub fn render(frame: &mut Frame, setup: &Setup) {
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(frame.area());
    render_header(frame, header_area, setup);

    let rows = SetupField::ALL.len() as u16;
    let width = (body_area.width * 70 / 100).clamp(60, body_area.width.max(1));
    let height = (rows + 6).min(body_area.height);
    let card = helpers::centered(
        body_area,
        Constraint::Length(width),
        Constraint::Length(height),
    );

    let block = helpers::pane_block("Setup", true).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let [fields_area, hint_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(inner);
    render_fields(frame, fields_area, setup);
    frame.render_widget(hint_line(setup), hint_area);
}

fn render_header(frame: &mut Frame, area: Rect, setup: &Setup) {
    let left =
        Line::from(format!(" {}", setup.path.display())).style(Style::new().fg(THEME.text_muted));
    frame.render_widget(left, area);

    let (text, color) = match &setup.notice {
        Some(Notice::Info(message)) => (message.as_str(), THEME.green),
        Some(Notice::Error(message)) => (message.as_str(), THEME.red),
        None => ("hurdler setup", THEME.text_muted),
    };

    let text = if setup.testing {
        format!("{} {text}", helpers::spinner())
    } else {
        text.to_string()
    };

    let right = Line::from(format!(" {text} "))
        .style(Style::new().fg(color))
        .right_aligned();
    frame.render_widget(right, area);
}

fn render_fields(frame: &mut Frame, area: Rect, setup: &Setup) {
    let items: Vec<ListItem> = SetupField::ALL
        .iter()
        .map(|&field| {
            let active = setup.selected == field;
            let is_editing = active && setup.editing.is_some();
            let label = Span::styled(
                format!(" {:<width$}  ", field.label(), width = label_width()),
                Style::new().fg(if active { THEME.accent } else { THEME.text }),
            );

            let line = match &setup.editing {
                Some(input) if active => helpers::input_line(label, input),
                _ => Line::from(vec![label, value_span(field, setup.value(field))]),
            };

            let item = ListItem::new(line);
            if is_editing {
                item.style(Style::new().bg(THEME.highlight_bg))
            } else if active {
                item.style(Style::new().bg(THEME.highlight_bg).bold())
            } else {
                item
            }
        })
        .collect();

    frame.render_widget(List::new(items), area);
}

fn value_span(field: SetupField, value: String) -> Span<'static> {
    if value.trim().is_empty() {
        return Span::styled("None", Style::new().fg(THEME.text_muted));
    }
    match field {
        SetupField::Token => Span::raw(mask(&value)),
        _ => Span::raw(value),
    }
}

fn mask(token: &str) -> String {
    const MASK: &str = "••••••••";
    let count = token.chars().count();
    if count <= 4 {
        return MASK.into();
    }
    format!(
        "{MASK}{}",
        token.chars().skip(count - 4).collect::<String>()
    )
}

fn hint_line(setup: &Setup) -> Line<'static> {
    let keys: &[(&str, &str)] = if setup.editing.is_some() {
        &[("Enter", "save field"), ("Esc", "cancel")]
    } else {
        &[
            ("j / k", "move"),
            ("Enter", "edit"),
            ("t", "test"),
            ("s", "save"),
            ("q", "quit"),
        ]
    };

    Line::from(
        keys.iter()
            .flat_map(|&(key, desc)| {
                [
                    Span::styled(format!(" {key} "), Style::new().fg(THEME.accent).bold()),
                    Span::styled(format!("{desc} "), Style::new().fg(THEME.text_muted)),
                ]
            })
            .collect::<Vec<_>>(),
    )
}
