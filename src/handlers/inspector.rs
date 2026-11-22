use std::time::Instant;

use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, Notice},
    domain::issue::FieldKind,
    event::Event,
    fetch,
    jira::client::JiraClient,
};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
    pending: Option<char>,
) {
    if app.creating {
        return;
    }

    match key.code {
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            create(app, tx, client)
        }
        KeyCode::Esc => app.close_inspector(),
        KeyCode::Char('j') | KeyCode::Down => app.select_next_field(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev_field(),
        KeyCode::Char('o') => super::open_selected_in_browser(app, client),
        KeyCode::Char('d') => {
            if pending == Some('d') {
                app.ask_delete()
            } else {
                app.pending_key = Some(('d', Instant::now()))
            }
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.request_external_edit()
        }
        KeyCode::Enter => match app.field_kind(app.selected_field) {
            FieldKind::Date => app.open_date_picker(Local::now().date_naive()),
            FieldKind::Text | FieldKind::LongText => app.start_edit(),
            FieldKind::SingleSelect | FieldKind::MultiSelect => {
                if let Some(request) = app.open_picker() {
                    fetch::spawn_fetch_options(client.clone(), tx.clone(), request);
                }
            }
            FieldKind::ReadOnly => {}
        },
        _ => {}
    }
}

fn create(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    if app.draft.is_none() {
        return;
    }
    match app.prepare_create() {
        Ok(draft) => {
            app.set_notice(Notice::Info("Creating...".into()));
            fetch::spawn_create_issue(client.clone(), tx.clone(), draft);
        }
        Err(message) => app.set_notice(Notice::Error(message)),
    }
}
