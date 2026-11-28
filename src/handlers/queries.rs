use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::App, event::Event, jira::client::JiraClient};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_query_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_query_cursor(-1),
        KeyCode::Esc => app.close_query_menu(),
        KeyCode::Enter | KeyCode::Char(' ') => {
            let choice = app.query_under_cursor();
            app.close_query_menu();
            if app.select_query(choice).is_some() {
                super::fetch_list(app, tx, client);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    use crate::{
        app::{Mode, StatefulTable},
        config::{Config, JqlQuery},
        domain::issue::Issue,
        jira::search::ALL_ISSUES_JQL,
    };

    fn app_and_client() -> (App, JiraClient) {
        let query = |name: &str, jql: &str| JqlQuery {
            name: name.into(),
            jql: jql.into(),
        };
        let mut app = App {
            issues: StatefulTable::new(vec![Issue {
                key: "MAPLE-1".into(),
                ..Default::default()
            }]),
            queries: vec![
                query("Maple", "project = MAPLE"),
                query("Birch", "project = BIRCH"),
            ],
            current_query: Some(0),
            ..Default::default()
        };
        app.list_jql = app.base_jql().to_string();

        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        (app, client)
    }

    fn press(app: &mut App, code: KeyCode, tx: &UnboundedSender<Event>, client: &JiraClient) {
        crate::handlers::handle_key(app, KeyEvent::from(code), tx, client);
    }

    #[tokio::test]
    async fn f_opens_the_menu_and_enter_switches_the_list() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (mut app, client) = app_and_client();

        press(&mut app, KeyCode::Char('f'), &tx, &client);
        assert_eq!(app.mode(), Mode::Queries);
        assert_eq!(app.menu().cursor, 1, "the menu opens on the active query");

        press(&mut app, KeyCode::Char('j'), &tx, &client);
        assert_eq!(app.menu().cursor, 2);
        press(&mut app, KeyCode::Char('k'), &tx, &client);
        assert_eq!(app.menu().cursor, 1);

        // Esc leaves the menu without touching the list
        press(&mut app, KeyCode::Esc, &tx, &client);
        assert!(app.query_menu.is_none());
        assert_eq!(app.list_jql, "project = MAPLE");
        assert!(!app.loading, "and costs no request");

        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Char('j'), &tx, &client);
        press(&mut app, KeyCode::Enter, &tx, &client);
        assert!(app.query_menu.is_none(), "Enter closes the menu");
        assert_eq!(app.current_query, Some(1));
        assert_eq!(app.list_jql, "project = BIRCH");
        assert!(app.loading, "and the new list is fetched");

        // Enter on the row already showing is not a request
        app.loading = false;
        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Enter, &tx, &client);
        assert!(!app.loading);
        assert_eq!(app.list_jql, "project = BIRCH");

        // Space applies the row under the cursor, the same as Enter
        app.loading = false;
        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Char('k'), &tx, &client);
        press(&mut app, KeyCode::Char(' '), &tx, &client);
        assert!(app.query_menu.is_none(), "Space closes the menu too");
        assert_eq!(app.current_query, Some(0));
        assert_eq!(app.list_jql, "project = MAPLE");
        assert!(app.loading);

        // the top row is the built-in list
        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Char('k'), &tx, &client);
        press(&mut app, KeyCode::Enter, &tx, &client);
        assert_eq!(app.current_query, None);
        assert_eq!(app.list_jql, ALL_ISSUES_JQL);
    }
}
