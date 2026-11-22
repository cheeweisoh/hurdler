use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::App, domain::issue::FieldKind, event::Event, jira::client::JiraClient};

pub(super) fn handle_key(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<Event>,
    client: &JiraClient,
) {
    let long_text = app
        .editing
        .as_ref()
        .is_some_and(|edit| app.field_kind(edit.field) == FieldKind::LongText);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('s') if ctrl => confirm(app, tx, client),
        KeyCode::Enter if long_text => app.edit_insert('\n'),
        KeyCode::Enter => confirm(app, tx, client),
        KeyCode::Esc => app.cancel_edit(),
        KeyCode::Backspace => app.edit_backspace(),
        KeyCode::Down => app.edit_move_down(),
        KeyCode::Up => app.edit_move_up(),
        KeyCode::Left => app.edit_move_left(),
        KeyCode::Right => app.edit_move_right(),
        KeyCode::Char('e') if ctrl => app.request_external_edit(),
        KeyCode::Char(c) if !ctrl => app.edit_insert(c),
        _ => {}
    }
}

fn confirm(app: &mut App, tx: &UnboundedSender<Event>, client: &JiraClient) {
    if let Some(change) = app.confirm_edit() {
        super::save_change(app, tx, client, change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crossterm::event::KeyModifiers;
    use tokio::sync::mpsc;

    use crate::{
        app::StatefulTable,
        config::Config,
        domain::issue::{FieldId, Issue},
    };

    fn client() -> JiraClient {
        JiraClient::new(Config {
            base_url: "http://localhost".into(),
            ..Default::default()
        })
    }

    fn app_editing(field: FieldId) -> App {
        let issue = Issue {
            key: "MAPLE-1".into(),
            summary: "maple".into(),
            description: "maple notes".into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue]),
            ..Default::default()
        };
        app.open_inspector();
        app.selected_field = field;
        app.start_edit();
        assert!(app.editing.is_some(), "{field:?} has to be editable");
        app
    }

    #[tokio::test]
    async fn enter_inserts_a_newline_in_a_long_text_edit() {
        let (client, (tx, _rx)) = (client(), mpsc::unbounded_channel());
        let press = |app: &mut App, key: KeyEvent| super::super::handle_key(app, key, &tx, &client);
        let enter = KeyEvent::from(KeyCode::Enter);
        let ctrl_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);

        // a Description is multi-line, so Enter is a line break, not a save
        let mut app = app_editing(FieldId::Description);
        press(&mut app, enter);
        press(&mut app, enter);
        let edit = app
            .editing
            .as_ref()
            .expect("Enter must not close a long-text edit");
        assert_eq!(
            edit.input.buffer,
            "maple notes

"
        );

        // Ctrl-S is the only way out of one
        press(&mut app, ctrl_s);
        assert!(app.editing.is_none(), "Ctrl-S saves a long-text edit");

        // a Summary is one line, so Enter saves it
        let mut app = app_editing(FieldId::Summary);
        press(&mut app, enter);
        assert!(app.editing.is_none(), "Enter saves a single-line edit");
        assert_eq!(
            app.issues.items[0].summary, "maple",
            "an unedited buffer changes nothing"
        );

        // and Ctrl-S works there too
        let mut app = app_editing(FieldId::Summary);
        press(&mut app, ctrl_s);
        assert!(app.editing.is_none());
    }
}
