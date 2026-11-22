use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::App, event::Event, fetch, jira::client::JiraClient};

pub(super) fn clear(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    if app.clear_search(client.jql()) {
        let generation = app.begin_refresh();
        fetch::spawn_fetch_issues(client.clone(), tx.clone(), app.list_jql.clone(), generation);
    }
}

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => app.close_search_input(),
        KeyCode::Esc => clear(app, tx, client),
        _ => {
            let Some(search) = &mut app.search else {
                return;
            };
            let input = &mut search.input;
            match key.code {
                KeyCode::Char('w') if ctrl => input.delete_word(),
                KeyCode::Char('u') if ctrl => input.clear(),
                KeyCode::Backspace => input.backspace(),
                KeyCode::Left => input.move_left(),
                KeyCode::Right => input.move_right(),
                KeyCode::Char(c) if !ctrl => input.insert(c),
                _ => return,
            }
            app.search_edited();
        }
    }
}
