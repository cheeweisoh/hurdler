use std::{
    collections::{BTreeSet, HashSet},
    fmt, io,
    time::{Duration, Instant},
};

use chrono::{Days, Months, NaiveDate};
use ratatui::widgets::TableState;

use crate::{
    cycle,
    domain::issue::{DATE_FORMAT, FieldChange, FieldId, FieldKind, FieldValue, Issue, User},
    jira::search::search_jql,
    text_input::TextInput,
};

const NOTICE_DURATION: Duration = Duration::from_secs(3);
const SEARCH_DELAY: Duration = Duration::from_millis(250);

pub const DRAFT_KEY: &str = "NEW";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Inspect,
    Create,
    Edit,
    Pick,
    Confirm,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Mode::Normal => "Normal",
            Mode::Search => "Search",
            Mode::Inspect => "Inspect",
            Mode::Create => "Create",
            Mode::Edit => "Edit",
            Mode::Pick => "Pick",
            Mode::Confirm => "Confirm",
        };
        write!(f, "{name}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Issues,
}

impl Tab {
    pub const ALL: [Tab; 1] = [Tab::Issues];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Issues => "Issues",
        }
    }

    pub fn next(self) -> Tab {
        cycle::step(&Tab::ALL, self, 1)
    }

    pub fn prev(self) -> Tab {
        cycle::step(&Tab::ALL, self, -1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    List,
    Inspector,
}

#[derive(Debug, Default)]
pub struct StatefulTable<T> {
    pub items: Vec<T>,
    pub state: TableState,
}

impl<T> StatefulTable<T> {
    pub fn new(items: Vec<T>) -> Self {
        let mut state = TableState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self { items, state }
    }

    pub fn selected(&self) -> Option<&T> {
        self.state.selected().and_then(|i| self.items.get(i))
    }

    pub fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) => (i + 1) % self.items.len(),
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(0) | None => self.items.len() - 1,
            Some(i) => i - 1,
        };
        self.state.select(Some(i));
    }

    pub fn first(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.state.select(Some(0));
    }

    pub fn last(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.state.select(Some(self.items.len() - 1));
    }
}

#[derive(Debug, Default)]
pub struct Search {
    pub input: TextInput,
    pub editing: bool,
    pub changed_at: Option<Instant>,
    pub active_query: String,
    pub stash: Option<Stash>,
}

#[derive(Debug, Default)]
pub struct Stash {
    pub issues: Vec<Issue>,
    pub selected: Option<usize>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditState {
    pub issue_key: String,
    pub field: FieldId,
    pub input: TextInput,
}

#[derive(Debug, Clone)]
pub enum Notice {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PickerOption {
    pub id: String,
    pub label: String,
    pub target: Option<FieldValue>,
}

#[derive(Debug, Clone)]
pub struct OptionsRequest {
    pub field: FieldId,
    pub issue_key: String,
    pub project_key: String,
    pub jql: Option<String>,
}

#[derive(Debug)]
pub struct ConfirmDelete {
    pub key: String,
    pub confirm_focused: bool,
}

#[derive(Debug)]
pub struct Picker {
    pub issue_key: String,
    pub field: FieldId,
    pub multi: bool,
    pub options: Vec<PickerOption>,
    pub selected: BTreeSet<String>,
    pub current: Option<FieldValue>,
    pub query: String,
    pub cursor: usize,
    pub loading: bool,
    pub searching: bool,
}

impl Picker {
    pub fn start_search(&mut self) {
        self.searching = true;
    }

    pub fn clear_filter(&mut self) {
        self.searching = false;
        self.query.clear();
        self.cursor = 0;
    }

    pub fn has_filter(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn push_query(&mut self, c: char) {
        self.query.push(c);
        self.cursor = 0;
    }

    pub fn pop_query(&mut self) {
        if self.query.pop().is_some() {
            self.cursor = 0;
        }
    }

    pub fn filtered(&self) -> Vec<&PickerOption> {
        let query = self.query.to_lowercase();
        self.options
            .iter()
            .filter(|option| option.label.to_lowercase().contains(&query))
            .collect()
    }

    pub fn move_down(&mut self) {
        let len = self.filtered().len();
        if len > 0 {
            self.cursor = (self.cursor + 1) % len;
        }
    }

    pub fn move_up(&mut self) {
        let len = self.filtered().len();
        if len > 0 {
            self.cursor = self.cursor.checked_sub(1).unwrap_or(len - 1);
        }
    }

    pub fn toggle(&mut self) {
        if !self.multi {
            return;
        }
        let Some(id) = self
            .filtered()
            .get(self.cursor)
            .map(|option| option.id.clone())
        else {
            return;
        };
        if !self.selected.remove(&id) {
            self.selected.insert(id);
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatePicker {
    pub issue_key: String,
    pub field: FieldId,
    pub date: NaiveDate,
}

impl DatePicker {
    pub fn move_days(&mut self, n: i64) {
        let moved = if n >= 0 {
            self.date.checked_add_days(Days::new(n as u64))
        } else {
            self.date.checked_sub_days(Days::new(n.unsigned_abs()))
        };
        if let Some(date) = moved {
            self.date = date;
        }
    }

    pub fn move_months(&mut self, n: i32) {
        let moved = if n >= 0 {
            self.date.checked_add_months(Months::new(n as u32))
        } else {
            self.date.checked_sub_months(Months::new(n.unsigned_abs()))
        };
        if let Some(date) = moved {
            self.date = date;
        }
    }
}

#[derive(Debug, Default)]
pub struct App {
    pub me: Option<User>,
    pub account: String,
    pub issues: StatefulTable<Issue>,

    pub view: View,
    pub active_tab: Tab,
    pub search: Option<Search>,
    pub list_jql: String,
    pub next_page_token: Option<String>,
    pub loading_more: bool,
    pub fetch_generation: u64,
    pub draft: Option<Issue>,
    pub creating: bool,
    pub selected_field: FieldId,
    pub details_scroll: u16,
    pub picker: Option<Picker>,
    pub date_picker: Option<DatePicker>,
    pub confirm_delete: Option<ConfirmDelete>,
    pub editing: Option<EditState>,
    pub external_edit: Option<EditState>,

    pub recent_assignees: Vec<String>,
    pub recent_reporters: Vec<String>,

    pub pending_key: Option<(char, Instant)>,

    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<(Notice, Instant)>,

    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    const LOAD_MORE_WIDTH: usize = 5;

    pub fn new() -> Self {
        Self {
            loading: true,
            ..Default::default()
        }
    }

    pub fn mode(&self) -> Mode {
        if self.confirm_delete.is_some() {
            Mode::Confirm
        } else if self.picker.is_some() {
            Mode::Pick
        } else if self.search_editing() {
            Mode::Search
        } else if self.editing.is_some() {
            Mode::Edit
        } else if self.draft.is_some() {
            Mode::Create
        } else if self.view == View::Inspector {
            Mode::Inspect
        } else {
            Mode::Normal
        }
    }

    pub fn display_name(&self) -> &str {
        self.me
            .as_ref()
            .map_or(self.account.as_str(), |user| user.display_name.as_str())
    }

    pub fn search_editing(&self) -> bool {
        self.search.as_ref().is_some_and(|search| search.editing)
    }

    pub fn search_query(&self) -> Option<&str> {
        self.search
            .as_ref()
            .map(|search| search.active_query.as_str())
            .filter(|query| !query.is_empty())
    }

    pub fn open_search(&mut self) {
        self.search.get_or_insert_with(Search::default).editing = true;
    }

    pub fn search_edited(&mut self) {
        if let Some(search) = &mut self.search {
            search.changed_at = Some(Instant::now());
        }
    }

    pub fn close_search_input(&mut self) {
        if let Some(search) = &mut self.search {
            search.editing = false;
        }
    }

    pub fn clear_search(&mut self, base_jql: &str) -> bool {
        self.list_jql = base_jql.to_string();
        let Some(search) = self.search.take() else {
            return false;
        };
        if search.active_query.is_empty() {
            return false;
        }
        let Some(stash) = search.stash else {
            return true;
        };

        self.fetch_generation += 1;
        self.loading = false;
        self.loading_more = false;
        self.issues = StatefulTable::new(stash.issues);
        self.issues.state.select(stash.selected);
        self.next_page_token = stash.next_page_token;
        self.details_scroll = 0;
        self.error = None;
        false
    }

    pub fn due_search(&mut self, base_jql: &str) -> Option<String> {
        let search = self.search.as_mut()?;
        let changed_at = search.changed_at?;
        if search.editing && changed_at.elapsed() < SEARCH_DELAY {
            return None;
        }
        search.changed_at = None;

        let query = search.input.buffer.trim().to_string();
        if query == search.active_query {
            return None;
        }
        let first = search.active_query.is_empty();
        search.active_query = query.clone();

        if first {
            let stash = Stash {
                issues: self.issues.items.clone(),
                selected: self.issues.state.selected(),
                next_page_token: self.next_page_token.clone(),
            };
            if let Some(search) = &mut self.search {
                search.stash = Some(stash);
            }
        }

        let jql = search_jql(base_jql, &query).unwrap_or_else(|| base_jql.to_string());
        self.list_jql = jql.clone();
        Some(jql)
    }

    pub fn begin_refresh(&mut self) -> u64 {
        self.fetch_generation += 1;
        self.loading = true;
        self.loading_more = false;
        self.next_page_token = None;
        self.fetch_generation
    }

    pub fn finish_refresh(
        &mut self,
        issues: Vec<Issue>,
        next_page_token: Option<String>,
        generation: u64,
    ) {
        if generation != self.fetch_generation {
            return;
        }
        self.issues = StatefulTable::new(issues);
        self.next_page_token = next_page_token;
        self.loading = false;
        self.error = None;
        self.details_scroll = 0;
    }

    pub fn fail_refresh(&mut self, message: String, generation: u64) {
        if generation != self.fetch_generation {
            return;
        }
        self.loading = false;
        self.error = Some(message);
    }

    pub fn request_more(&mut self) -> Option<(String, u64)> {
        if self.loading || self.loading_more {
            return None;
        }
        let token = self.next_page_token.clone()?;
        self.loading_more = true;
        Some((token, self.fetch_generation))
    }

    pub fn append_issues(
        &mut self,
        issues: Vec<Issue>,
        next_page_token: Option<String>,
        generation: u64,
    ) {
        if generation != self.fetch_generation {
            return;
        }
        self.loading_more = false;
        self.next_page_token = next_page_token;

        let existing: HashSet<String> = self
            .issues
            .items
            .iter()
            .map(|issue| issue.key.clone())
            .collect();
        self.issues.items.extend(
            issues
                .into_iter()
                .filter(|issue| !existing.contains(&issue.key)),
        );
    }

    pub fn fail_more(&mut self, message: String, generation: u64) {
        if generation != self.fetch_generation {
            return;
        }
        self.loading_more = false;
        self.set_notice(Notice::Error(format!(
            "Couldn't load more issues: {message}"
        )));
    }

    pub fn near_end(&self) -> bool {
        match self.issues.state.selected() {
            Some(i) => i + Self::LOAD_MORE_WIDTH >= self.issues.items.len(),
            None => false,
        }
    }

    pub fn current_issue(&self) -> Option<&Issue> {
        self.draft.as_ref().or_else(|| self.issues.selected())
    }

    fn issue_mut(&mut self, key: &str) -> Option<&mut Issue> {
        if key == DRAFT_KEY {
            return self.draft.as_mut();
        }
        self.issues.items.iter_mut().find(|issue| issue.key == key)
    }

    pub fn apply_change(
        &mut self,
        issue_key: &str,
        field: FieldId,
        new_value: FieldValue,
        transition_id: Option<String>,
    ) -> Option<FieldChange> {
        let issue = self.issue_mut(issue_key)?;
        let old_value = issue.raw_value(field);
        if old_value == new_value {
            return None;
        }
        issue.set_field_value(field, new_value.clone());
        Some(FieldChange {
            issue_key: issue_key.to_string(),
            field,
            old_value,
            new_value,
            transition_id,
        })
    }

    pub fn is_draft(&self, key: &str) -> bool {
        key == DRAFT_KEY && self.draft.is_some()
    }

    pub fn field_kind(&self, field: FieldId) -> FieldKind {
        field.kind_when(self.draft.is_some())
    }

    pub fn start_draft(&mut self) {
        let project = self
            .issues
            .selected()
            .map(|issue| issue.project.clone())
            .unwrap_or_default();

        self.draft = Some(Issue {
            key: DRAFT_KEY.to_string(),
            project,
            ..Default::default()
        });
        self.view = View::Inspector;
        self.selected_field = FieldId::default();
        self.details_scroll = 0;
    }

    pub fn prepare_create(&mut self) -> Result<Issue, String> {
        if self.creating {
            return Err("Already creating an issue".into());
        }
        let draft = self.draft.as_ref().ok_or("Nothing to create")?;

        if draft.project.is_empty() {
            return Err("No project - open an issue in one first".into());
        }
        if draft.issue_type_id.is_empty() {
            return Err("Choose a type".into());
        }
        if draft.summary.trim().is_empty() {
            return Err("Enter a summary".into());
        }

        let draft = draft.clone();
        self.creating = true;
        Ok(draft)
    }

    pub fn finish_create(&mut self, issue: Issue) {
        let key = issue.key.clone();
        self.draft = None;
        self.creating = false;
        self.issues.items.insert(0, issue);
        self.issues.state.select(Some(0));
        self.view = View::Inspector;
        self.selected_field = FieldId::default();
        self.details_scroll = 0;
        self.set_notice(Notice::Info(format!("Created {key}")));
    }

    pub fn ask_delete(&mut self) {
        if let Some(key) = self.selected_issue_key().filter(|key| !self.is_draft(key)) {
            self.confirm_delete = Some(ConfirmDelete {
                key,
                confirm_focused: false,
            });
        }
    }

    pub fn cancel_delete(&mut self) {
        self.confirm_delete = None;
    }

    pub fn focus_delete_button(&mut self, confirm: bool) {
        if let Some(dialog) = &mut self.confirm_delete {
            dialog.confirm_focused = confirm;
        }
    }

    pub fn toggle_delete_button(&mut self) {
        if let Some(dialog) = &mut self.confirm_delete {
            dialog.confirm_focused = !dialog.confirm_focused;
        }
    }

    pub fn take_delete(&mut self) -> Option<String> {
        self.confirm_delete.take().map(|dialog| dialog.key)
    }

    pub fn press_delete_button(&mut self) -> Option<String> {
        if !self.confirm_delete.as_ref()?.confirm_focused {
            self.confirm_delete = None;
            return None;
        }
        self.take_delete()
    }

    pub fn finish_delete(&mut self, key: &str) {
        let Some(i) = self.issues.items.iter().position(|issue| issue.key == key) else {
            return;
        };
        self.issues.items.remove(i);

        let selected = match self.issues.items.len() {
            0 => None,
            len => Some(i.min(len - 1)),
        };
        self.issues.state.select(selected);

        if self.view == View::Inspector {
            self.close_inspector();
        }
        self.set_notice(Notice::Info(format!("Deleted {key}")));
    }

    pub fn open_inspector(&mut self) {
        if self.issues.selected().is_some() {
            self.view = View::Inspector;
            self.details_scroll = 0;
            self.selected_field = FieldId::default();
        }
    }

    pub fn close_inspector(&mut self) {
        self.view = View::List;
        self.details_scroll = 0;
        self.editing = None;
        self.draft = None;
    }

    pub fn selected_issue_key(&self) -> Option<String> {
        self.current_issue().map(|issue| issue.key.clone())
    }

    pub fn select_next_field(&mut self) {
        let creating = self.draft.is_some();
        self.selected_field = self.selected_field.next_editable(creating);
    }

    pub fn select_prev_field(&mut self) {
        let creating = self.draft.is_some();
        self.selected_field = self.selected_field.prev_editable(creating);
    }

    pub fn open_picker(&mut self) -> Option<OptionsRequest> {
        self.open_picker_for(self.selected_field)
    }

    pub fn open_picker_for(&mut self, field: FieldId) -> Option<OptionsRequest> {
        let kind = self.field_kind(field);
        let issue = self.current_issue()?;

        let (multi, selected, current) = match (kind, issue.raw_value(field)) {
            (FieldKind::SingleSelect, value) => (false, BTreeSet::new(), Some(value)),
            (FieldKind::MultiSelect, FieldValue::List(labels)) => {
                (true, labels.into_iter().collect(), None)
            }
            _ => return None,
        };

        let issue_key = issue.key.clone();
        let project_key = issue.project_key().to_string();
        let jql = (field == FieldId::Parent).then(|| issue.parent_candidates_jql());
        self.picker = Some(Picker {
            issue_key: issue_key.clone(),
            field,
            multi,
            options: Vec::new(),
            selected,
            current,
            query: String::new(),
            cursor: 0,
            loading: true,
            searching: false,
        });
        Some(OptionsRequest {
            field,
            issue_key,
            project_key,
            jql,
        })
    }

    pub fn fail_picker_options(&mut self, message: String) {
        let noun = self
            .picker
            .as_ref()
            .map_or("options", |picker| picker.field.label());
        self.set_notice(Notice::Error(format!("Couldn't load {noun}: {message}")));
        self.picker = None;
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
    }

    pub fn set_picker_options(&mut self, field: FieldId, mut options: Vec<PickerOption>) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        if picker.field != field {
            return;
        }

        for id in &picker.selected {
            if !options.iter().any(|option| &option.id == id) {
                options.push(PickerOption {
                    id: id.clone(),
                    label: id.clone(),
                    target: None,
                });
            }
        }
        let recent: &[String] = match field {
            FieldId::Assignee => &self.recent_assignees,
            FieldId::Reporter => &self.recent_reporters,
            _ => &[],
        };
        let rank = |option: &PickerOption| {
            recent
                .iter()
                .position(|id| *id == option.id)
                .unwrap_or(usize::MAX)
        };
        if !matches!(field, FieldId::Type | FieldId::FixVersions) {
            options.sort_by(|a, b| {
                (!a.id.is_empty(), rank(a), &a.label).cmp(&(!b.id.is_empty(), rank(b), &b.label))
            });
        }

        picker.options = options.clone();
        picker.loading = false;

        if !picker.multi
            && let Some(current) = &picker.current
            && let Some(i) = options
                .iter()
                .position(|o| o.target.as_ref() == Some(current))
        {
            picker.selected.insert(options[i].id.clone());
            picker.cursor = i;
        }
    }

    fn record_recent(&mut self, field: FieldId, id: &str) {
        let recent = match field {
            FieldId::Assignee => &mut self.recent_assignees,
            FieldId::Reporter => &mut self.recent_reporters,
            _ => return,
        };
        if id.is_empty() {
            return;
        }
        recent.retain(|known| known != id);
        recent.insert(0, id.to_string());
    }

    fn confirm_single(&mut self, picker: Picker) -> Option<FieldChange> {
        let option = picker.filtered().get(picker.cursor).map(|o| (*o).clone())?;
        let new_value = option.target?;
        let transition_id = (picker.field == FieldId::Status).then_some(option.id.clone());

        self.record_recent(picker.field, &option.id);
        self.apply_change(&picker.issue_key, picker.field, new_value, transition_id)
    }

    fn confirm_multi(&mut self, picker: Picker) -> Option<FieldChange> {
        let issue = self.issue_mut(&picker.issue_key)?;

        let old_value = issue.raw_value(picker.field);
        let FieldValue::List(old_labels) = &old_value else {
            return None;
        };
        let old_set: BTreeSet<String> = old_labels.iter().cloned().collect();
        if old_set == picker.selected {
            return None;
        }

        let new_value = FieldValue::List(picker.selected.into_iter().collect());
        issue.set_field_value(picker.field, new_value.clone());
        Some(FieldChange {
            issue_key: picker.issue_key,
            field: picker.field,
            old_value,
            new_value,
            transition_id: None,
        })
    }

    pub fn confirm_picker(&mut self) -> Option<FieldChange> {
        let picker = self.picker.take()?;
        if picker.multi {
            self.confirm_multi(picker)
        } else {
            self.confirm_single(picker)
        }
    }

    pub fn open_date_picker(&mut self, today: NaiveDate) {
        let Some(issue) = self.current_issue() else {
            return;
        };
        let field = self.selected_field;
        let FieldValue::Date(current) = issue.raw_value(field) else {
            return;
        };
        self.date_picker = Some(DatePicker {
            issue_key: issue.key.clone(),
            field,
            date: current.unwrap_or(today),
        });
    }

    pub fn close_date_picker(&mut self) {
        self.date_picker = None;
    }

    pub fn confirm_date_picker(&mut self, clear: bool) -> Option<FieldChange> {
        let picker = self.date_picker.take()?;
        let new_value = FieldValue::Date((!clear).then_some(picker.date));

        self.apply_change(&picker.issue_key, picker.field, new_value, None)
    }

    pub fn request_external_edit(&mut self) {
        if let Some(edit) = self.editing.take() {
            self.external_edit = Some(edit);
            return;
        }
        let field = self.selected_field;
        if self.field_kind(field) != FieldKind::LongText {
            return;
        }
        let Some(issue) = self.current_issue() else {
            return;
        };
        self.external_edit = Some(EditState {
            issue_key: issue.key.clone(),
            field,
            input: TextInput::new(issue.edit_text(field)),
        });
    }

    pub fn finish_external_edit(
        &mut self,
        edit: EditState,
        result: io::Result<Option<String>>,
    ) -> Option<FieldChange> {
        let (buffer, save) = match result {
            Ok(Some(text)) => (text, true),
            Ok(None) => {
                self.set_notice(Notice::Info("Editor closed without changes".into()));
                (edit.input.buffer.clone(), false)
            }
            Err(e) => {
                self.set_notice(Notice::Error(format!("Couldn't run your editor: {e}")));
                (edit.input.buffer.clone(), false)
            }
        };
        self.editing = Some(EditState {
            input: TextInput::new(buffer),
            ..edit
        });
        save.then(|| self.confirm_edit()).flatten()
    }

    pub fn start_edit(&mut self) {
        let field = self.selected_field;
        if !matches!(
            self.field_kind(field),
            FieldKind::Text | FieldKind::Date | FieldKind::LongText
        ) {
            return;
        }
        let Some(issue) = self.current_issue() else {
            return;
        };
        let buffer = issue.edit_text(field);
        self.editing = Some(EditState {
            issue_key: issue.key.clone(),
            field,
            input: TextInput::new(buffer),
        });
    }

    pub fn edit_insert(&mut self, c: char) {
        if let Some(edit) = &mut self.editing {
            edit.input.insert(c);
        }
    }

    pub fn edit_backspace(&mut self) {
        if let Some(edit) = &mut self.editing {
            edit.input.backspace();
        }
    }

    pub fn edit_move_down(&mut self) {
        if let Some(edit) = &mut self.editing {
            edit.input.move_down();
        }
    }

    pub fn edit_move_up(&mut self) {
        if let Some(edit) = &mut self.editing {
            edit.input.move_up();
        }
    }

    pub fn edit_move_left(&mut self) {
        if let Some(edit) = &mut self.editing {
            edit.input.move_left();
        }
    }

    pub fn edit_move_right(&mut self) {
        if let Some(edit) = &mut self.editing {
            edit.input.move_right();
        }
    }

    pub fn confirm_edit(&mut self) -> Option<FieldChange> {
        let edit = self.editing.as_ref()?;
        let new_value = match parse_edit(edit.field, &edit.input.buffer) {
            Ok(value) => value,
            Err(message) => {
                self.set_notice(Notice::Error(message));
                return None;
            }
        };

        let edit = self.editing.take()?;
        self.apply_change(&edit.issue_key, edit.field, new_value, None)
    }

    pub fn revert_change(&mut self, change: &FieldChange) {
        let Some(issue) = self.issue_mut(&change.issue_key) else {
            return;
        };

        if issue.raw_value(change.field) == change.new_value {
            issue.set_field_value(change.field, change.old_value.clone());
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    pub fn assign_to_me(&mut self) -> Option<FieldChange> {
        let me = self.me.clone()?;
        let key = self.selected_issue_key()?;
        self.apply_change(&key, FieldId::Assignee, FieldValue::User(Some(me)), None)
    }

    pub fn set_notice(&mut self, notice: Notice) {
        self.notice = Some((notice, Instant::now()));
    }

    pub fn expire_notice(&mut self) {
        if let Some((_, set_at)) = &self.notice
            && set_at.elapsed() > NOTICE_DURATION
        {
            self.notice = None;
        }
    }
}

#[cfg(test)]
impl App {
    pub(crate) fn picker_mut(&mut self) -> &mut Picker {
        self.picker.as_mut().expect("the picker is open")
    }
}

fn parse_edit(field: FieldId, text: &str) -> Result<FieldValue, String> {
    match field.kind() {
        FieldKind::LongText => Ok(FieldValue::Text(text.trim_end().to_string())),
        FieldKind::Date => {
            let text = text.trim();
            if text.is_empty() {
                Ok(FieldValue::Date(None))
            } else {
                NaiveDate::parse_from_str(text, DATE_FORMAT)
                    .map(|date| FieldValue::Date(Some(date)))
                    .map_err(|_| format!("\"{text}\" is not a date - use YYYY-MM-DD"))
            }
        }
        _ => Ok(FieldValue::Text(text.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::issue::{Choice, User};

    #[test]
    fn a_due_date_survives_the_trip_from_edit_text_through_the_parser() {
        let issue = Issue {
            due: NaiveDate::from_ymd_opt(2026, 1, 15),
            ..Default::default()
        };

        assert_eq!(
            parse_edit(FieldId::Due, &issue.edit_text(FieldId::Due)),
            Ok(FieldValue::Date(issue.due)),
            "the edit buffer and parse_edit have to agree on the date format"
        );

        let cleared = Issue::default();
        assert_eq!(cleared.edit_text(FieldId::Due), "");
        assert_eq!(
            parse_edit(FieldId::Due, &cleared.edit_text(FieldId::Due)),
            Ok(FieldValue::Date(None)),
            "an unset due date opens an empty buffer and stays unset"
        );
    }

    impl App {
        fn edit_search(&mut self, c: char) {
            self.search
                .as_mut()
                .expect("the bar is open")
                .input
                .insert(c);
            self.search_edited();
        }
    }

    fn edit(text: &str, cursor: usize) -> EditState {
        EditState {
            issue_key: "MAPLE-1".into(),
            field: FieldId::Summary,
            input: TextInput {
                buffer: text.into(),
                cursor,
                goal_column: None,
            },
        }
    }

    #[test]
    fn a_draft_stands_in_for_the_selected_issue_while_it_is_open() {
        let mut app = app_with_issue();
        app.issues.items[0].project = "MAPLE".into();
        app.open_inspector();
        assert_eq!(app.current_issue().map(|i| i.key.as_str()), Some("MAPLE-1"));

        app.start_draft();
        let draft = app.current_issue().expect("the draft is what is on screen");
        assert_eq!(draft.key, DRAFT_KEY);
        assert_eq!(
            draft.project, "MAPLE",
            "the draft lands in the same project"
        );
        assert_eq!(app.selected_field, FieldId::default());
        assert_eq!(app.mode(), Mode::Create);
        assert!(app.is_draft(DRAFT_KEY));
        assert!(!app.is_draft("MAPLE-1"));

        // edits land on the draft, not on the issue underneath it
        app.selected_field = FieldId::Summary;
        app.start_edit();
        app.edit_insert('h');
        app.edit_insert('i');
        let change = app.confirm_edit().expect("a draft edit is still a change");
        assert_eq!(change.issue_key, DRAFT_KEY);
        assert!(
            app.is_draft(&change.issue_key),
            "so it is never sent to Jira"
        );
        assert_eq!(app.draft.as_ref().map(|d| d.summary.as_str()), Some("hi"));
        assert_eq!(app.issues.items[0].summary, "maple");

        // Esc throws the draft away
        app.close_inspector();
        assert!(app.draft.is_none());
        assert_eq!(app.current_issue().map(|i| i.key.as_str()), Some("MAPLE-1"));
    }

    #[test]
    fn a_draft_picks_its_type_in_jiras_order() {
        let mut app = app_with_issue();
        app.issues.items[0].project = "MAPLE".into();
        app.start_draft();
        app.selected_field = FieldId::Type;

        let request = app.open_picker().expect("Type is pickable on a draft");
        assert_eq!(request.field, FieldId::Type);
        assert_eq!(request.project_key, "MAPLE");

        let types = ["Task", "Bug"].map(|name| Choice {
            id: name.to_lowercase(),
            name: name.into(),
        });
        app.set_picker_options(
            FieldId::Type,
            types
                .iter()
                .map(|choice| PickerOption {
                    id: choice.id.clone(),
                    label: choice.name.clone(),
                    target: Some(FieldValue::Choice(Some(choice.clone()))),
                })
                .collect(),
        );
        assert_eq!(
            option_labels(app.picker.as_ref().expect("the picker is open")),
            ["Task", "Bug"],
            "Jira's own order survives, unlike the name-sorted pickers"
        );

        let change = app.confirm_picker().expect("picking a type is a change");
        assert_eq!(change.issue_key, DRAFT_KEY);
        assert!(change.transition_id.is_none());
        let draft = app.draft.as_ref().expect("the draft is still open");
        assert_eq!(
            (draft.issue_type.as_str(), draft.issue_type_id.as_str()),
            ("Task", "task")
        );
    }

    #[test]
    fn prepare_create_holds_a_draft_jira_would_reject() {
        let mut app = app_with_issue();
        assert_eq!(app.prepare_create().unwrap_err(), "Nothing to create");

        app.issues.items[0].project = "MAPLE".into();
        app.start_draft();
        assert_eq!(app.prepare_create().unwrap_err(), "Choose a type");

        app.draft.as_mut().unwrap().set_field_value(
            FieldId::Type,
            FieldValue::Choice(Some(Choice {
                id: "10001".into(),
                name: "Task".into(),
            })),
        );
        assert_eq!(app.prepare_create().unwrap_err(), "Enter a summary");

        // whitespace is not a summary
        app.draft.as_mut().unwrap().summary = "   ".into();
        assert_eq!(app.prepare_create().unwrap_err(), "Enter a summary");
        assert!(
            !app.creating,
            "a rejected draft must not lock the inspector"
        );
        assert!(app.draft.is_some(), "and must not be thrown away");

        app.draft.as_mut().unwrap().summary = "plant a maple".into();
        let ready = app.prepare_create().expect("a filled-in draft is sent");
        assert_eq!(ready.summary, "plant a maple");
        assert_eq!(ready.issue_type_id, "10001");
        assert_eq!(ready.project, "MAPLE");
        assert!(app.creating);
        assert!(app.draft.is_some(), "the draft stays until Jira answers");

        // a second Ctrl+S while the first is in flight creates nothing
        assert_eq!(
            app.prepare_create().unwrap_err(),
            "Already creating an issue"
        );

        // and with no issue to borrow a project from there is nothing to send
        let mut app = App::default();
        app.start_draft();
        assert!(app.prepare_create().is_err());
    }

    #[test]
    fn a_created_issue_replaces_the_draft_and_tops_the_list() {
        let mut app = app_with_issue();
        app.start_draft();
        app.creating = true;

        app.finish_create(Issue {
            key: "MAPLE-2".into(),
            project: "MAPLE".into(),
            summary: "hi".into(),
            ..Default::default()
        });

        assert!(app.draft.is_none());
        assert!(!app.creating);
        assert_eq!(app.view, View::Inspector);
        assert_eq!(app.issues.state.selected(), Some(0));
        assert_eq!(
            app.current_issue().map(|i| i.key.as_str()),
            Some("MAPLE-2"),
            "the inspector moves on to the real issue, JQL match or not"
        );
        assert_eq!(app.issues.items.len(), 2);
        assert_eq!(app.mode(), Mode::Inspect);

        // and its server-side fields are editable again
        assert_eq!(app.field_kind(FieldId::Status), FieldKind::SingleSelect);
    }

    #[test]
    fn leaving_the_external_editor_saves_without_ctrl_s() {
        let mut app = app_with_issue();

        let change = app
            .finish_external_edit(edit("maple", 0), Ok(Some("maple birch".into())))
            .expect("edited text is sent to Jira");
        assert_eq!(change.new_value, FieldValue::Text("maple birch".into()));
        assert!(app.editing.is_none());

        // An editor that failed keeps the buffer so nothing is lost.
        assert!(
            app.finish_external_edit(edit("maple", 0), Ok(None))
                .is_none()
        );
        assert_eq!(
            app.editing.as_ref().map(|e| e.input.buffer.as_str()),
            Some("maple")
        );
    }

    #[test]
    fn a_stale_failure_cannot_clobber_a_newer_fetch() {
        let mut app = App::new();
        let stale = app.begin_refresh();
        let current = app.begin_refresh();

        app.fail_refresh("boom".into(), stale);
        assert!(app.loading, "the newer fetch is still in flight");
        assert_eq!(app.error, None);

        app.fail_refresh("boom".into(), current);
        assert!(!app.loading);
        assert_eq!(app.error.as_deref(), Some("boom"));

        app.loading_more = true;
        app.fail_more("boom".into(), stale);
        assert!(app.loading_more, "the same guard covers pagination");

        app.fail_more("boom".into(), current);
        assert!(!app.loading_more);
    }

    #[test]
    fn a_failed_picker_fetch_names_the_field_it_was_loading() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Assignee;
        app.open_picker().expect("assignee is single select");

        app.fail_picker_options("nope".into());

        assert!(app.picker.is_none(), "the popup closes");
        assert!(
            matches!(&app.notice, Some((Notice::Error(message), _)) if message.contains("Assignee")),
            "{:?}",
            app.notice
        );
    }

    fn app_with_issue() -> App {
        let issue = Issue {
            key: "MAPLE-1".into(),
            summary: "maple".into(),
            status: "To Do".into(),
            status_category: "new".into(),
            assignee: Some(User {
                account_id: "maple-1".into(),
                display_name: "Maple".into(),
            }),
            labels: vec!["maple2".into(), "maple1".into()],
            ..Default::default()
        };
        App {
            issues: StatefulTable::new(vec![issue]),
            ..Default::default()
        }
    }

    fn status_option(id: &str, name: &str) -> PickerOption {
        PickerOption {
            id: id.into(),
            label: name.into(),
            target: Some(FieldValue::Status {
                name: name.into(),
                category: "new".into(),
            }),
        }
    }

    fn label_option(name: &str) -> PickerOption {
        PickerOption {
            id: name.into(),
            label: name.into(),
            target: None,
        }
    }

    fn user_option(id: &str, name: &str) -> PickerOption {
        PickerOption {
            id: id.into(),
            label: name.into(),
            target: Some(FieldValue::User(Some(User {
                account_id: id.into(),
                display_name: name.into(),
            }))),
        }
    }

    fn assignee_options() -> Vec<PickerOption> {
        vec![
            user_option("maple-2", "Mapleberry"),
            user_option("maple-1", "Maple"),
            PickerOption {
                id: String::new(),
                label: "Unassigned".into(),
                target: Some(FieldValue::User(None)),
            },
        ]
    }

    fn option_labels(picker: &Picker) -> Vec<&str> {
        picker.options.iter().map(|o| o.label.as_str()).collect()
    }

    #[test]
    fn confirming_an_unedited_summary_emits_no_change() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Summary;

        app.start_edit();
        assert!(app.confirm_edit().is_none());
        assert!(app.editing.is_none());

        app.start_edit();
        app.edit_insert('!');
        let change = app.confirm_edit().expect("an edited buffer is a change");
        assert_eq!(change.new_value, FieldValue::Text("maple!".into()));
        assert_eq!(app.issues.items[0].summary, "maple!");
    }

    #[test]
    fn re_picking_the_current_status_emits_no_change() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Status;
        app.open_picker().expect("status is single select");
        app.set_picker_options(
            FieldId::Status,
            vec![status_option("1", "To Do"), status_option("2", "Done")],
        );
        assert!(
            app.confirm_picker().is_none(),
            "the cursor opens on \"To Do\", and re-picking it is a no-op"
        );
        assert_eq!(app.issues.items[0].status, "To Do");

        app.selected_field = FieldId::Status;
        app.open_picker().expect("status is single select");
        app.set_picker_options(
            FieldId::Status,
            vec![status_option("1", "To Do"), status_option("2", "Done")],
        );
        app.picker_mut().move_down();
        let change = app.confirm_picker().expect("picking \"Done\" is a change");
        assert_eq!(change.transition_id.as_deref(), Some("2"));
        assert_eq!(app.issues.items[0].status, "Done");
    }

    #[test]
    fn confirming_untouched_labels_emits_no_change() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Labels;
        app.open_picker().expect("labels is multi select");
        app.set_picker_options(FieldId::Labels, vec![label_option("maple0")]);
        assert!(
            app.confirm_picker().is_none(),
            "touching nothing is a no-op"
        );
        assert_eq!(app.issues.items[0].labels, ["maple2", "maple1"]);

        app.open_picker().expect("labels is multi select");
        app.set_picker_options(FieldId::Labels, vec![label_option("maple1")]);
        app.picker_mut().toggle();
        let change = app.confirm_picker().expect("dropping a label is a change");
        assert_eq!(change.new_value, FieldValue::List(vec!["maple2".into()]));
        assert_eq!(app.issues.items[0].labels, ["maple2"]);
    }

    #[test]
    fn assignee_picks_round_trip_through_the_issue() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Assignee;

        app.open_picker().expect("assignee is single select");
        app.set_picker_options(FieldId::Assignee, assignee_options());

        // whatever order the server answered in, the "none" sentinel leads
        let picker = app.picker.as_ref().expect("picker stays open");
        assert_eq!(option_labels(picker), ["Unassigned", "Maple", "Mapleberry"]);

        // the cursor opens on the current assignee, not on the Unassigned sentinel
        assert!(
            app.confirm_picker().is_none(),
            "re-picking \"Maple\" is a no-op"
        );
        assert_eq!(app.issues.items[0].assignee_name(), "Maple");

        app.open_picker().expect("assignee is single select");
        app.set_picker_options(FieldId::Assignee, assignee_options());
        app.picker_mut().move_down();
        let change = app
            .confirm_picker()
            .expect("picking \"Mapleberry\" is a change");
        assert_eq!(
            change.new_value,
            FieldValue::User(Some(User {
                account_id: "maple-2".into(),
                display_name: "Mapleberry".into()
            }))
        );
        assert_eq!(
            change.transition_id, None,
            "only Status rides a transition id"
        );
        assert_eq!(app.issues.items[0].assignee_name(), "Mapleberry");

        app.open_picker().expect("assignee is single select");
        app.set_picker_options(FieldId::Assignee, assignee_options());
        let picker = app.picker.as_ref().expect("picker stays open");
        assert_eq!(option_labels(picker), ["Unassigned", "Mapleberry", "Maple"]);

        app.picker_mut().move_up();
        let change = app
            .confirm_picker()
            .expect("clearing the assignee is a change");
        assert_eq!(change.new_value, FieldValue::User(None));
        assert_eq!(app.issues.items[0].assignee_name(), "Unassigned");
        assert_eq!(
            app.recent_assignees,
            ["maple-2", "maple-1"],
            "a no-op re-pick still counts as a use, and the sentinel never does"
        );
    }

    #[test]
    fn assignee_and_reporter_keep_separate_recents() {
        let mut app = app_with_issue();

        app.selected_field = FieldId::Assignee;
        app.open_picker().expect("assignee is single select");
        app.set_picker_options(FieldId::Assignee, assignee_options());
        app.picker_mut().move_down();
        app.confirm_picker()
            .expect("picking \"Mapleberry\" is a change");

        app.selected_field = FieldId::Reporter;
        app.open_picker().expect("reporter is single select");
        app.set_picker_options(
            FieldId::Reporter,
            vec![
                user_option("maple-2", "Mapleberry"),
                user_option("maple-1", "Maple"),
            ],
        );
        assert_eq!(
            option_labels(app.picker.as_ref().unwrap()),
            ["Maple", "Mapleberry"],
            "the assignee's recents do not reorder the reporter list"
        );
        app.confirm_picker().expect("picking \"Maple\" is a change");

        app.selected_field = FieldId::Reporter;
        app.open_picker().expect("reporter is single select");
        app.set_picker_options(
            FieldId::Reporter,
            vec![
                user_option("maple-2", "Mapleberry"),
                user_option("maple-1", "Maple"),
            ],
        );
        assert_eq!(
            option_labels(app.picker.as_ref().unwrap()),
            ["Maple", "Mapleberry"]
        );

        assert_eq!(app.recent_assignees, ["maple-2"]);
        assert_eq!(app.recent_reporters, ["maple-1"]);
    }

    #[test]
    fn picker_options_keep_selections_the_server_did_not_return() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Labels;
        app.open_picker().expect("labels is multi select");

        app.set_picker_options(
            FieldId::Labels,
            vec![label_option("maple3"), label_option("maple1")],
        );
        let picker = app.picker.as_ref().expect("picker stays open");
        assert_eq!(option_labels(picker), ["maple1", "maple2", "maple3"]);
        assert!(!picker.loading);

        app.set_picker_options(FieldId::Status, vec![status_option("1", "Done")]);
        assert_eq!(
            option_labels(app.picker.as_ref().unwrap()),
            ["maple1", "maple2", "maple3"]
        );
    }

    #[test]
    fn revert_leaves_a_value_that_changed_again_alone() {
        let mut app = app_with_issue();
        let change = FieldChange {
            issue_key: "MAPLE-1".into(),
            field: FieldId::Summary,
            old_value: FieldValue::Text("maple".into()),
            new_value: FieldValue::Text("maple!".into()),
            transition_id: None,
        };

        app.issues.items[0].summary = "maple!".into();
        app.revert_change(&change);
        assert_eq!(app.issues.items[0].summary, "maple");

        // a newer edit must survive a failed write's revert
        app.issues.items[0].summary = "maple syrup".into();
        app.revert_change(&change);
        assert_eq!(app.issues.items[0].summary, "maple syrup");
    }

    #[test]
    fn toggle_adds_and_only_applies_to_multi_select() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Labels;
        app.open_picker().expect("labels is multi select");
        app.set_picker_options(FieldId::Labels, vec![label_option("maple0")]);

        // maple0 sorts ahead of the issue's own maple1/maple2, so the cursor starts on it
        assert_eq!(
            option_labels(app.picker.as_ref().unwrap()),
            ["maple0", "maple1", "maple2"]
        );
        app.picker_mut().toggle();
        let change = app.confirm_picker().expect("adding a label is a change");
        assert_eq!(
            change.new_value,
            FieldValue::List(vec!["maple0".into(), "maple1".into(), "maple2".into()])
        );
        assert_eq!(app.issues.items[0].labels, ["maple0", "maple1", "maple2"]);

        // Space is bound unconditionally, so it has to be inert on a single select
        let mut app = app_with_issue();
        app.selected_field = FieldId::Status;
        app.open_picker().expect("status is single select");
        app.set_picker_options(FieldId::Status, vec![status_option("2", "Done")]);
        app.picker_mut().toggle();
        assert!(
            app.picker.as_ref().unwrap().selected.is_empty(),
            "toggling a single select selects nothing"
        );
    }

    #[test]
    fn table_navigation_wraps_around() {
        let mut table = StatefulTable::new(vec!["a", "b", "c"]);
        assert_eq!(table.state.selected(), Some(0));

        table.prev();
        assert_eq!(
            table.state.selected(),
            Some(2),
            "the top wraps to the bottom"
        );

        table.next();
        assert_eq!(
            table.state.selected(),
            Some(0),
            "the bottom wraps to the top"
        );

        table.next();
        table.next();
        assert_eq!(table.state.selected(), Some(2));

        table.first();
        assert_eq!(table.state.selected(), Some(0));
        table.last();
        assert_eq!(table.state.selected(), Some(2));

        // an empty table selects nothing and no move may panic
        let mut empty: StatefulTable<&str> = StatefulTable::new(vec![]);
        for _ in 0..2 {
            empty.next();
            empty.prev();
            empty.first();
            empty.last();
            assert_eq!(empty.state.selected(), None);
        }
    }

    #[test]
    fn picker_confirms_the_option_the_filter_shows() {
        let mut app = app_with_issue();
        app.selected_field = FieldId::Status;
        app.open_picker().expect("status is single select");
        app.set_picker_options(
            FieldId::Status,
            vec![
                status_option("1", "To Do"),
                status_option("2", "Done"),
                status_option("3", "In Review"),
                status_option("4", "In Progress"),
            ],
        );

        app.picker_mut().push_query('i');
        app.picker_mut().push_query('n');
        app.picker_mut().move_down();

        let picker = app.picker.as_ref().expect("picker stays open");
        assert_eq!(
            picker
                .filtered()
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            ["In Progress", "In Review"]
        );

        // the cursor wraps inside the filtered list, not the full option list
        app.picker_mut().move_down();
        assert_eq!(app.picker.as_ref().unwrap().cursor, 0);
        app.picker_mut().move_up();
        assert_eq!(app.picker.as_ref().unwrap().cursor, 1);

        let change = app.confirm_picker().expect("picking a filtered option");
        assert_eq!(change.transition_id.as_deref(), Some("3"));
        assert_eq!(app.issues.items[0].status, "In Review");
    }

    fn date_picker(y: i32, m: u32, d: u32) -> DatePicker {
        DatePicker {
            issue_key: "MAPLE-1".into(),
            field: FieldId::Due,
            date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
        }
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn date_picker_steps_across_month_and_year_edges() {
        let mut p = date_picker(2026, 11, 30);
        p.move_days(1);
        assert_eq!(p.date, ymd(2026, 12, 1));

        let mut p = date_picker(2026, 1, 1);
        p.move_days(-1);
        assert_eq!(p.date, ymd(2025, 12, 31));

        let mut p = date_picker(2026, 1, 31);
        p.move_months(1);
        assert_eq!(
            p.date,
            ymd(2026, 2, 28),
            "a month step clamps to the shorter month"
        );

        let mut p = date_picker(2026, 1, 15);
        p.move_months(-1);
        assert_eq!(p.date, ymd(2025, 12, 15));
    }

    #[test]
    fn date_picker_opens_on_the_current_value_and_clears_it() {
        let today = ymd(2026, 11, 15);

        let mut app = app_with_issue();
        app.selected_field = FieldId::Due;

        app.open_date_picker(today);
        let picker = app.date_picker.as_ref().expect("due is a date field");
        assert_eq!(picker.date, today, "an empty due date opens on today");
        assert_eq!(picker.field, FieldId::Due);
        assert_eq!(picker.issue_key, "MAPLE-1");

        app.close_date_picker();
        app.issues.items[0].due = Some(ymd(2026, 11, 10));
        app.open_date_picker(today);
        assert_eq!(
            app.date_picker.as_ref().unwrap().date,
            ymd(2026, 11, 10),
            "an existing due date opens on itself, not on today"
        );

        let change = app
            .confirm_date_picker(true)
            .expect("clearing a due date is a change");
        assert_eq!(change.old_value, FieldValue::Date(Some(ymd(2026, 11, 10))));
        assert_eq!(change.new_value, FieldValue::Date(None));
        assert_eq!(change.transition_id, None);
        assert_eq!(app.issues.items[0].due, None);
        assert!(app.date_picker.is_none());
    }

    #[test]
    fn a_date_that_does_not_parse_keeps_the_buffer_open() {
        let mut app = app_with_issue();
        app.issues.items[0].due = Some(ymd(2026, 11, 10));
        app.selected_field = FieldId::Due;

        // Due is an editable Date field, so the user can type into it directly
        // instead of going through the date picker
        app.start_edit();
        for _ in 0.."2026-11-10".len() {
            app.edit_backspace();
        }
        for c in "tomorrow".chars() {
            app.edit_insert(c);
        }

        assert!(app.confirm_edit().is_none(), "a bad date is not a change");
        assert_eq!(
            app.issues.items[0].due,
            Some(ymd(2026, 11, 10)),
            "the issue keeps the date it had"
        );
        assert!(
            matches!(&app.notice, Some((Notice::Error(m), _)) if m.contains("YYYY-MM-DD")),
            "{:?}",
            app.notice
        );
        // the half-typed text survives, so a typo does not cost the whole edit
        assert_eq!(
            app.editing.as_ref().map(|e| e.input.buffer.as_str()),
            Some("tomorrow")
        );

        // retyping it properly goes through
        for _ in 0.."tomorrow".len() {
            app.edit_backspace();
        }
        for c in "2026-12-01".chars() {
            app.edit_insert(c);
        }
        let change = app.confirm_edit().expect("a parseable date is a change");
        assert_eq!(change.new_value, FieldValue::Date(Some(ymd(2026, 12, 1))));
        assert_eq!(app.issues.items[0].due, Some(ymd(2026, 12, 1)));

        // and an empty buffer clears the field rather than erroring
        app.start_edit();
        for _ in 0.."2026-12-01".len() {
            app.edit_backspace();
        }
        let change = app.confirm_edit().expect("an empty date clears the field");
        assert_eq!(change.new_value, FieldValue::Date(None));
        assert_eq!(app.issues.items[0].due, None);
    }

    #[test]
    fn deleting_a_row_holds_the_cursor_in_place() {
        let issue = |key: &str| Issue {
            key: key.into(),
            ..Default::default()
        };
        let mut app = App {
            issues: StatefulTable::new(vec![issue("MAPLE-1"), issue("MAPLE-2"), issue("MAPLE-3")]),
            ..Default::default()
        };

        // the row that shifts up into the gap takes the cursor, like vim's dd
        app.issues.state.select(Some(1));
        app.finish_delete("MAPLE-2");
        assert_eq!(app.issues.state.selected(), Some(1));
        assert_eq!(
            app.issues.selected().map(|i| i.key.as_str()),
            Some("MAPLE-3")
        );

        // deleting the last row has nothing below it, so the cursor steps back
        app.finish_delete("MAPLE-3");
        assert_eq!(app.issues.state.selected(), Some(0));
        assert_eq!(
            app.issues.selected().map(|i| i.key.as_str()),
            Some("MAPLE-1")
        );

        // an empty table must hold no index at all
        app.finish_delete("MAPLE-1");
        assert!(app.issues.items.is_empty());
        assert_eq!(app.issues.state.selected(), None);

        // a key that is not in the list leaves everything alone
        app.issues = StatefulTable::new(vec![issue("MAPLE-9")]);
        app.issues.state.select(Some(0));
        app.finish_delete("MAPLE-404");
        assert_eq!(app.issues.items.len(), 1);
        assert_eq!(app.issues.state.selected(), Some(0));
    }

    #[test]
    fn a_delete_confirmation_names_the_issue_and_closes_the_inspector() {
        let mut app = app_with_issue();
        app.open_inspector();

        app.ask_delete();
        let dialog = app.confirm_delete.as_ref().expect("the dialog is open");
        assert_eq!(dialog.key, "MAPLE-1");
        assert!(!dialog.confirm_focused, "Cancel holds focus on open");
        assert_eq!(app.mode(), Mode::Confirm, "the dialog owns the keyboard");

        // pressing Cancel closes the dialog without yielding a key to delete
        assert_eq!(app.press_delete_button(), None);
        assert!(app.confirm_delete.is_none());

        // backing out changes nothing
        app.cancel_delete();
        assert_eq!(app.mode(), Mode::Inspect);
        assert_eq!(app.issues.items.len(), 1);

        // confirming hands the key over exactly once
        app.ask_delete();
        app.focus_delete_button(true);
        assert_eq!(app.press_delete_button().as_deref(), Some("MAPLE-1"));
        assert_eq!(
            app.press_delete_button(),
            None,
            "a second Enter has nothing to send"
        );

        app.finish_delete("MAPLE-1");
        assert!(app.issues.items.is_empty());
        assert_eq!(
            app.view,
            View::List,
            "the inspector cannot show a dead issue"
        );
    }

    #[test]
    fn the_header_falls_back_to_the_email_until_myself_lands() {
        let mut app = app_with_issue();
        app.account = "nobody@example.com".into();

        // /myself is still in flight, so the configured email stands in
        assert_eq!(app.display_name(), "nobody@example.com");

        app.me = Some(User {
            account_id: "maple-1".into(),
            display_name: "Maple".into(),
        });
        assert_eq!(app.display_name(), "Maple");
    }

    #[test]
    fn mode_reports_the_innermost_layer() {
        let mut app = app_with_issue();
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.mode().to_string(), "Normal");

        app.open_inspector();
        assert_eq!(app.mode(), Mode::Inspect);

        app.selected_field = FieldId::Summary;
        app.start_edit();
        assert_eq!(app.mode(), Mode::Edit, "an edit shows over the inspector");

        app.selected_field = FieldId::Status;
        app.open_picker().expect("status is single select");
        assert_eq!(app.mode(), Mode::Pick, "a picker shows over an edit");

        app.close_picker();
        app.close_inspector();
        assert_eq!(app.mode(), Mode::Normal);
    }

    #[test]
    fn a_search_runs_once_typing_pauses() {
        const BASE: &str = "project = MAPLE ORDER BY updated DESC";
        let mut app = app_with_issue();
        app.list_jql = BASE.into();

        app.open_search();
        assert_eq!(app.mode(), Mode::Search);
        for c in "login".chars() {
            app.edit_search(c);
        }
        assert!(
            app.due_search(BASE).is_none(),
            "mid-typing the delay has not run out"
        );

        // the delay backdated rather than slept through
        app.search.as_mut().unwrap().changed_at = Some(Instant::now() - Duration::from_millis(400));
        let jql = app.due_search(BASE).expect("the pause fires the search");
        assert!(jql.contains("text ~ \"login*\""), "{jql}");
        assert!(jql.ends_with("ORDER BY updated DESC"), "{jql}");
        assert_eq!(app.list_jql, jql, "further pages follow the search");
        assert_eq!(app.search_query(), Some("login"));

        // the same text does not fire a second request
        app.search.as_mut().unwrap().changed_at = Some(Instant::now() - Duration::from_millis(400));
        assert!(app.due_search(BASE).is_none());

        // Enter keeps the results and leaves the bar
        app.close_search_input();
        assert!(!app.search_editing());
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.search_query(), Some("login"), "the bar keeps its text");

        // text with nothing searchable in it falls back to the plain list
        app.open_search();
        app.search.as_mut().unwrap().input.clear();
        app.edit_search('a');
        app.search.as_mut().unwrap().changed_at = Some(Instant::now() - Duration::from_millis(400));
        assert_eq!(app.due_search(BASE).as_deref(), Some(BASE));
        assert_eq!(app.search_query(), Some("a"));
    }

    #[test]
    fn enter_sends_a_pending_search_without_waiting_out_the_delay() {
        const BASE: &str = "project = MAPLE";
        let mut app = app_with_issue();

        app.open_search();
        for c in "login".chars() {
            app.edit_search(c);
        }
        assert!(app.due_search(BASE).is_none(), "still typing");

        // Enter means "I am done", so the queued search goes on the next tick
        app.close_search_input();
        let jql = app.due_search(BASE).expect("Enter does not wait");
        assert!(jql.contains("text ~ \"login*\""), "{jql}");
    }

    #[test]
    fn esc_puts_the_old_list_back_without_a_round_trip() {
        const BASE: &str = "project = MAPLE";
        let mut app = app_with_issue();
        app.issues.items.push(Issue {
            key: "MAPLE-2".into(),
            summary: "birch".into(),
            ..Default::default()
        });
        app.issues.state.select(Some(1));
        app.next_page_token = Some("page-2".into());
        let before = app.fetch_generation;

        // opening the bar and pressing Esc right away costs no request
        app.open_search();
        assert!(!app.clear_search(BASE));
        assert!(app.search.is_none());
        assert_eq!(app.list_jql, BASE);

        app.open_search();
        app.edit_search('x');
        app.edit_search('y');
        app.search.as_mut().unwrap().changed_at = Some(Instant::now() - Duration::from_millis(400));
        app.due_search(BASE).expect("a search ran");
        assert_ne!(app.list_jql, BASE);

        // the search lands, replacing the list it was launched from
        let generation = app.begin_refresh();
        app.finish_refresh(
            vec![Issue {
                key: "MAPLE-9".into(),
                ..Default::default()
            }],
            Some("search-page-2".into()),
            generation,
        );
        assert_eq!(app.issues.items.len(), 1);

        assert!(
            !app.clear_search(BASE),
            "the old list is already in hand, so no fetch"
        );
        assert_eq!(
            app.issues
                .items
                .iter()
                .map(|i| i.key.as_str())
                .collect::<Vec<_>>(),
            ["MAPLE-1", "MAPLE-2"]
        );
        assert_eq!(
            app.issues.state.selected(),
            Some(1),
            "and the cursor comes back to the issue it left"
        );
        assert_eq!(app.next_page_token.as_deref(), Some("page-2"));
        assert!(!app.loading);
        assert!(app.search.is_none());
        assert_eq!(app.list_jql, BASE);
        assert!(
            app.fetch_generation > generation && generation > before,
            "a search result still in flight must not land on the restored list"
        );

        // proof of that: the in-flight result is dropped
        app.finish_refresh(vec![], None, generation);
        assert_eq!(app.issues.items.len(), 2);
    }

    #[test]
    fn tabs_wrap_in_both_directions() {
        let all = Tab::ALL;
        for (i, &tab) in all.iter().enumerate() {
            assert_eq!(tab.next(), all[(i + 1) % all.len()]);
            assert_eq!(tab.prev(), all[(i + all.len() - 1) % all.len()]);
        }
    }
}
