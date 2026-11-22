use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::App, event::Event, jira::client::JiraClient};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let today = Local::now().date_naive();

    let save = match key.code {
        KeyCode::Esc => return app.close_date_picker(),
        KeyCode::Enter => Some(false),
        KeyCode::Char('x') | KeyCode::Backspace => Some(true),
        _ => None,
    };
    if let Some(clear) = save {
        if let Some(change) = app.confirm_date_picker(clear) {
            super::save_change(app, tx, client, change);
        }
        return;
    }

    let Some(picker) = &mut app.date_picker else {
        return;
    };
    match key.code {
        KeyCode::Char('t') => picker.date = today,
        KeyCode::Char('h') | KeyCode::Left => picker.move_days(-1),
        KeyCode::Char('l') | KeyCode::Right => picker.move_days(1),
        KeyCode::Char('k') | KeyCode::Up => picker.move_days(-7),
        KeyCode::Char('j') | KeyCode::Down => picker.move_days(7),
        KeyCode::Char('H') => picker.move_months(-1),
        KeyCode::Char('L') => picker.move_months(1),
        _ => {}
    }
}
