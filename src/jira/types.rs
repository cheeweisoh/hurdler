use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, de::IgnoredAny};
use serde_json::{Value, json};

use super::adf::{self, AdfNode};
use crate::domain::issue::{Choice, FieldId, FieldValue, Issue, User};

pub struct SearchPage {
    pub issues: Vec<Issue>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub issues: Vec<RawIssue>,
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub is_last: bool,
}

#[derive(Debug, Deserialize)]
pub struct RawIssue {
    pub key: String,
    pub fields: RawFields,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFields {
    pub project: RawProject,
    pub issuetype: RawIssueType,
    pub summary: String,
    pub status: RawStatus,
    pub priority: Option<RawPriority>,
    #[serde(default)]
    pub assignee: Option<RawUser>,
    pub parent: Option<RawParent>,
    pub duedate: Option<NaiveDate>,
    #[serde(default)]
    pub fix_versions: Vec<RawVersion>,
    pub labels: Vec<String>,
    pub reporter: Option<RawUser>,
    pub description: Option<AdfNode>,
    #[serde(deserialize_with = "jira_datetime")]
    pub created: DateTime<Utc>,
    #[serde(deserialize_with = "jira_datetime")]
    pub updated: DateTime<Utc>,
}

impl From<RawIssue> for Issue {
    fn from(raw: RawIssue) -> Self {
        let fields = raw.fields;
        Issue {
            key: raw.key,
            project: fields.project.key,
            issue_type: fields.issuetype.name,
            issue_type_id: fields.issuetype.id,
            is_subtask: fields.issuetype.subtask,
            summary: fields.summary,
            status: fields.status.name,
            status_category: fields.status.status_category.key,
            priority: fields.priority.map(Choice::from),
            assignee: fields.assignee.map(User::from),
            parent: fields.parent.map(Choice::from),
            due: fields.duedate,
            fix_versions: fields.fix_versions.into_iter().map(|v| v.name).collect(),
            labels: fields.labels,
            reporter: fields.reporter.map(User::from),
            description: fields
                .description
                .map(|doc| adf::to_markdown(&doc))
                .unwrap_or_default(),
            created: fields.created,
            updated: fields.updated,
        }
    }
}

fn jira_datetime<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    DateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S%.f%z")
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(serde::de::Error::custom)
}

#[derive(Debug, Deserialize)]
pub struct RawProject {
    pub key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIssueType {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub subtask: bool,
}

impl From<RawIssueType> for Choice {
    fn from(t: RawIssueType) -> Self {
        Choice {
            id: t.id,
            name: t.name,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawStatus {
    pub name: String,
    pub status_category: RawStatusCategory,
}

#[derive(Debug, Deserialize)]
pub struct RawStatusCategory {
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct TransitionsResponse {
    pub transitions: Vec<RawTransition>,
}

#[derive(Debug, Deserialize)]
pub struct RawTransition {
    pub id: String,
    pub name: String,
    pub to: RawStatus,
}

#[derive(Debug, Deserialize)]
pub struct RawPriority {
    pub id: String,
    pub name: String,
}

impl From<RawPriority> for Choice {
    fn from(p: RawPriority) -> Self {
        Choice {
            id: p.id,
            name: p.name,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTypesResponse {
    pub issue_types: Vec<RawIssueType>,
}

#[derive(Debug, Deserialize)]
pub struct RawCreated {
    pub key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawUser {
    pub account_id: String,
    pub display_name: String,
    #[serde(default)]
    pub account_type: String,
}

impl From<RawUser> for User {
    fn from(u: RawUser) -> Self {
        User {
            account_id: u.account_id,
            display_name: u.display_name,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RawParent {
    pub key: String,
    pub fields: Option<RawParentFields>,
}

#[derive(Debug, Deserialize)]
pub struct RawParentFields {
    pub summary: String,
}

impl From<RawParent> for Choice {
    fn from(p: RawParent) -> Self {
        Choice {
            id: p.key,
            name: p.fields.map(|f| f.summary).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EditMeta {
    pub fields: EditMetaFields,
}

#[derive(Debug, Deserialize)]
pub struct EditMetaFields {
    pub reporter: Option<EditMetaField<IgnoredAny>>,
    pub priority: Option<EditMetaField<RawPriority>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditMetaField<T> {
    #[serde(default)]
    pub required: bool,
    #[serde(default = "Vec::new")]
    pub allowed_values: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct RawVersion {
    pub name: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub released: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelPage {
    pub values: Vec<String>,
}

pub fn update_payload(field: FieldId, value: &FieldValue) -> Option<Value> {
    match (field, value) {
        (FieldId::Summary, FieldValue::Text(text)) => Some(json!({"fields": {"summary": text}})),
        (FieldId::Priority, FieldValue::Choice(Some(choice))) => {
            Some(json!({"fields": {"priority": {"id": choice.id}}}))
        }
        (FieldId::Assignee, FieldValue::User(user)) => {
            let assignee = user.as_ref().map(|u| json!({"accountId": u.account_id}));
            Some(json!({"fields": {"assignee": assignee}}))
        }
        (FieldId::Parent, FieldValue::Choice(parent)) => match parent {
            Some(choice) => Some(json!({"fields": {"parent": {"key": choice.id}}})),
            None => Some(json!({"update": {"parent": [{"set": {"none": true}}]}})),
        },
        (FieldId::Due, FieldValue::Date(date)) => {
            let due = date.map(|d| d.format("%Y-%m-%d").to_string());
            Some(json!({"fields": {"duedate": due}}))
        }
        (FieldId::FixVersions, FieldValue::List(names)) => {
            let versions: Vec<Value> = names.iter().map(|name| json!({"name": name})).collect();
            Some(json!({"fields": {"fixVersions": versions}}))
        }
        (FieldId::Labels, FieldValue::List(labels)) => Some(json!({"fields": {"labels": labels}})),
        (FieldId::Reporter, FieldValue::User(Some(user))) => {
            Some(json!({"fields": {"reporter": {"accountId": user.account_id}}}))
        }
        (FieldId::Description, FieldValue::Text(markdown)) => {
            let description = if markdown.trim().is_empty() {
                Value::Null
            } else {
                adf::from_markdown(markdown)
            };
            Some(json!({"fields": {"description": description}}))
        }
        _ => None,
    }
}

pub fn create_payload(issue: &Issue) -> Value {
    let mut fields = serde_json::Map::new();
    fields.insert("project".into(), json!({"key": issue.project}));
    fields.insert("issuetype".into(), json!({"id": issue.issue_type_id}));

    for field in [
        FieldId::Summary,
        FieldId::Description,
        FieldId::Assignee,
        FieldId::Labels,
        FieldId::Parent,
        FieldId::Due,
        FieldId::FixVersions,
    ] {
        let value = issue.raw_value(field);
        if is_empty_value(&value) {
            continue;
        }
        if let Some(Value::Object(part)) = update_payload(field, &value)
            .as_ref()
            .and_then(|p| p.get("fields"))
        {
            fields.extend(part.clone());
        }
    }

    json!({"fields": fields})
}

fn is_empty_value(value: &FieldValue) -> bool {
    match value {
        FieldValue::Text(text) => text.trim().is_empty(),
        FieldValue::List(items) => items.is_empty(),
        FieldValue::User(user) => user.is_none(),
        FieldValue::Choice(choice) => choice.is_none(),
        FieldValue::Date(date) => date.is_none(),
        FieldValue::Status { name, .. } => name.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jira_payloads_map_onto_the_domain() {
        let payload = r#"{"issues": [
            {"key": "MAPLE-1", "fields": {
                "project": {"key": "MAPLE"},
                "summary": "maple full",
                "status": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
                "issuetype": {"id": "10001", "name": "Task"},
                "assignee": {"accountId": "maple-1", "displayName": "Maple"},
                "labels": ["maple"],
                "fixVersions": [{"id": "10100", "name": "1.9", "released": true}],
                "duedate": "2026-11-30",
                "priority": {"id": "3", "name": "Medium"},
                "reporter": {"accountId": "maple-9", "displayName": "Mapleboss"},
                "description": {"type": "doc", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "maple body"}]}
                ]},
                "created": "2024-01-01T09:00:00.000+0000",
                "updated": "2024-01-02T09:00:00.000+0000"
            }},
            {"key": "MAPLE-2", "fields": {
                "project": {"key": "MAPLE"},
                "summary": "maple bare",
                "status": {"name": "To Do", "statusCategory": {"key": "new"}},
                "issuetype": {"name": "Bug"},
                "labels": [],
                "created": "2024-01-01T09:00:00.000+0000",
                "updated": "2024-01-02T09:00:00.000+0000"
            }},
            {"key": "MAPLE-3", "fields": {
                "project": {"key": "MAPLE"},
                "summary": "maple child",
                "status": {"name": "To Do", "statusCategory": {"key": "new"}},
                "issuetype": {"name": "Subtask", "subtask": true},
                "parent": {"key": "MAPLE-1", "fields": {"summary": "maple full"}},
                "labels": [],
                "created": "2024-01-01T09:00:00.000+0000",
                "updated": "2024-01-02T09:00:00.000+0000"
            }}
        ]}"#;

        let page: SearchResponse = serde_json::from_str(payload).expect("search payload parses");
        let issues: Vec<Issue> = page.issues.into_iter().map(Issue::from).collect();

        let full = &issues[0];
        assert_eq!(full.key, "MAPLE-1");
        assert_eq!(full.summary, "maple full");
        assert_eq!(
            (full.status.as_str(), full.status_category.as_str()),
            ("In Progress", "indeterminate")
        );
        assert_eq!(full.issue_type, "Task");
        assert_eq!(full.issue_type_id, "10001");
        assert_eq!(full.project_key(), "MAPLE");
        assert_eq!(full.assignee_name(), "Maple");
        assert_eq!(
            full.assignee.as_ref().map(|u| u.account_id.as_str()),
            Some("maple-1")
        );
        assert_eq!(full.labels, ["maple"]);
        assert_eq!(full.fix_versions, ["1.9"], "only the name is kept");
        assert_eq!(full.description, "maple body");
        // the camelCase rename must not disturb the single-word Jira field names
        assert_eq!(full.due, NaiveDate::from_ymd_opt(2026, 11, 30));
        assert_eq!(
            full.priority.as_ref().map(|p| p.name.as_str()),
            Some("Medium")
        );
        assert_eq!(full.reporter_name(), "Mapleboss");

        // an unassigned issue omits assignee and description entirely
        let bare = &issues[1];
        assert_eq!(bare.assignee_name(), "Unassigned");
        assert!(bare.description.is_empty());
        assert!(bare.labels.is_empty());
        assert!(
            bare.fix_versions.is_empty(),
            "a project without releases omits fixVersions entirely"
        );
        assert_eq!(bare.parent, None);
        assert!(!bare.is_subtask, "a missing subtask flag defaults to false");

        // a subtask carries its parent and the flag that picks its parent candidates
        let child = &issues[2];
        assert!(child.is_subtask);
        assert_eq!(
            child.parent,
            Some(Choice {
                id: "MAPLE-1".into(),
                name: "maple full".into()
            })
        );
    }

    #[test]
    fn edit_meta_reads_the_reporter_field() {
        let payload = r#"{"fields": {
            "reporter": {"required": true, "allowedValues": []},
            "priority": {"required": false, "allowedValues": [{"id": "3", "name": "Medium"}]}
        }}"#;

        let meta: EditMeta = serde_json::from_str(payload).expect("editmeta payload parses");
        let reporter = meta.fields.reporter.expect("reporter is editable");
        assert!(reporter.required);
        assert_eq!(
            meta.fields
                .priority
                .expect("priority is editable")
                .allowed_values
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            ["Medium"]
        );

        let payload = r#"{"fields": {"summary": {"required": true}}}"#;
        let meta: EditMeta = serde_json::from_str(payload).expect("editmeta payload parses");
        assert!(meta.fields.reporter.is_none());
        assert!(meta.fields.priority.is_none());
    }

    #[test]
    fn update_payload_shapes_each_field() {
        let maple = User {
            account_id: "maple-1".into(),
            display_name: "Maple".into(),
        };
        assert_eq!(
            update_payload(FieldId::Assignee, &FieldValue::User(Some(maple))),
            Some(json!({"fields": {"assignee": {"accountId": "maple-1"}}}))
        );

        // an explicit null is what clears the field; an omitted key would no-op
        assert_eq!(
            update_payload(FieldId::Assignee, &FieldValue::User(None)),
            Some(json!({"fields": {"assignee": null}}))
        );

        assert_eq!(
            update_payload(FieldId::Summary, &FieldValue::Text("maple".into())),
            Some(json!({"fields": {"summary": "maple"}}))
        );

        assert_eq!(
            update_payload(FieldId::Labels, &FieldValue::List(vec!["maple".into()])),
            Some(json!({"fields": {"labels": ["maple"]}}))
        );

        // versions go back as objects keyed by name, and an empty list clears them
        assert_eq!(
            update_payload(
                FieldId::FixVersions,
                &FieldValue::List(vec!["1.9".into(), "1.10".into()])
            ),
            Some(json!({"fields": {"fixVersions": [{"name": "1.9"}, {"name": "1.10"}]}}))
        );
        assert_eq!(
            update_payload(FieldId::FixVersions, &FieldValue::List(vec![])),
            Some(json!({"fields": {"fixVersions": []}}))
        );

        // only the key reaches Jira, never the summary shown beside it
        assert_eq!(
            update_payload(
                FieldId::Parent,
                &FieldValue::Choice(Some(Choice {
                    id: "MAPLE-9".into(),
                    name: "a maple epic".into()
                }))
            ),
            Some(json!({"fields": {"parent": {"key": "MAPLE-9"}}}))
        );

        // clearing a parent takes the update verb, not a fields write
        assert_eq!(
            update_payload(FieldId::Parent, &FieldValue::Choice(None)),
            Some(json!({"update": {"parent": [{"set": {"none": true}}]}}))
        );

        // status moves through the transitions endpoint, never a field PUT
        assert_eq!(
            update_payload(
                FieldId::Status,
                &FieldValue::Status {
                    name: "Done".into(),
                    category: "done".into()
                }
            ),
            None
        );

        // the type is only ever set at creation, never updated
        assert_eq!(
            update_payload(
                FieldId::Type,
                &FieldValue::Choice(Some(Choice {
                    id: "10001".into(),
                    name: "Task".into()
                }))
            ),
            None
        );
    }

    #[test]
    fn create_payload_carries_the_project_the_type_and_what_was_filled_in() {
        let mut draft = Issue {
            project: "MAPLE".into(),
            issue_type_id: "10001".into(),
            issue_type: "Task".into(),
            summary: "plant a maple".into(),
            ..Default::default()
        };

        let bare = create_payload(&draft);
        assert_eq!(
            bare,
            json!({"fields": {
                "project": {"key": "MAPLE"},
                "issuetype": {"id": "10001"},
                "summary": "plant a maple"
            }})
        );

        draft.labels = vec!["maple".into()];
        draft.fix_versions = vec!["1.9".into()];
        draft.due = NaiveDate::from_ymd_opt(2026, 11, 30);
        draft.assignee = Some(User {
            account_id: "maple-1".into(),
            display_name: "Maple".into(),
        });
        draft.parent = Some(Choice {
            id: "MAPLE-9".into(),
            name: "a maple epic".into(),
        });
        draft.description = "maple **notes**".into();

        let full = create_payload(&draft);
        let fields = full.get("fields").expect("a fields object");
        assert_eq!(fields["labels"], json!(["maple"]));
        assert_eq!(fields["duedate"], json!("2026-11-30"));
        assert_eq!(fields["fixVersions"], json!([{"name": "1.9"}]));
        assert_eq!(fields["assignee"], json!({"accountId": "maple-1"}));
        assert_eq!(fields["parent"], json!({"key": "MAPLE-9"}));
        assert_eq!(fields["description"]["type"], json!("doc"));

        // an empty parent would otherwise arrive as an "update" verb, which
        // POST /issue rejects alongside "fields"
        assert!(full.get("update").is_none());
        draft.parent = None;
        assert!(create_payload(&draft).get("update").is_none());
        assert!(create_payload(&draft)["fields"].get("parent").is_none());
    }

    #[test]
    fn issue_types_keep_jiras_order_and_their_ids() {
        let payload = r#"{"issueTypes": [
            {"id": "10001", "name": "Task", "subtask": false},
            {"id": "10003", "name": "Subtask", "subtask": true},
            {"id": "10002", "name": "Bug"}
        ]}"#;

        let page: IssueTypesResponse =
            serde_json::from_str(payload).expect("issue types payload parses");
        let kept: Vec<Choice> = page
            .issue_types
            .into_iter()
            .filter(|t| !t.subtask)
            .map(Choice::from)
            .collect();

        assert_eq!(
            kept.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["Task", "Bug"],
            "sub-tasks need a parent at creation, so they are dropped"
        );
        assert_eq!(kept[0].id, "10001");
    }
}
