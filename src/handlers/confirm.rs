use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, DeleteTarget, Notice},
    event::Event,
    fetch,
    jira::client::JiraClient,
};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let deleting = match key.code {
        KeyCode::Char('h') | KeyCode::Left => return app.focus_delete_button(true),
        KeyCode::Char('l') | KeyCode::Right => return app.focus_delete_button(false),
        KeyCode::Tab | KeyCode::BackTab => return app.toggle_delete_button(),
        KeyCode::Enter => app.press_delete_button(),
        KeyCode::Char('y') => app.take_delete(),
        KeyCode::Esc | KeyCode::Char('n') => return app.cancel_delete(),
        _ => return,
    };

    match deleting {
        Some(DeleteTarget::Issue(key)) => {
            app.set_notice(Notice::Info(format!("Deleting {key}...")));
            fetch::spawn_delete_issue(client.clone(), tx.clone(), key);
        }
        Some(DeleteTarget::Query(i)) => {
            let active_removed = app.delete_query(i).is_some();
            if active_removed {
                super::fetch_list(app, tx, client);
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    use crate::{app::StatefulTable, config::Config, domain::issue::Issue};

    fn app_and_client() -> (App, JiraClient, mpsc::UnboundedReceiver<Event>) {
        let issue = |key: &str| Issue {
            key: key.into(),
            ..Default::default()
        };
        let app = App {
            issues: StatefulTable::new(vec![issue("MAPLE-1"), issue("MAPLE-2")]),
            ..Default::default()
        };
        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        let (_tx, rx) = mpsc::unbounded_channel();
        (app, client, rx)
    }

    fn pending_key(app: &App) -> Option<&str> {
        app.pending_delete_key()
    }

    fn focused(app: &App) -> bool {
        app.confirm_delete
            .as_ref()
            .expect("the dialog is open")
            .confirm_focused
    }

    #[tokio::test]
    async fn enter_presses_whichever_button_has_focus() {
        let (mut app, client, _rx) = app_and_client();
        let (tx, _keep) = mpsc::unbounded_channel();
        let press = |app: &mut App, code: KeyCode| {
            handle_key(app, KeyEvent::from(code), &tx, &client);
        };

        // Cancel holds focus on open, so a stray Enter backs out rather than deletes
        app.ask_delete();
        assert!(!focused(&app), "the dialog opens on Cancel");
        press(&mut app, KeyCode::Enter);
        assert!(app.confirm_delete.is_none(), "Enter on Cancel backs out");
        assert_eq!(app.issues.items.len(), 2, "cancelling deletes nothing");

        // h / Left reaches Confirm, l / Right goes back
        app.ask_delete();
        for code in [KeyCode::Char('h'), KeyCode::Left] {
            app.focus_delete_button(false);
            press(&mut app, code);
            assert!(focused(&app), "{code:?} moves onto Confirm");
        }
        for code in [KeyCode::Char('l'), KeyCode::Right, KeyCode::Tab] {
            app.focus_delete_button(true);
            press(&mut app, code);
            assert!(!focused(&app), "{code:?} leaves Confirm");
        }

        // Enter on Confirm closes the dialog and hands the key to the request
        app.focus_delete_button(true);
        press(&mut app, KeyCode::Enter);
        assert!(
            app.confirm_delete.is_none(),
            "the dialog closes so a second Enter cannot fire a second delete"
        );
        assert_eq!(
            app.issues.items.len(),
            2,
            "the row goes only once Jira confirms"
        );
    }

    #[tokio::test]
    async fn cancel_keys_back_out_and_unknown_keys_do_nothing() {
        let (mut app, client, _rx) = app_and_client();
        let (tx, _keep) = mpsc::unbounded_channel();

        for code in [KeyCode::Esc, KeyCode::Char('n')] {
            app.ask_delete();
            handle_key(&mut app, KeyEvent::from(code), &tx, &client);
            assert!(app.confirm_delete.is_none(), "{code:?} cancels");
            assert_eq!(app.issues.items.len(), 2, "cancelling deletes nothing");
        }

        for code in [KeyCode::Char('d'), KeyCode::Char('Y'), KeyCode::Char(' ')] {
            app.ask_delete();
            handle_key(&mut app, KeyEvent::from(code), &tx, &client);
            assert_eq!(
                pending_key(&app),
                Some("MAPLE-1"),
                "{code:?} must leave the dialog open"
            );
            assert!(!focused(&app), "and must not move focus onto Confirm");
        }

        // y stays as a shortcut past the buttons
        handle_key(&mut app, KeyEvent::from(KeyCode::Char('y')), &tx, &client);
        assert!(app.confirm_delete.is_none());
    }

    #[tokio::test]
    async fn a_draft_is_discarded_rather_than_deleted() {
        let (mut app, _client, _rx) = app_and_client();
        app.start_draft();

        app.ask_delete();
        assert!(
            app.confirm_delete.is_none(),
            "a draft has no Jira issue to delete; Esc throws it away"
        );
    }
}
