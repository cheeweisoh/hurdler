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
            ("j / k", "move between queries"),
            ("Enter / Spc", "apply the selected query"),
            ("Esc", "close the list"),
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
    let entries = help_entries(mode);
    let popup = helpers::popup_area(
        frame,
        helpers::centered(
            area,
            Constraint::Length(56),
            Constraint::Length(entries.len() as u16 + 6),
        ),
    );

    let block =
        helpers::pane_block(&format!("Help · {mode}"), true).padding(Padding::new(2, 2, 1, 1));

    let lines: Vec<Line> = entries
        .iter()
        .map(|&(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:<KEY_WIDTH$}"),
                    Style::new().fg(THEME.accent).bold(),
                ),
                Span::styled(desc, Style::new().fg(THEME.text)),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
