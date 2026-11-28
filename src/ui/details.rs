use std::mem;

use chrono::{DateTime, Local, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::{
    app::{App, EditState, View},
    domain::issue::{FieldId, FieldKind, Issue},
    text_input::TextInput,
    theme::THEME,
    ui::helpers::{self, due_style, status_span},
};

fn field_rows() -> Vec<FieldId> {
    FieldId::ALL
        .into_iter()
        .filter(|f| f.kind() != FieldKind::LongText)
        .collect()
}

fn label_width() -> usize {
    FieldId::ALL
        .iter()
        .map(|field| field.label().chars().count())
        .max()
        .unwrap_or(0)
}

fn value_indent() -> usize {
    label_width() + 3
}

pub(super) fn render_details(frame: &mut Frame, area: Rect, now: DateTime<Utc>, app: &App) {
    let inspecting = app.view == View::Inspector;
    let creating = app.draft.is_some();
    let issue = app.current_issue();
    let cursor = inspecting.then_some(app.selected_field);
    let editing = app.editing.as_ref();
    let rows = field_rows();

    let title = match issue {
        Some(issue) if creating => format!("New issue · {}", issue.project),
        Some(issue) => issue.key.clone(),
        _ => "Details".to_string(),
    };

    let mut block = helpers::pane_block(&title, inspecting);
    if app.creating {
        block = block.title_bottom(Line::styled(
            format!(" {} Creating... ", helpers::spinner()),
            Style::new().fg(THEME.text_muted),
        ));
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(issue) = issue else {
        frame.render_widget(Paragraph::new("Select an item"), inner);
        return;
    };

    let extra_rows = editing
        .filter(|edit| edit.field.kind() != FieldKind::LongText)
        .map(|edit| wrap_pieces(&edit.input.buffer, edit_width(inner.width)).len() as u16 - 1)
        .unwrap_or(0);

    let [fields_area, description_area] = Layout::vertical([
        Constraint::Length(rows.len() as u16 + extra_rows),
        Constraint::Fill(1),
    ])
    .areas(inner);

    render_fields(frame, fields_area, now, issue, creating, cursor, editing);
    render_description(
        frame,
        description_area,
        issue,
        app.details_scroll,
        cursor,
        editing,
    );
}

fn render_fields(
    frame: &mut Frame,
    area: Rect,
    now: DateTime<Utc>,
    issue: &Issue,
    creating: bool,
    cursor: Option<FieldId>,
    editing: Option<&EditState>,
) {
    let today = now.with_timezone(&Local).date_naive();
    let items: Vec<ListItem> = field_rows()
        .iter()
        .map(|&field| {
            let is_editing = editing.is_some_and(|edit| edit.field == field);
            let active = is_editing || cursor == Some(field);
            let label = Span::styled(
                format!(" {:<width$}  ", field.label(), width = label_width()),
                Style::new().fg(if active { THEME.accent } else { THEME.text }),
            );

            let item = match (field, editing) {
                (_, Some(edit)) if edit.field == field => ListItem::new(edit_lines(
                    label,
                    value_indent(),
                    &edit.input,
                    edit_width(area.width),
                )),
                _ => {
                    let value = match (field, issue.field_value(field, now)) {
                        (FieldId::Status, _) => status_span(&issue.status, &issue.status_category),
                        (_, value) if value.is_empty() => {
                            Span::styled("None", Style::new().fg(THEME.text_muted))
                        }
                        (FieldId::Due, value) => {
                            Span::styled(value, due_style(issue.due_state(today)))
                        }
                        (field, value) if !field.is_editable(creating) => {
                            Span::styled(value, Style::new().fg(THEME.text_muted))
                        }
                        (_, value) => Span::raw(value),
                    };
                    ListItem::new(Line::from(vec![label, value]))
                }
            };

            if is_editing {
                item.style(Style::new().bg(THEME.highlight_bg))
            } else if cursor == Some(field) {
                item.style(Style::new().bg(THEME.highlight_bg).bold())
            } else {
                item
            }
        })
        .collect();

    frame.render_widget(List::new(items), area);
}

fn edit_width(area_width: u16) -> usize {
    (area_width as usize)
        .saturating_sub(value_indent() + 1)
        .max(8)
}

fn wrap_pieces(text: &str, width: usize) -> Vec<(usize, String)> {
    let mut pieces = Vec::new();
    let mut offset = 0;

    for hard_line in text.split('\n') {
        pieces.extend(wrap_line(hard_line, width, offset));
        offset += hard_line.chars().count() + 1;
    }

    pieces
}

fn wrap_line(text: &str, width: usize, offset: usize) -> Vec<(usize, String)> {
    let mut pieces: Vec<(usize, String)> = Vec::new();
    let mut line = String::new();
    let mut line_start = offset;
    let mut pos = offset;

    for mut word in text.split_inclusive(' ') {
        loop {
            let line_len = line.chars().count();
            let word_len = word.chars().count();

            if line_len + word_len <= width {
                line.push_str(word);
                pos += word_len;
                break;
            }
            if line.is_empty() {
                let split = word
                    .char_indices()
                    .nth(width)
                    .map_or(word.len(), |(i, _)| i);
                let (head, tail) = word.split_at(split);
                pieces.push((line_start, head.to_string()));
                pos += head.chars().count();
                line_start = pos;
                word = tail;
                continue;
            }
            pieces.push((line_start, mem::take(&mut line)));
            line_start = pos;
        }
    }

    pieces.push((line_start, line));
    pieces
}

pub(super) fn edit_lines<'a>(
    label: impl Into<Line<'a>>,
    indent: usize,
    input: &TextInput,
    width: usize,
) -> Vec<Line<'a>> {
    let label = label.into();
    let text_style = Style::new().fg(THEME.text);
    let cursor_style = Style::new()
        .fg(THEME.base)
        .bg(THEME.text)
        .add_modifier(Modifier::SLOW_BLINK);

    let pieces = wrap_pieces(&input.buffer, width);
    let last = pieces.len() - 1;
    let hard_break: Vec<bool> = pieces
        .iter()
        .zip(pieces.iter().skip(1))
        .map(|((start, text), (next_start, _))| *next_start > start + text.chars().count())
        .chain([false])
        .collect();

    pieces
        .into_iter()
        .enumerate()
        .map(|(n, (start, text))| {
            let mut spans = if n == 0 {
                label.spans.clone()
            } else {
                vec![Span::raw(" ".repeat(indent))]
            };

            let len = text.chars().count();
            match input.cursor.checked_sub(start) {
                Some(at) if at < len => {
                    let byte = text.char_indices().nth(at).map_or(text.len(), |(i, _)| i);
                    let (before, rest) = text.split_at(byte);
                    let mut after = rest.chars();
                    let c = after.next().unwrap_or(' ');
                    spans.push(Span::styled(before.to_string(), text_style));
                    spans.push(Span::styled(c.to_string(), cursor_style));
                    spans.push(Span::styled(after.as_str().to_string(), text_style));
                }
                _ => {
                    spans.push(Span::styled(text, text_style));
                    if input.cursor == start + len && hard_break[n] {
                        spans.push(Span::styled(" ", cursor_style));
                    } else if n == last && input.cursor >= start + len {
                        spans.push(Span::styled(
                            "█",
                            text_style.add_modifier(Modifier::SLOW_BLINK),
                        ));
                    }
                }
            }

            Line::from(spans)
        })
        .collect()
}

fn render_description(
    frame: &mut Frame,
    area: Rect,
    issue: &Issue,
    scroll: u16,
    cursor: Option<FieldId>,
    editing: Option<&EditState>,
) {
    let editing = editing.filter(|edit| edit.field == FieldId::Description);
    let active = editing.is_some() || cursor == Some(FieldId::Description);

    let style = Style::new().fg(if active { THEME.accent } else { THEME.border });
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(style)
        .title(Span::styled(" Description ", style));

    let body = match editing {
        Some(edit) => Paragraph::new(edit_lines(
            Span::raw(""),
            0,
            &edit.input,
            (area.width as usize).saturating_sub(1).max(8),
        ))
        .style(style)
        .block(block)
        .scroll((scroll, 0)),
        None => {
            let lines: Vec<Line> = if issue.description.is_empty() {
                vec![Line::from("No description").style(Style::new().fg(THEME.text_muted))]
            } else {
                issue.description.lines().map(Line::from).collect()
            };
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(block)
                .scroll((scroll, 0))
        }
    };
    frame.render_widget(body, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_input::TextInput;

    #[test]
    fn wrap_pieces_covers_text_and_fits_width() {
        let text = "maple the flaky maplesyrup test on supercalifragilistic maplebushes";
        let pieces = wrap_pieces(text, 12);

        assert_eq!(
            pieces.iter().map(|(_, t)| t.as_str()).collect::<String>(),
            text
        );
        for (start, piece) in &pieces {
            assert!(piece.chars().count() <= 12, "too wide: {piece:?}");
            assert_eq!(
                &text
                    .chars()
                    .skip(*start)
                    .take(piece.chars().count())
                    .collect::<String>(),
                piece
            );
        }
        assert!(pieces.len() > 1);
        assert_eq!(wrap_pieces("", 12), vec![(0, String::new())]);
    }

    #[test]
    fn edit_lines_break_at_newlines_and_show_the_cursor_on_one() {
        let mut edit = EditState {
            issue_key: "MAPLE-1".into(),
            field: FieldId::Description,
            input: TextInput {
                buffer: "map\nple\n\nmaple".into(),
                cursor: 3,
                goal_column: None,
            },
        };

        let lines = edit_lines(Span::raw(""), 0, &edit.input, 20);
        let text = |line: &Line| {
            line.spans
                .iter()
                .map(|s| s.content.to_string())
                .collect::<String>()
        };
        assert_eq!(
            lines.iter().map(text).collect::<Vec<_>>(),
            ["map ", "ple", "", "maple"]
        );

        // the cursor sits on the first newline, drawn as a highlighted space
        assert_eq!(lines[0].spans.last().unwrap().content.as_ref(), " ");

        edit.input.cursor = 5;
        let lines = edit_lines(Span::raw(""), 0, &edit.input, 20);
        assert_eq!(lines[1].spans[2].content.as_ref(), "l");
    }

    #[test]
    fn edit_lines_render_at_the_minimum_width() {
        let label = Span::raw("Summary");
        let mut edit = EditState {
            issue_key: "MAPLE-1".into(),
            field: FieldId::Summary,
            input: TextInput {
                buffer: "éé maplé spläsh".into(),
                cursor: 0,
                goal_column: None,
            },
        };

        let expected = wrap_pieces(&edit.input.buffer, 8).len();
        assert!(
            expected > 1,
            "the buffer has to wrap for this to mean anything"
        );

        for cursor in [0, 5, edit.input.buffer.chars().count()] {
            edit.input.cursor = cursor;
            assert_eq!(
                edit_lines(label.clone(), value_indent(), &edit.input, 8).len(),
                expected
            );
        }

        // the block cursor only shows up past the end of the text
        let last = edit_lines(label.clone(), value_indent(), &edit.input, 8)
            .pop()
            .unwrap();
        assert!(last.spans.iter().any(|s| s.content.as_ref() == "█"));
    }
}
