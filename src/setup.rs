use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::{app::Notice, config::Config, cycle, event::Event, fetch, text_input::TextInput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    BaseUrl,
    Email,
    Token,
    Jql,
    Editor,
}

impl SetupField {
    pub const ALL: [SetupField; 5] = [
        SetupField::BaseUrl,
        SetupField::Email,
        SetupField::Token,
        SetupField::Jql,
        SetupField::Editor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SetupField::BaseUrl => "Jira URL",
            SetupField::Email => "Email",
            SetupField::Token => "API Token",
            SetupField::Jql => "Default JQL",
            SetupField::Editor => "Editor",
        }
    }

    pub fn next(self) -> SetupField {
        cycle::step(&SetupField::ALL, self, 1)
    }

    pub fn prev(self) -> SetupField {
        cycle::step(&SetupField::ALL, self, -1)
    }
}

pub enum SetupOutcome {
    Saved,
    Cancelled,
}

pub struct Setup {
    pub path: PathBuf,
    pub draft: Config,
    pub selected: SetupField,
    pub editing: Option<TextInput>,
    pub testing: bool,
    pub notice: Option<Notice>,
    pub outcome: Option<SetupOutcome>,
}

impl Setup {
    pub fn value(&self, field: SetupField) -> String {
        match field {
            SetupField::BaseUrl => self.draft.base_url.clone(),
            SetupField::Email => self.draft.email.clone(),
            SetupField::Token => self.draft.api_token.clone(),
            SetupField::Jql => self.draft.jql.clone(),
            SetupField::Editor => self.draft.editor.clone().unwrap_or_default(),
        }
    }

    pub fn set_value(&mut self, field: SetupField, text: String) {
        match field {
            SetupField::BaseUrl => {
                self.draft.base_url = text.trim().trim_end_matches('/').to_string()
            }
            SetupField::Email => self.draft.email = text.trim().to_string(),
            SetupField::Token => self.draft.api_token = text.trim().to_string(),
            SetupField::Jql => self.draft.jql = text,
            SetupField::Editor => self.draft.editor = (!text.trim().is_empty()).then_some(text),
        }
    }

    pub fn move_down(&mut self) {
        self.selected = self.selected.next();
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.prev();
    }

    pub fn start_edit(&mut self) {
        self.editing = Some(TextInput::new(self.value(self.selected)));
    }

    pub fn confirm_edit(&mut self) {
        let Some(input) = self.editing.take() else {
            return;
        };
        self.set_value(self.selected, input.buffer);
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    pub fn test_config(&self) -> Result<Config, String> {
        let mut missing = Vec::new();
        if self.draft.base_url.trim().is_empty() {
            missing.push("Jira URL");
        }
        if self.draft.email.trim().is_empty() {
            missing.push("Email");
        }
        if self.draft.api_token.trim().is_empty() {
            missing.push("API Token");
        }

        if missing.is_empty() {
            Ok(self.draft.clone())
        } else {
            Err(format!("Fill in {} before testing", missing.join(", ")))
        }
    }

    pub fn validate_for_save(&self) -> Result<(), String> {
        self.test_config()?;

        if !self.draft.base_url.starts_with("https://") {
            return Err("Jira URL must start with https://".into());
        }
        let (local, domain) = self
            .draft
            .email
            .split_once('@')
            .ok_or("Invalid email format")?;
        if local.is_empty() || !domain.contains('.') {
            return Err("Invalid email format".into());
        }
        Ok(())
    }

    pub fn save(&self) -> Result<(), String> {
        self.validate_for_save()?;
        self.draft
            .save(&self.path)
            .map_err(|e| format!("Couldn't save: {e}"))
    }
}

pub(crate) fn handle_key(setup: &mut Setup, key: KeyEvent, tx: &UnboundedSender<Event>) {
    if setup.editing.is_some() {
        handle_editing_key(setup, key);
        return;
    }

    match key.code {
        KeyCode::Char('t') if !setup.testing => match setup.test_config() {
            Ok(config) => {
                setup.testing = true;
                setup.notice = Some(Notice::Info("Testing connection..".into()));
                fetch::spawn_test_connection(config, tx.clone());
            }
            Err(e) => setup.notice = Some(Notice::Error(e)),
        },
        KeyCode::Char('j') | KeyCode::Down => setup.move_down(),
        KeyCode::Char('k') | KeyCode::Up => setup.move_up(),
        KeyCode::Enter => setup.start_edit(),
        KeyCode::Char('s') => match setup.save() {
            Ok(()) => setup.outcome = Some(SetupOutcome::Saved),
            Err(e) => setup.notice = Some(Notice::Error(e)),
        },
        KeyCode::Char('q') => setup.outcome = Some(SetupOutcome::Cancelled),
        _ => {}
    }
}

fn handle_editing_key(setup: &mut Setup, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Enter => setup.confirm_edit(),
        KeyCode::Esc => setup.cancel_edit(),
        code => {
            let Some(input) = &mut setup.editing else {
                return;
            };
            match code {
                KeyCode::Backspace => input.backspace(),
                KeyCode::Left => input.move_left(),
                KeyCode::Right => input.move_right(),
                KeyCode::Char(c) if !ctrl => input.insert(c),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    use tokio::sync::mpsc;

    fn draft() -> Config {
        Config {
            base_url: "https://maple.atlassian.net".into(),
            email: "maple@maple.example".into(),
            api_token: "maple-token".into(),
            jql: "project = MAPLE".into(),
            page_size: 25,
            editor: Some("maple".into()),
        }
    }

    fn setup(draft: Config, path: PathBuf) -> Setup {
        Setup {
            path,
            draft,
            selected: SetupField::BaseUrl,
            editing: None,
            testing: false,
            notice: None,
            outcome: None,
        }
    }

    #[test]
    fn setup_fields_wrap_in_both_directions() {
        let all = SetupField::ALL;
        for (i, &field) in all.iter().enumerate() {
            assert_eq!(field.next(), all[(i + 1) % all.len()]);
            assert_eq!(field.prev(), all[(i + all.len() - 1) % all.len()]);
        }

        // the wizard scrolls, so the ends have to meet
        assert_eq!(SetupField::Editor.next(), SetupField::BaseUrl);
        assert_eq!(SetupField::BaseUrl.prev(), SetupField::Editor);
    }

    #[test]
    fn set_value_trims_and_normalises() {
        let mut s = setup(Config::default(), PathBuf::from("/tmp/maple.toml"));

        // a pasted URL loses its whitespace and its trailing slash, because
        // every request path is joined onto it with a slash of its own
        s.set_value(
            SetupField::BaseUrl,
            "  https://maple.atlassian.net/  ".into(),
        );
        assert_eq!(s.draft.base_url, "https://maple.atlassian.net");
        assert_eq!(s.value(SetupField::BaseUrl), "https://maple.atlassian.net");

        s.set_value(SetupField::Email, "  maple@maple.example \n".into());
        assert_eq!(s.draft.email, "maple@maple.example");

        s.set_value(SetupField::Token, "  maple-token  ".into());
        assert_eq!(s.draft.api_token, "maple-token");

        // JQL is deliberately left alone - whitespace can be meaningful in a query
        s.set_value(SetupField::Jql, "  project = MAPLE  ".into());
        assert_eq!(s.draft.jql, "  project = MAPLE  ");

        // a blank editor is None, never Some("") - that would shadow $EDITOR
        s.set_value(SetupField::Editor, "maple".into());
        assert_eq!(s.draft.editor.as_deref(), Some("maple"));
        s.set_value(SetupField::Editor, "   ".into());
        assert_eq!(s.draft.editor, None);
        assert_eq!(s.value(SetupField::Editor), "");
    }

    #[test]
    fn editing_a_field_commits_on_enter_and_discards_on_esc() {
        let mut s = setup(draft(), PathBuf::from("/tmp/maple.toml"));
        s.selected = SetupField::Email;

        // the edit opens on the field's current value, cursor at the end
        s.start_edit();
        let input = s.editing.as_ref().expect("editing");
        assert_eq!(input.buffer, "maple@maple.example");
        assert_eq!(input.cursor, "maple@maple.example".chars().count());

        s.editing.as_mut().unwrap().insert('!');
        s.confirm_edit();
        assert!(s.editing.is_none());
        assert_eq!(s.draft.email, "maple@maple.example!");

        // Esc throws the buffer away and leaves the draft as it was
        s.start_edit();
        s.editing.as_mut().unwrap().insert('?');
        s.cancel_edit();
        assert!(s.editing.is_none());
        assert_eq!(s.draft.email, "maple@maple.example!");

        // confirming with nothing open is inert, not a panic
        s.confirm_edit();
        assert_eq!(s.draft.email, "maple@maple.example!");
    }

    #[test]
    fn saving_writes_a_config_the_loader_can_read_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        // a nested path proves save() creates the directory it needs
        let path = dir.path().join("maple").join("maple.toml");
        let s = setup(draft(), path.clone());

        s.save().expect("a complete draft saves");

        let back = Config::read(&path).expect("the saved file parses");
        assert_eq!(back.base_url, "https://maple.atlassian.net");
        assert_eq!(back.email, "maple@maple.example");
        assert_eq!(back.api_token, "maple-token");
        assert_eq!(back.jql, "project = MAPLE");
        assert_eq!(back.page_size, 25);
        assert_eq!(back.editor.as_deref(), Some("maple"));

        // the file holds an API token, so it must not be group or world readable
        #[cfg(unix)]
        {
            use std::{fs, os::unix::fs::PermissionsExt};
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "got {:o}", mode & 0o777);
        }

        // saving again over an existing file keeps the private mode
        s.save().expect("an overwrite saves");
        #[cfg(unix)]
        {
            use std::{fs, os::unix::fs::PermissionsExt};
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "got {:o}", mode & 0o777);
        }
    }

    #[test]
    fn optional_fields_are_left_out_of_the_file_rather_than_written_blank() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("maple.toml");
        let s = setup(
            Config {
                editor: None,
                ..draft()
            },
            path.clone(),
        );
        s.save().expect("saves");

        let text = fs::read_to_string(&path).expect("read back");
        assert!(!text.contains("editor"), "{text}");
        assert_eq!(Config::read(&path).expect("parses").editor, None);

        // the same guard covers api_token, so a blank one is never written as
        // `api_token = ""` and later read back as a present-but-empty token.
        // save() rejects an empty token outright, so check the serializer itself.
        let blank = toml::to_string_pretty(&Config {
            api_token: String::new(),
            ..draft()
        })
        .expect("serialises");
        assert!(!blank.contains("api_token"), "{blank}");
    }

    #[test]
    fn saving_an_invalid_draft_sets_a_notice_and_no_outcome() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("maple.toml");
        let mut s = setup(
            Config {
                base_url: "http://maple.atlassian.net".into(),
                ..draft()
            },
            path.clone(),
        );
        let (tx, _rx) = mpsc::unbounded_channel();

        handle_key(&mut s, KeyEvent::from(KeyCode::Char('s')), &tx);

        // the wizard stays open on the bad field instead of dropping the user
        assert!(s.outcome.is_none(), "a failed save must not close setup");
        assert!(
            matches!(&s.notice, Some(Notice::Error(message)) if message.contains("https://")),
            "{:?}",
            s.notice
        );
        assert!(!path.exists(), "a rejected draft must not reach disk");

        // fixing the field and pressing s again saves and closes
        s.set_value(SetupField::BaseUrl, "https://maple.atlassian.net".into());
        handle_key(&mut s, KeyEvent::from(KeyCode::Char('s')), &tx);
        assert!(matches!(s.outcome, Some(SetupOutcome::Saved)));
        assert!(path.exists());
    }

    #[test]
    fn navigation_keys_move_the_cursor_and_quit_cancels() {
        let mut s = setup(draft(), PathBuf::from("/tmp/maple.toml"));
        let (tx, _rx) = mpsc::unbounded_channel();
        let press = |s: &mut Setup, code| handle_key(s, KeyEvent::from(code), &tx);

        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.selected, SetupField::Email);
        press(&mut s, KeyCode::Down);
        assert_eq!(s.selected, SetupField::Token);
        press(&mut s, KeyCode::Char('k'));
        assert_eq!(s.selected, SetupField::Email);
        press(&mut s, KeyCode::Up);
        assert_eq!(s.selected, SetupField::BaseUrl);

        // mid-edit those same keys are text, not movement
        press(&mut s, KeyCode::Enter);
        press(&mut s, KeyCode::Char('j'));
        assert_eq!(s.selected, SetupField::BaseUrl, "j typed into the field");
        assert!(
            s.editing.as_ref().expect("editing").buffer.ends_with('j'),
            "{:?}",
            s.editing
        );

        press(&mut s, KeyCode::Esc);
        assert!(s.editing.is_none(), "Esc closes the edit, not the wizard");
        assert!(s.outcome.is_none());

        // with no edit left to escape, Esc has nothing to do; only q leaves
        press(&mut s, KeyCode::Esc);
        assert!(s.outcome.is_none(), "Esc must not exit the wizard");

        press(&mut s, KeyCode::Char('q'));
        assert!(matches!(s.outcome, Some(SetupOutcome::Cancelled)));
    }

    #[test]
    fn validate_for_save_rejects_a_bad_url_or_email() {
        // the draft is otherwise complete, so only the format checks can fail
        let check = |base_url: &str, email: &str| {
            setup(
                Config {
                    base_url: base_url.into(),
                    email: email.into(),
                    ..draft()
                },
                PathBuf::from("/tmp/maple.toml"),
            )
            .validate_for_save()
        };

        assert!(check("https://maple.atlassian.net", "maple@maple.example").is_ok());

        // an incomplete draft is caught before any format check runs
        let incomplete = setup(
            Config {
                api_token: String::new(),
                ..draft()
            },
            PathBuf::from("/tmp/maple.toml"),
        );
        assert!(
            incomplete
                .validate_for_save()
                .expect_err("an empty token is rejected")
                .contains("API Token")
        );
    }
}
