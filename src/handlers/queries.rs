use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, FormFocus, QueryForm},
    event::Event,
    jira::client::JiraClient,
};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    if app.query_form_open() {
        return handle_form_key(app, key, tx, client);
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_query_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_query_cursor(-1),
        KeyCode::Char('a') => app.start_query_form(false),
        KeyCode::Char('e') => app.start_query_form(true),
        KeyCode::Char('d') => app.ask_delete_query(),
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

fn handle_form_key(app: &mut App, key: KeyEvent, tx: &UnboundedSender<Event>, client: &JiraClient) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if !app.query_form_typing() {
        match key.code {
            KeyCode::Enter => form(app).start_edit(),
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('k') | KeyCode::Up => {
                form(app).toggle_focus();
            }
            KeyCode::Esc => app.close_query_form(),
            _ => {}
        }
        return;
    }

    let long_text = form(app).focus == FormFocus::Jql;

    match key.code {
        KeyCode::Char('s') if ctrl => {
            form(app).finish_edit();
            submit(app, tx, client);
        }
        KeyCode::Enter if !long_text => {
            form(app).finish_edit();
            submit(app, tx, client);
        }
        KeyCode::Esc => form(app).cancel_edit(),
        _ => {
            let form = form(app);
            let input = form.focused_input();
            match key.code {
                KeyCode::Char('w') if ctrl => input.delete_word(),
                KeyCode::Char('u') if ctrl => input.clear(),
                KeyCode::Enter => input.insert('\n'),
                KeyCode::Backspace => input.backspace(),
                KeyCode::Up => input.move_up(),
                KeyCode::Down => input.move_down(),
                KeyCode::Left => input.move_left(),
                KeyCode::Right => input.move_right(),
                KeyCode::Char(c) if !ctrl => input.insert(c),
                _ => return,
            }
            form.error = None;
        }
    }
}

fn form(app: &mut App) -> &mut QueryForm {
    app.query_form_mut().expect("the form is open")
}

fn submit(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    let (name, jql) = {
        let form = form(app);
        (
            form.name.buffer.trim().to_string(),
            form.jql.buffer.trim().to_string(),
        )
    };
    match app.submit_query_form() {
        Some(_) => super::fetch_list(app, tx, client),
        None if app.query_form().is_some_and(|form| form.error.is_none()) => {
            crate::fetch::spawn_check_jql(client.clone(), tx.clone(), name, jql);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use crate::{
        app::{DeleteTarget, Mode, StatefulTable},
        config::{Config, JqlQuery},
        domain::issue::Issue,
        jira::search::ALL_ISSUES_JQL,
    };

    // deleting or saving a query rewrites the config file, so it has to exist
    fn app_and_client() -> (App, JiraClient, TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "base_url = \"https://maple.atlassian.net\"\n")
            .expect("write config");

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
            config_path: path,
            ..Default::default()
        };
        app.list_jql = app.base_jql().to_string();

        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        (app, client, dir)
    }

    fn press(app: &mut App, code: KeyCode, tx: &UnboundedSender<Event>, client: &JiraClient) {
        crate::handlers::handle_key(app, KeyEvent::from(code), tx, client);
    }

    fn press_ctrl(app: &mut App, c: char, tx: &UnboundedSender<Event>, client: &JiraClient) {
        let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        crate::handlers::handle_key(app, key, tx, client);
    }

    fn type_text(app: &mut App, text: &str, tx: &UnboundedSender<Event>, client: &JiraClient) {
        for c in text.chars() {
            press(app, KeyCode::Char(c), tx, client);
        }
    }

    #[tokio::test]
    async fn f_opens_the_menu_and_enter_switches_the_list() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (mut app, client, _dir) = app_and_client();

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

    #[tokio::test]
    async fn the_form_edits_one_field_at_a_time_like_the_inspector() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (mut app, client, _dir) = app_and_client();

        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Char('a'), &tx, &client);
        assert!(app.query_form_open(), "a opens an empty form");
        assert!(
            !app.query_form_typing(),
            "with the cursor on a field, not in it"
        );

        // a stray keystroke is not text until Enter says so
        press(&mut app, KeyCode::Char('x'), &tx, &client);
        assert!(app.form().name.buffer.is_empty());

        press(&mut app, KeyCode::Enter, &tx, &client);
        assert!(
            app.query_form_typing(),
            "Enter edits the field under the cursor"
        );
        type_text(&mut app, "q j Oak", &tx, &client);
        assert!(!app.should_quit, "q is a character while a field is open");
        assert_eq!(app.form().name.buffer, "q j Oak");
        assert_eq!(app.menu().cursor, 1, "and j does not move the menu cursor");

        // Enter closes the field and saves - a half-filled query says what it is missing
        press(&mut app, KeyCode::Enter, &tx, &client);
        assert!(!app.query_form_typing());
        assert_eq!(app.form().name.buffer, "q j Oak");
        assert_eq!(app.queries.len(), 2, "nothing is saved yet");
        assert!(
            app.query_form_open(),
            "and the form stays open to be finished"
        );
        let error = app.form().error.as_deref().expect("it says why");
        assert!(error.contains("Query cannot be empty"), "{error}");

        // j reaches the Query field - there is no Tab to switch with
        press(&mut app, KeyCode::Tab, &tx, &client);
        assert_eq!(app.form().focus, FormFocus::Name, "Tab is not a form key");
        press(&mut app, KeyCode::Char('j'), &tx, &client);
        assert_eq!(app.form().focus, FormFocus::Jql);
        press(&mut app, KeyCode::Enter, &tx, &client);
        type_text(&mut app, "project = OAK", &tx, &client);
        press_ctrl(&mut app, 'w', &tx, &client);
        assert_eq!(app.form().jql.buffer, "project = ");
        press(&mut app, KeyCode::Esc, &tx, &client);
        assert!(!app.query_form_typing(), "Esc leaves the field");
        assert!(
            app.form().jql.buffer.is_empty(),
            "and puts the old text back"
        );

        // Ctrl+S outside a field is not a save
        press_ctrl(&mut app, 's', &tx, &client);
        assert_eq!(app.queries.len(), 2);

        press(&mut app, KeyCode::Enter, &tx, &client);
        type_text(&mut app, "project = OAK", &tx, &client);
        assert_eq!(app.form().error, None, "typing clears the complaint");

        // the Query field is long text: Enter breaks the line instead of saving
        press(&mut app, KeyCode::Enter, &tx, &client);
        type_text(&mut app, "AND status != Done", &tx, &client);
        assert!(app.query_form_typing(), "Enter keeps the field open");
        assert_eq!(app.form().jql.buffer, "project = OAK\nAND status != Done");
        assert_eq!(app.queries.len(), 2, "and saves nothing");

        // Up walks back a line rather than leaving the field
        press(&mut app, KeyCode::Up, &tx, &client);
        assert!(app.query_form_typing());
        assert_eq!(app.form().jql.cursor, 13, "the end of the first line");

        // with both fields filled in, Ctrl+S saves the query and stays on the form
        press_ctrl(&mut app, 's', &tx, &client);
        assert!(
            app.query_form_open(),
            "Ctrl+S saves without leaving the form"
        );
        assert!(!app.query_form_typing(), "the field it saved is closed");
        assert_eq!(app.queries.len(), 3);
        assert_eq!(app.queries[2].name, "q j Oak");
        assert_eq!(app.queries[2].jql, "project = OAK\nAND status != Done");
        assert_eq!(
            Config::read(&app.config_path).expect("read config").queries,
            app.queries
        );

        // a second save on the same text saves the same query, it does not add another
        press(&mut app, KeyCode::Enter, &tx, &client);
        press_ctrl(&mut app, 's', &tx, &client);
        assert_eq!(app.queries.len(), 3);
        assert_eq!(app.form().error, None);

        // Esc is the way back to the list of queries
        press(&mut app, KeyCode::Esc, &tx, &client);
        assert!(!app.query_form_open(), "Esc leaves the form");
        assert!(app.query_menu.is_some(), "onto the menu it came from");

        // e opens the row under the cursor filled in, and Esc unwinds one layer at a time
        press(&mut app, KeyCode::Char('e'), &tx, &client);
        let form = app.form();
        assert_eq!(form.name.buffer, "q j Oak");
        assert_eq!(form.jql.buffer, "project = OAK\nAND status != Done");
        assert_eq!(form.editing, Some(2));

        press(&mut app, KeyCode::Esc, &tx, &client);
        assert!(!app.query_form_open(), "Esc leaves the form");
        press(&mut app, KeyCode::Esc, &tx, &client);
        assert!(app.query_menu.is_none(), "and the next Esc leaves the menu");
        assert_eq!(app.queries.len(), 3, "a cancelled form changes nothing");
    }

    #[tokio::test]
    async fn d_asks_before_deleting_a_query() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let (mut app, client, _dir) = app_and_client();

        // the built-in list has nothing to delete
        press(&mut app, KeyCode::Char('f'), &tx, &client);
        press(&mut app, KeyCode::Char('k'), &tx, &client);
        press(&mut app, KeyCode::Char('d'), &tx, &client);
        assert!(app.confirm_delete.is_none());

        press(&mut app, KeyCode::Char('j'), &tx, &client);
        press(&mut app, KeyCode::Char('d'), &tx, &client);
        assert_eq!(
            app.confirm_delete.as_ref().map(|dialog| &dialog.target),
            Some(&DeleteTarget::Query(0)),
            "d asks first"
        );
        assert_eq!(app.queries.len(), 2, "the dialog deletes nothing yet");

        press(&mut app, KeyCode::Char('y'), &tx, &client);
        assert_eq!(app.queries.len(), 1);
        assert_eq!(app.queries[0].name, "Birch");
        assert!(
            app.query_menu.is_some(),
            "the menu stays open behind the dialog"
        );
        assert_eq!(
            app.menu().cursor,
            1,
            "the cursor clamps to the shorter list"
        );
        assert_eq!(
            app.current_query, None,
            "the deleted query was the one on screen"
        );
        assert_eq!(app.list_jql, ALL_ISSUES_JQL);
        assert!(app.loading, "so the built-in list is fetched");
        assert_eq!(
            Config::read(&app.config_path).expect("read config").queries,
            app.queries
        );
    }
}
