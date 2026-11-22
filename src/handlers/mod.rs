mod confirm;
mod date_picker;
mod editing;
mod inspector;
mod issues;
mod picker;
mod search;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, Notice, Tab, View},
    browser,
    domain::issue::FieldChange,
    event::Event,
    fetch,
    jira::client::JiraClient,
};

const PENDING_TIMEOUT: Duration = Duration::from_millis(1000);

pub(crate) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let pending = app
        .pending_key
        .take()
        .filter(|(_, pressed_at)| pressed_at.elapsed() < PENDING_TIMEOUT)
        .map(|(c, _)| c);

    let typing = app.editing.is_some()
        || app.search_editing()
        || app.picker.as_ref().is_some_and(|picker| picker.searching);

    if key.code == KeyCode::Char('q') && !typing {
        app.should_quit = true;
        return;
    }

    if app.picker.is_some() {
        picker::handle_key(app, key, tx, client);
        return;
    }

    if app.date_picker.is_some() {
        date_picker::handle_key(app, key, tx, client);
        return;
    }

    if app.confirm_delete.is_some() {
        confirm::handle_key(app, key, tx, client);
        return;
    }

    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.show_help = false;
        }
        return;
    }

    if key.code == KeyCode::Char('?') && app.editing.is_none() && !app.search_editing() {
        app.show_help = true;
        return;
    }

    if app.active_tab == Tab::Issues {
        if app.editing.is_some() {
            editing::handle_key(app, key, tx, client);
            return;
        }
        if app.view == View::Inspector {
            inspector::handle_key(app, key, tx, client, pending);
            return;
        }
        if app.search_editing() {
            search::handle_key(app, key, tx, client);
            return;
        }
    }

    match key.code {
        KeyCode::Esc if app.search.is_some() => search::clear(app, tx, client),
        KeyCode::Tab => app.active_tab = app.active_tab.next(),
        KeyCode::BackTab => app.active_tab = app.active_tab.prev(),
        KeyCode::Char('r') if !app.loading => {
            let generation = app.begin_refresh();
            fetch::spawn_fetch_issues(client.clone(), tx.clone(), app.list_jql.clone(), generation);
        }
        _ if app.active_tab == Tab::Issues => issues::handle_key(app, key, tx, client, pending),
        _ => {}
    }
}

fn save_change(app: &App, tx: &UnboundedSender<Event>, client: &JiraClient, change: FieldChange) {
    if app.is_draft(&change.issue_key) {
        return;
    }
    fetch::spawn_update_field(client.clone(), tx.clone(), change);
}

fn open_selected_in_browser(app: &mut App, client: &JiraClient) {
    let Some(key) = app.selected_issue_key().filter(|key| !app.is_draft(key)) else {
        return;
    };
    match browser::open_url(&client.browse_url(&key)) {
        Ok(()) => app.set_notice(Notice::Info(format!("Opened {key} in browser"))),
        Err(e) => app.set_notice(Notice::Error(format!("Couldn't open browser: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    use crate::{app::StatefulTable, config::Config, domain::issue::Issue};

    #[tokio::test]
    async fn question_mark_types_into_an_edit_instead_of_opening_help() {
        let issue = Issue {
            key: "MAPLE-1".into(),
            summary: "maple".into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue]),
            ..Default::default()
        };
        app.start_edit();

        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            email: "nobody@example.com".into(),
            jql: String::new(),
            api_token: String::new(),
            ..Default::default()
        });
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('?')), &tx, &client);

        assert!(!app.show_help, "help must not steal a keystroke mid-edit");
        assert_eq!(app.editing.expect("still editing").input.buffer, "maple?");
    }

    fn app_and_client() -> (App, JiraClient) {
        let issue = Issue {
            key: "MAPLE-1".into(),
            summary: "maple".into(),
            ..Default::default()
        };
        let app = App {
            issues: StatefulTable::new(vec![issue]),
            ..Default::default()
        };
        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        (app, client)
    }

    #[tokio::test]
    async fn q_quits_from_every_layer_and_esc_only_backs_out_of_one() {
        let (tx, _rx) = mpsc::unbounded_channel();

        // whatever is on screen, q leaves the app
        for layer in ["list", "inspector", "confirmation", "help"] {
            let (mut app, client) = app_and_client();
            match layer {
                "inspector" => app.open_inspector(),
                "confirmation" => {
                    app.open_inspector();
                    app.ask_delete();
                }
                "help" => app.show_help = true,
                _ => {}
            }
            handle_key(&mut app, KeyEvent::from(KeyCode::Char('q')), &tx, &client);
            assert!(app.should_quit, "q must quit from the {layer}");
        }

        // Esc unwinds one layer at a time and never quits
        let (mut app, client) = app_and_client();
        let press = |app: &mut App, code| handle_key(app, KeyEvent::from(code), &tx, &client);

        app.open_inspector();
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.view, View::List, "Esc leaves the inspector");
        assert!(!app.should_quit);

        press(&mut app, KeyCode::Esc);
        assert!(!app.should_quit, "Esc on the bare list does nothing");

        app.open_inspector();
        app.ask_delete();
        press(&mut app, KeyCode::Esc);
        assert!(app.confirm_delete.is_none(), "Esc cancels the dialog");
        assert_eq!(app.view, View::Inspector, "and stops at that one layer");
    }

    #[tokio::test]
    async fn q_is_a_character_wherever_text_is_being_typed() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let q = KeyEvent::from(KeyCode::Char('q'));

        // a field edit
        let (mut app, client) = app_and_client();
        app.start_edit();
        handle_key(&mut app, q, &tx, &client);
        assert!(!app.should_quit, "q must not quit mid-edit");
        assert_eq!(app.editing.expect("still editing").input.buffer, "mapleq");

        // the search bar
        let (mut app, client) = app_and_client();
        app.open_search();
        handle_key(&mut app, q, &tx, &client);
        assert!(!app.should_quit, "q must not quit mid-search");
        assert_eq!(app.search.expect("still searching").input.buffer, "q");

        // a picker's filter
        let (mut app, client) = app_and_client();
        app.open_inspector();
        app.selected_field = crate::domain::issue::FieldId::Status;
        app.open_picker().expect("status is pickable");
        app.picker_mut().start_search();
        handle_key(&mut app, q, &tx, &client);
        assert!(!app.should_quit, "q must not quit mid-filter");
        assert_eq!(app.picker.expect("the picker is open").query, "q");
    }
}
