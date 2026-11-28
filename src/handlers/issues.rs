use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, Tab},
    domain::issue::FieldId,
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
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.issues.next();
            load_more_if_needed(app, tx, client);
        }
        KeyCode::Char('k') | KeyCode::Up => app.issues.prev(),
        KeyCode::Char('o') => super::open_selected_in_browser(app, client),
        KeyCode::Char('n') if app.active_tab == Tab::Issues => app.start_draft(),
        KeyCode::Char('d') if app.active_tab == Tab::Issues => {
            if pending == Some('d') {
                app.ask_delete()
            } else {
                app.pending_key = Some(('d', Instant::now()))
            }
        }
        KeyCode::Char('g') => {
            if app.active_tab == Tab::Issues && pending == Some('g') {
                app.issues.first()
            } else {
                app.pending_key = Some(('g', Instant::now()))
            }
        }
        KeyCode::Char('G') => {
            if app.active_tab == Tab::Issues {
                app.issues.last();
                load_more_if_needed(app, tx, client);
            }
        }
        KeyCode::Char('f') if app.active_tab == Tab::Issues => app.open_query_menu(),
        KeyCode::Char('/') if app.active_tab == Tab::Issues => app.open_search(),
        KeyCode::Enter => app.open_inspector(),
        KeyCode::Char('a') if app.active_tab == Tab::Issues => {
            if let Some(change) = app.assign_to_me() {
                super::save_change(app, tx, client, change);
            }
        }
        KeyCode::Char('s') if app.active_tab == Tab::Issues => {
            if let Some(request) = app.open_picker_for(FieldId::Status) {
                fetch::spawn_fetch_options(client.clone(), tx.clone(), request);
            }
        }
        _ => {}
    }
}

fn load_more_if_needed(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    if !app.near_end() {
        return;
    }
    if let Some((token, generation)) = app.request_more() {
        let jql = app.list_jql.clone();
        fetch::spawn_fetch_more(client.clone(), tx.clone(), jql, token, generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    use crate::{app::StatefulTable, config::Config, domain::issue::Issue};

    #[tokio::test]
    async fn gg_jumps_to_the_top_only_as_a_pair() {
        let issue = |key: &str| Issue {
            key: key.into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue("MAPLE-1"), issue("MAPLE-2"), issue("MAPLE-3")]),
            ..Default::default()
        };
        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        let (tx, _rx) = mpsc::unbounded_channel();

        // routed through handlers::handle_key so the pending-key take and its
        // timeout filter are both in the path
        let press = |app: &mut App, c: char| {
            super::super::handle_key(app, KeyEvent::from(KeyCode::Char(c)), &tx, &client)
        };

        app.issues.last();
        assert_eq!(app.issues.state.selected(), Some(2));

        // the first g arms the pair and moves nothing
        press(&mut app, 'g');
        assert_eq!(app.issues.state.selected(), Some(2));
        assert!(app.pending_key.is_some(), "g on its own is a half-command");

        // the second completes it
        press(&mut app, 'g');
        assert_eq!(app.issues.state.selected(), Some(0));
        assert!(app.pending_key.is_none(), "gg consumes the pending g");

        // anything in between disarms it, so g-k-g is not gg
        app.issues.last();
        press(&mut app, 'g');
        press(&mut app, 'k');
        press(&mut app, 'g');
        assert_eq!(app.issues.state.selected(), Some(1), "g k g is not gg");

        // G needs no pair
        press(&mut app, 'G');
        assert_eq!(app.issues.state.selected(), Some(2));

        // ponytail: the 1s PENDING_TIMEOUT expiry is left untested rather than
        // sleep in the suite. Inject a clock if it ever gets more logic.
    }

    #[tokio::test]
    async fn a_single_d_arms_nothing_destructive() {
        let issue = |key: &str| Issue {
            key: key.into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue("MAPLE-1"), issue("MAPLE-2")]),
            ..Default::default()
        };
        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        let (tx, _rx) = mpsc::unbounded_channel();
        let press = |app: &mut App, c: char| {
            super::super::handle_key(app, KeyEvent::from(KeyCode::Char(c)), &tx, &client)
        };

        app.issues.first();

        // one d is a half-command: no dialog, nothing deleted
        press(&mut app, 'd');
        assert!(app.confirm_delete.is_none(), "a lone d must not confirm");
        assert!(app.pending_key.is_some());

        // the pair opens the dialog, which is the only thing that can delete
        press(&mut app, 'd');
        assert_eq!(app.pending_delete_key(), Some("MAPLE-1"));
        assert_eq!(app.issues.items.len(), 2, "the dialog deletes nothing yet");

        // d-j-d is not dd
        app.cancel_delete();
        press(&mut app, 'd');
        press(&mut app, 'j');
        press(&mut app, 'd');
        assert!(app.confirm_delete.is_none(), "d j d is not dd");
    }
}
