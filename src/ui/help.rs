use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use crate::{
    app::{App, Mode},
    theme::THEME,
    ui::helpers,
};

const KEY_WIDTH: usize = 12;

fn help_entries(mode: Mode) -> &'static [(&'static str, &'static str)] {
    match mode {
        Mode::Normal => &[
            ("f", "pick a saved query"),
            ("j / k", "move between issues"),
            ("gg / G", "first / last issue"),
            ("Enter", "open issue"),
            ("n / dd", "new / delete issue"),
            ("J / K", "scroll description"),
            ("Tab / 1-4", "switch view"),
            ("/", "search all issues"),
            ("Esc", "clear search"),
            ("r", "refresh"),
            ("q", "quit"),
        ],
        Mode::Queries => &[
            ("", "On the list"),
            ("j / k", "move between queries"),
            ("Enter / Spc", "apply the selected query"),
            ("a / e / d", "add / edit / delete a query"),
            ("Esc", "close the list"),
            ("", "In the form"),
            ("j / k", "move between fields"),
            ("Enter", "edit field / line break in Query"),
            ("Ctrl+S", "save the query"),
            ("Esc", "cancel the field, then leave the form"),
        ],
        Mode::Search => &[
            ("type", "search every issue in Jira"),
            ("Backspace", "delete char"),
            ("Ctrl+W", "delete word"),
            ("Ctrl+U", "delete all"),
            ("Enter", "keep results, back to list"),
            ("Esc", "clear search"),
        ],
        Mode::Inspect => &[
            ("j / k", "move between fields"),
            ("Enter", "edit / pick value"),
            ("dd", "delete this issue"),
            ("Esc", "back to list"),
            ("q", "quit"),
        ],
        Mode::Create => &[
            ("j / k", "move between fields"),
            ("Enter", "edit / pick value"),
            ("Ctrl+S", "create the issue"),
            ("Esc", "discard the draft"),
        ],
        Mode::Edit => &[
            ("Left / Right", "move cursor"),
            ("Backspace", "delete char"),
            ("Enter", "save field"),
            ("Esc", "cancel"),
        ],
        Mode::Pick => &[
            ("type", "filter options"),
            ("Up / Down", "move between options"),
            ("Space", "toggle option"),
            ("Enter", "save selection"),
            ("Esc", "cancel"),
        ],
        Mode::Confirm => &[
            ("h / l / Tab", "switch button"),
            ("Enter", "press the focused button"),
            ("y", "delete without the buttons"),
            ("n / Esc", "keep it"),
            ("q", "quit"),
        ],
    }
}

pub(super) fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    if !app.show_help {
        return;
    }

    let mode = app.mode();
    let lines = help_lines(mode);
    let popup = helpers::popup_area(
        frame,
        helpers::centered(
            area,
            Constraint::Length(56),
            Constraint::Length(lines.len() as u16 + 6),
        ),
    );

    let block =
        helpers::pane_block(&format!("Help · {mode}"), true).padding(Padding::new(2, 2, 1, 1));

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

fn help_lines(mode: Mode) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    for &(key, desc) in help_entries(mode) {
        if key.is_empty() {
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            lines.push(Line::styled(desc, Style::new().fg(THEME.text_muted).bold()));
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{key:<KEY_WIDTH$}"),
                Style::new().fg(THEME.accent).bold(),
            ),
            Span::styled(desc, Style::new().fg(THEME.text)),
        ]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_breaks_the_two_enters_apart() {
        let lines: Vec<String> = help_lines(Mode::Queries)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        let enters: Vec<&String> = lines
            .iter()
            .filter(|line| line.starts_with("Enter"))
            .collect();
        assert_eq!(enters.len(), 2, "{lines:?}");

        let heading = lines
            .iter()
            .position(|line| line == "In the form")
            .expect("the form has its own heading");
        let list_enter = lines
            .iter()
            .position(|line| line.starts_with("Enter / Spc"))
            .expect("the list has its own Enter");
        assert!(
            list_enter < heading,
            "each Enter sits under the heading it belongs to: {lines:?}"
        );
        assert!(lines[heading - 1].is_empty(), "with a blank line above it");

        // every key fits its column, or the descriptions would not line up
        for &(key, _) in help_entries(Mode::Queries) {
            assert!(key.chars().count() <= KEY_WIDTH, "{key}");
        }
    }
}
