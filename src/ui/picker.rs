use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{App, Picker, PickerOption},
    domain::issue::FieldId,
    theme::THEME,
    ui::helpers,
};

fn picker_title(picker: &Picker) -> String {
    if picker.multi {
        format!(
            "{} ({} selected)",
            picker.field.label(),
            picker.selected.len()
        )
    } else {
        format!("Change {}", picker.field.label())
    }
}

fn options_noun(field: FieldId) -> &'static str {
    match field {
        FieldId::Labels => "labels",
        FieldId::FixVersions => "versions",
        FieldId::Status => "transitions",
        FieldId::Assignee | FieldId::Reporter => "people",
        FieldId::Priority => "priorities",
        FieldId::Parent => "issues",
        FieldId::Type => "issue types",
        _ => "options",
    }
}

pub(super) fn render_picker(frame: &mut Frame, area: Rect, app: &App) {
    let Some(picker) = &app.picker else {
        return;
    };

    let popup = helpers::popup_area(
        frame,
        helpers::centered(area, Constraint::Percentage(50), Constraint::Percentage(60)),
    );

    let block = helpers::pane_block(&picker_title(picker), true);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [filter_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);

    frame.render_widget(filter_line(picker), filter_area);

    if picker.loading {
        let text = Paragraph::new(format!(
            " {} Loading {}...",
            helpers::spinner(),
            options_noun(picker.field)
        ))
        .style(Style::new().fg(THEME.text_muted));
        frame.render_widget(text, list_area);
        return;
    }

    let filtered = picker.filtered();
    if filtered.is_empty() {
        let text = Paragraph::new(format!(" No matching {}", options_noun(picker.field)))
            .style(Style::new().fg(THEME.text_muted));
        frame.render_widget(text, list_area);
        return;
    }

    let items: Vec<ListItem> = filtered
        .iter()
        .map(|option| picker_item(picker, option))
        .collect();

    let list = List::new(items).highlight_style(Style::new().bg(THEME.highlight_bg).bold());
    let mut state = ListState::default().with_selected(Some(picker.cursor));
    frame.render_stateful_widget(list, list_area, &mut state);
}

fn picker_item<'a>(picker: &Picker, option: &'a PickerOption) -> ListItem<'a> {
    const CHECK_ON: &str = "\u{f4a7}";
    const CHECK_OFF: &str = "\u{f51d}";

    let ticked = picker.selected.contains(&option.id);
    let mark = if ticked { CHECK_ON } else { CHECK_OFF };
    let mark_style = Style::new()
        .fg(if ticked {
            THEME.green
        } else {
            THEME.text_muted
        })
        .bold();

    ListItem::new(Line::from(vec![
        Span::styled(format!(" {mark} "), mark_style),
        Span::styled(option.label.as_str(), Style::new().fg(THEME.text)),
    ]))
}

fn filter_line(picker: &Picker) -> Line<'_> {
    let icon = Span::styled(
        format!(" {} ", helpers::SEARCH_ICON),
        Style::new().fg(THEME.accent),
    );

    if picker.searching {
        Line::from(vec![
            icon,
            Span::styled(picker.query.as_str(), Style::new().fg(THEME.text)),
            Span::styled("█", Style::new().fg(THEME.accent)),
        ])
    } else if picker.query.is_empty() {
        Line::from(vec![
            icon,
            Span::styled("Search", Style::new().fg(THEME.text_muted).italic()),
        ])
    } else {
        Line::from(vec![
            icon,
            Span::styled(picker.query.as_str(), Style::new().fg(THEME.text_muted)),
        ])
    }
}
