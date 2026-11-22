use chrono::{DateTime, Local, NaiveDate, Utc};

const DUE_SOON_DAYS: i64 = 3;

pub const DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    ReadOnly,
    Text,
    LongText,
    Date,
    SingleSelect,
    MultiSelect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(String),
    List(Vec<String>),
    Status { name: String, category: String },
    User(Option<User>),
    Choice(Option<Choice>),
    Date(Option<NaiveDate>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldId {
    Type,
    #[default]
    Summary,
    Status,
    Priority,
    Assignee,
    Parent,
    Due,
    FixVersions,
    Labels,
    Reporter,
    Created,
    Updated,
    Description,
}

impl FieldId {
    pub const ALL: [FieldId; 13] = [
        FieldId::Summary,
        FieldId::Type,
        FieldId::Status,
        FieldId::Priority,
        FieldId::Assignee,
        FieldId::Reporter,
        FieldId::Due,
        FieldId::Parent,
        FieldId::Labels,
        FieldId::FixVersions,
        FieldId::Created,
        FieldId::Updated,
        FieldId::Description,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FieldId::Type => "Type",
            FieldId::Summary => "Summary",
            FieldId::Status => "Status",
            FieldId::Priority => "Priority",
            FieldId::Assignee => "Assignee",
            FieldId::Parent => "Parent",
            FieldId::Due => "Due Date",
            FieldId::FixVersions => "Fix Versions",
            FieldId::Labels => "Labels",
            FieldId::Reporter => "Reporter",
            FieldId::Created => "Created",
            FieldId::Updated => "Updated",
            FieldId::Description => "Description",
        }
    }

    pub fn kind(self) -> FieldKind {
        match self {
            FieldId::Summary => FieldKind::Text,
            FieldId::Status => FieldKind::SingleSelect,
            FieldId::Priority => FieldKind::SingleSelect,
            FieldId::Assignee => FieldKind::SingleSelect,
            FieldId::Parent => FieldKind::SingleSelect,
            FieldId::Due => FieldKind::Date,
            FieldId::FixVersions | FieldId::Labels => FieldKind::MultiSelect,
            FieldId::Reporter => FieldKind::SingleSelect,
            FieldId::Description => FieldKind::LongText,
            _ => FieldKind::ReadOnly,
        }
    }

    pub fn kind_when(self, creating: bool) -> FieldKind {
        if !creating {
            return self.kind();
        }
        match self {
            FieldId::Type => FieldKind::SingleSelect,
            FieldId::Status | FieldId::Priority | FieldId::Reporter => FieldKind::ReadOnly,
            other => other.kind(),
        }
    }

    pub fn is_editable(self, creating: bool) -> bool {
        self.kind_when(creating) != FieldKind::ReadOnly
    }

    fn index(self) -> usize {
        FieldId::ALL.iter().position(|&f| f == self).unwrap_or(0)
    }

    pub fn next_editable(self, creating: bool) -> FieldId {
        let i = self.index();
        FieldId::ALL
            .iter()
            .cycle()
            .skip(i + 1)
            .take(FieldId::ALL.len())
            .copied()
            .find(|f| f.is_editable(creating))
            .unwrap_or(self)
    }

    pub fn prev_editable(self, creating: bool) -> FieldId {
        let i = self.index();
        FieldId::ALL
            .iter()
            .rev()
            .cycle()
            .skip(FieldId::ALL.len() - i)
            .take(FieldId::ALL.len())
            .copied()
            .find(|f| f.is_editable(creating))
            .unwrap_or(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FieldChange {
    pub issue_key: String,
    pub field: FieldId,
    pub old_value: FieldValue,
    pub new_value: FieldValue,
    pub transition_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueState {
    Overdue,
    DueSoon,
    OnTrack,
}

#[derive(Debug, Clone, Default)]
pub struct Issue {
    pub key: String,
    pub project: String,
    pub issue_type: String,
    pub issue_type_id: String,
    pub is_subtask: bool,
    pub summary: String,
    pub status: String,
    pub status_category: String,
    pub priority: Option<Choice>,
    pub assignee: Option<User>,
    pub parent: Option<Choice>,
    pub due: Option<NaiveDate>,
    pub fix_versions: Vec<String>,
    pub labels: Vec<String>,
    pub reporter: Option<User>,
    pub description: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl Issue {
    pub fn project_key(&self) -> &str {
        &self.project
    }

    pub fn assignee_name(&self) -> &str {
        self.assignee
            .as_ref()
            .map(|u| u.display_name.as_str())
            .unwrap_or("Unassigned")
    }

    pub fn reporter_name(&self) -> &str {
        self.reporter
            .as_ref()
            .map(|u| u.display_name.as_str())
            .unwrap_or("No Reporter")
    }

    pub fn parent_candidates_jql(&self) -> String {
        let types = if self.is_subtask {
            "issuetype in standardIssueTypes()"
        } else {
            "issuetype = Epic"
        };
        let except = if self.key.contains('-') {
            format!(" AND key != {}", self.key)
        } else {
            String::new()
        };
        format!(
            "project = {} AND {types}{except} ORDER BY updated DESC",
            self.project_key()
        )
    }

    pub fn due_state(&self, today: NaiveDate) -> Option<DueState> {
        let due = self.due?;
        if self.status_category == "done" {
            return Some(DueState::OnTrack);
        }

        let days_left = due.signed_duration_since(today).num_days();
        Some(if days_left < 0 {
            DueState::Overdue
        } else if days_left <= DUE_SOON_DAYS {
            DueState::DueSoon
        } else {
            DueState::OnTrack
        })
    }

    pub fn field_value(&self, field: FieldId, now: DateTime<Utc>) -> String {
        match field {
            FieldId::Type => self.issue_type.clone(),
            FieldId::Summary => self.summary.clone(),
            FieldId::Status => self.status.clone(),
            FieldId::Priority => self
                .priority
                .as_ref()
                .map(|choice| choice.name.clone())
                .unwrap_or_default(),
            FieldId::Assignee => self.assignee_name().to_string(),
            FieldId::Parent => self.parent.as_ref().map(parent_label).unwrap_or_default(),
            FieldId::Due => self.due.map(format_date).unwrap_or_default(),
            FieldId::FixVersions => self.fix_versions.join(", "),
            FieldId::Labels => self.labels.join(", "),
            FieldId::Reporter => self.reporter_name().to_string(),
            FieldId::Created => format_timestamp(self.created, now),
            FieldId::Updated => format_timestamp(self.updated, now),
            FieldId::Description => self.description.clone(),
        }
    }

    pub fn raw_value(&self, field: FieldId) -> FieldValue {
        match field {
            FieldId::Type => FieldValue::Choice((!self.issue_type_id.is_empty()).then(|| Choice {
                id: self.issue_type_id.clone(),
                name: self.issue_type.clone(),
            })),
            FieldId::Summary => FieldValue::Text(self.summary.clone()),
            FieldId::Status => FieldValue::Status {
                name: self.status.clone(),
                category: self.status_category.clone(),
            },
            FieldId::Priority => FieldValue::Choice(self.priority.clone()),
            FieldId::Assignee => FieldValue::User(self.assignee.clone()),
            FieldId::Parent => FieldValue::Choice(self.parent.clone()),
            FieldId::Due => FieldValue::Date(self.due),
            FieldId::FixVersions => FieldValue::List(self.fix_versions.clone()),
            FieldId::Labels => FieldValue::List(self.labels.clone()),
            FieldId::Reporter => FieldValue::User(self.reporter.clone()),
            FieldId::Created => FieldValue::Text(self.created.to_rfc3339()),
            FieldId::Updated => FieldValue::Text(self.updated.to_rfc3339()),
            FieldId::Description => FieldValue::Text(self.description.clone()),
        }
    }

    pub fn edit_text(&self, field: FieldId) -> String {
        match field {
            FieldId::Summary => self.summary.clone(),
            FieldId::Description => self.description.clone(),
            FieldId::Due => self.due.map(format_date).unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub fn set_field_value(&mut self, field: FieldId, value: FieldValue) {
        match (field, value) {
            (FieldId::Type, FieldValue::Choice(choice)) => {
                let (id, name) = choice.map(|c| (c.id, c.name)).unwrap_or_default();
                self.issue_type_id = id;
                self.issue_type = name;
            }
            (FieldId::Summary, FieldValue::Text(text)) => self.summary = text,
            (FieldId::Status, FieldValue::Status { name, category }) => {
                self.status = name;
                self.status_category = category;
            }
            (FieldId::Priority, FieldValue::Choice(choice)) => self.priority = choice,
            (FieldId::Assignee, FieldValue::User(user)) => self.assignee = user,
            (FieldId::Parent, FieldValue::Choice(choice)) => self.parent = choice,
            (FieldId::Due, FieldValue::Date(date)) => self.due = date,
            (FieldId::FixVersions, FieldValue::List(names)) => self.fix_versions = names,
            (FieldId::Labels, FieldValue::List(labels)) => self.labels = labels,
            (FieldId::Reporter, FieldValue::User(user)) => self.reporter = user,
            (FieldId::Description, FieldValue::Text(text)) => self.description = text,
            _ => {}
        }
    }
}

pub fn parent_label(parent: &Choice) -> String {
    if parent.name.is_empty() {
        parent.id.clone()
    } else {
        format!("{}: {}", parent.id, parent.name)
    }
}

fn format_date(date: NaiveDate) -> String {
    date.format(DATE_FORMAT).to_string()
}

fn format_timestamp(dt: DateTime<Utc>, now: DateTime<Utc>) -> String {
    if dt == DateTime::<Utc>::default() {
        return String::new();
    }
    let local = dt.with_timezone(&Local).format("%Y-%m-%d %H:%M");
    format!("{local}  ({})", relative_time(dt, now))
}

fn relative_time(dt: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(dt);
    let minutes = delta.num_minutes();
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if delta.num_hours() < 24 {
        format!("{}h ago", delta.num_hours())
    } else if delta.num_days() < 30 {
        format!("{}d ago", delta.num_days())
    } else {
        format!("{}mo ago", delta.num_days() / 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_fields_cycle_skipping_read_only() {
        let cycle = [
            FieldId::Summary,
            FieldId::Status,
            FieldId::Priority,
            FieldId::Assignee,
            FieldId::Reporter,
            FieldId::Due,
            FieldId::Parent,
            FieldId::Labels,
            FieldId::FixVersions,
            FieldId::Description,
        ];

        for (i, &field) in cycle.iter().enumerate() {
            assert_eq!(field.next_editable(false), cycle[(i + 1) % cycle.len()]);
            assert_eq!(
                field.prev_editable(false),
                cycle[(i + cycle.len() - 1) % cycle.len()]
            );
        }

        // Type is read only and never lands under the cursor
        assert!(!FieldId::Type.is_editable(false));
    }

    #[test]
    fn creating_moves_the_cursor_onto_type_and_off_the_server_side_fields() {
        // the row order of FieldId::ALL, minus the rows a draft can't fill
        let cycle = [
            FieldId::Summary,
            FieldId::Type,
            FieldId::Assignee,
            FieldId::Due,
            FieldId::Parent,
            FieldId::Labels,
            FieldId::FixVersions,
            FieldId::Description,
        ];

        for (i, &field) in cycle.iter().enumerate() {
            assert_eq!(field.next_editable(true), cycle[(i + 1) % cycle.len()]);
            assert_eq!(
                field.prev_editable(true),
                cycle[(i + cycle.len() - 1) % cycle.len()]
            );
        }

        // a draft picks its type, but has no transitions or editmeta to ask
        assert_eq!(FieldId::Type.kind_when(true), FieldKind::SingleSelect);
        for field in [FieldId::Status, FieldId::Priority, FieldId::Reporter] {
            assert_eq!(field.kind_when(true), FieldKind::ReadOnly);
            assert_eq!(field.kind_when(false), FieldKind::SingleSelect);
        }

        // everything else keeps the kind it always had
        for field in FieldId::ALL {
            if !matches!(
                field,
                FieldId::Type | FieldId::Status | FieldId::Priority | FieldId::Reporter
            ) {
                assert_eq!(field.kind_when(true), field.kind(), "{field:?}");
            }
        }
    }

    #[test]
    fn a_type_round_trips_as_a_choice() {
        let mut issue = Issue::default();
        assert_eq!(issue.raw_value(FieldId::Type), FieldValue::Choice(None));

        issue.set_field_value(
            FieldId::Type,
            FieldValue::Choice(Some(Choice {
                id: "10001".into(),
                name: "Task".into(),
            })),
        );
        assert_eq!(issue.issue_type_id, "10001");
        assert_eq!(issue.field_value(FieldId::Type, Utc::now()), "Task");
        assert_eq!(
            issue.raw_value(FieldId::Type),
            FieldValue::Choice(Some(Choice {
                id: "10001".into(),
                name: "Task".into()
            }))
        );

        issue.set_field_value(FieldId::Type, FieldValue::Choice(None));
        assert_eq!(issue.issue_type_id, "");
        assert_eq!(issue.field_value(FieldId::Type, Utc::now()), "");
    }

    #[test]
    fn parent_candidates_depend_on_the_issue_being_a_subtask() {
        let issue = |is_subtask: bool| Issue {
            key: "MAPLE-1".into(),
            project: "MAPLE".into(),
            is_subtask,
            ..Default::default()
        };

        let epics = issue(false).parent_candidates_jql();
        assert!(epics.contains("issuetype = Epic"), "{epics}");

        let standard = issue(true).parent_candidates_jql();
        assert!(
            standard.contains("issuetype in standardIssueTypes()"),
            "{standard}"
        );

        // either way the issue is scoped to its project and excludes itself
        for jql in [epics, standard] {
            assert!(jql.contains("project = MAPLE"), "{jql}");
            assert!(jql.contains("key != MAPLE-1"), "{jql}");
        }
    }

    #[test]
    fn a_parent_shows_its_key_and_summary() {
        let parent = |key: &str, summary: &str| Choice {
            id: key.into(),
            name: summary.into(),
        };
        let mut issue = Issue {
            key: "MAPLE-1".into(),
            parent: Some(parent("MAPLE-9", "a maple epic")),
            ..Default::default()
        };

        assert_eq!(
            issue.field_value(FieldId::Parent, Utc::now()),
            "MAPLE-9: a maple epic"
        );
        assert_eq!(
            issue.raw_value(FieldId::Parent),
            FieldValue::Choice(Some(parent("MAPLE-9", "a maple epic")))
        );

        // no summary came back with the parent, so the key stands alone
        issue.set_field_value(
            FieldId::Parent,
            FieldValue::Choice(Some(parent("MAPLE-8", ""))),
        );
        assert_eq!(issue.field_value(FieldId::Parent, Utc::now()), "MAPLE-8");

        issue.set_field_value(FieldId::Parent, FieldValue::Choice(None));
        assert_eq!(issue.parent, None);
        assert_eq!(issue.field_value(FieldId::Parent, Utc::now()), "");

        // a value of the wrong shape leaves the field alone
        issue.set_field_value(FieldId::Parent, FieldValue::List(vec!["MAPLE-7".into()]));
        assert_eq!(issue.parent, None);
    }

    #[test]
    fn a_draft_looks_for_parents_without_excluding_a_key() {
        let jql = Issue {
            project: "MAPLE".into(),
            ..Default::default()
        }
        .parent_candidates_jql();

        assert!(jql.contains("project = MAPLE"), "{jql}");
        assert!(!jql.contains("key !="), "a draft has no key yet: {jql}");
    }

    #[test]
    fn due_state_tracks_the_deadline_and_respects_done() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        let state = |due: Option<NaiveDate>, category: &str| {
            Issue {
                due,
                status_category: category.into(),
                ..Default::default()
            }
            .due_state(today)
        };
        let nov = |d: u32| NaiveDate::from_ymd_opt(2026, 11, d);

        // an issue with no due date has nothing to report
        assert_eq!(state(None, "indeterminate"), None);

        assert_eq!(state(nov(10), "indeterminate"), Some(DueState::Overdue));
        assert_eq!(state(nov(14), "new"), Some(DueState::Overdue));

        // today and the next DUE_SOON_DAYS are the warning window, inclusive
        assert_eq!(state(nov(15), "new"), Some(DueState::DueSoon));
        assert_eq!(state(nov(18), "new"), Some(DueState::DueSoon));
        assert_eq!(
            state(nov(19), "new"),
            Some(DueState::OnTrack),
            "one day past the window is calm again"
        );

        // a closed issue never nags, however far past its date it is
        assert_eq!(state(nov(10), "done"), Some(DueState::OnTrack));
        assert_eq!(state(nov(30), "done"), Some(DueState::OnTrack));
    }

    #[test]
    fn relative_time_picks_its_unit() {
        let now = DateTime::parse_from_rfc3339("2026-11-15T12:00:00Z")
            .expect("a fixed instant")
            .with_timezone(&Utc);
        let ago = |seconds: i64| relative_time(now - chrono::Duration::seconds(seconds), now);

        assert_eq!(ago(0), "just now");
        assert_eq!(ago(59), "just now");
        assert_eq!(ago(60), "1m ago");
        assert_eq!(ago(59 * 60), "59m ago");
        assert_eq!(ago(60 * 60), "1h ago");
        assert_eq!(ago(23 * 3600), "23h ago");
        assert_eq!(ago(24 * 3600), "1d ago");
        assert_eq!(ago(29 * 86400), "29d ago");

        // ponytail: months are num_days() / 30, not calendar months, so the
        // whole back half of a month reads the same. Fine for a "last touched"
        // hint; swap in a calendar diff if it ever has to be exact.
        assert_eq!(ago(30 * 86400), "1mo ago");
        assert_eq!(ago(59 * 86400), "1mo ago");
        assert_eq!(ago(60 * 86400), "2mo ago");
    }
}
