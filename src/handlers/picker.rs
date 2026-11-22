use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::App, event::Event, jira::client::JiraClient};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let Some(picker) = &app.picker else {
        return;
    };
    let (searching, multi, has_filter) = (picker.searching, picker.multi, picker.has_filter());

    match key.code {
        KeyCode::Enter => return confirm(app, tx, client),
        KeyCode::Char(' ') if !searching && !multi => return confirm(app, tx, client),
        KeyCode::Esc if !searching && !has_filter => return app.close_picker(),
        _ => {}
    }

    let Some(picker) = &mut app.picker else {
        return;
    };
    if searching {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => picker.clear_filter(),
            KeyCode::Backspace => picker.pop_query(),
            KeyCode::Down => picker.move_down(),
            KeyCode::Up => picker.move_up(),
            KeyCode::Char(c) if !ctrl => picker.push_query(c),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Esc => picker.clear_filter(),
            KeyCode::Char('/') => picker.start_search(),
            KeyCode::Char('j') | KeyCode::Down => picker.move_down(),
            KeyCode::Char('k') | KeyCode::Up => picker.move_up(),
            KeyCode::Char(' ') => picker.toggle(),
            KeyCode::Backspace => picker.pop_query(),
            _ => {}
        }
    }
}

fn confirm(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    if let Some(change) = app.confirm_picker() {
        super::save_change(app, tx, client, change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::mpsc;

    use crate::{
        app::{PickerOption, StatefulTable},
        config::Config,
        domain::issue::{FieldId, Issue},
    };

    fn app_with_picker_open(field: FieldId) -> (App, JiraClient) {
        let issue = Issue {
            key: "MAPLE-1".into(),
            summary: "maple".into(),
            status: "To Do".into(),
            status_category: "new".into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue]),
            ..Default::default()
        };
        app.open_inspector();
        app.selected_field = field;
        app.open_picker().expect("the field is pickable");

        let client = JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        });
        (app, client)
    }

    #[tokio::test]
    async fn esc_unwinds_the_picker_one_layer_at_a_time() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let esc = KeyEvent::from(KeyCode::Esc);

        // Esc is how you leave the filter, so it must work with nothing typed yet
        let (mut app, client) = app_with_picker_open(FieldId::Status);
        app.picker_mut().start_search();
        handle_key(&mut app, esc, &tx, &client);
        let picker = app
            .picker
            .as_ref()
            .expect("Esc out of an empty filter must not close the picker");
        assert!(!picker.searching, "Esc leaves search mode");

        // with text typed, Esc clears it and still holds the picker open
        let (mut app, client) = app_with_picker_open(FieldId::Status);
        app.picker_mut().start_search();
        app.picker_mut().push_query('d');
        handle_key(&mut app, esc, &tx, &client);
        let picker = app
            .picker
            .as_ref()
            .expect("clearing a filter is not closing the picker");
        assert!(picker.query.is_empty(), "Esc clears the filter");
        assert!(!picker.searching);

        // only an Esc with no filter and no search left to unwind closes it
        handle_key(&mut app, esc, &tx, &client);
        assert!(app.picker.is_none(), "the next Esc closes the picker");
    }

    #[tokio::test]
    async fn space_toggles_a_row_but_is_a_character_inside_the_filter() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let space = KeyEvent::from(KeyCode::Char(' '));

        let (mut app, client) = app_with_picker_open(FieldId::Labels);
        app.set_picker_options(
            FieldId::Labels,
            vec![PickerOption {
                id: "maple".into(),
                label: "maple".into(),
                target: None,
            }],
        );

        // outside the filter, Space ticks the row under the cursor
        handle_key(&mut app, space, &tx, &client);
        assert!(
            app.picker
                .as_ref()
                .expect("a multi-select picker stays open")
                .selected
                .contains("maple"),
            "Space ticks a multi-select row"
        );

        // inside the filter, the very same key is text
        app.picker_mut().start_search();
        handle_key(&mut app, space, &tx, &client);
        assert_eq!(
            app.picker.as_ref().expect("still open").query,
            " ",
            "Space must type into the filter, not toggle"
        );
    }
}
