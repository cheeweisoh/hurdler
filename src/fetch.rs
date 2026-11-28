use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{OptionsRequest, PickerOption},
    config::Config,
    domain::issue::{self, Choice, FieldChange, FieldId, FieldValue, Issue, User},
    event::Event,
    jira::{
        client::JiraClient,
        types::{RawTransition, RawVersion},
    },
};

pub fn spawn_test_connection(config: Config, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        let result = JiraClient::new(config)
            .myself()
            .await
            .map(|user| user.display_name)
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(Event::SetupTested(result));
    });
}

pub fn spawn_fetch_me(client: JiraClient, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        if let Ok(user) = client.myself().await {
            let _ = tx.send(Event::MeFetched(user));
        }
    });
}

pub fn spawn_fetch_issues(
    client: JiraClient,
    tx: UnboundedSender<Event>,
    jql: String,
    generation: u64,
) {
    tokio::spawn(async move {
        let event = match client.search_page(&jql, None).await {
            Ok(page) => Event::IssuesFetched {
                issues: page.issues,
                next_page_token: page.next_page_token,
                generation,
            },
            Err(e) => Event::IssuesFetchFailed {
                message: format!("{e:#}"),
                generation,
            },
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_check_jql(client: JiraClient, tx: UnboundedSender<Event>, name: String, jql: String) {
    tokio::spawn(async move {
        if let Err(e) = client.check_jql(&jql).await {
            let _ = tx.send(Event::QueryCheckFailed {
                name,
                message: format!("{e:#}"),
            });
        }
    });
}

pub fn spawn_fetch_more(
    client: JiraClient,
    tx: UnboundedSender<Event>,
    jql: String,
    token: String,
    generation: u64,
) {
    tokio::spawn(async move {
        let event = match client.search_page(&jql, Some(&token)).await {
            Ok(page) => Event::MoreIssuesFetched {
                issues: page.issues,
                next_page_token: page.next_page_token,
                generation,
            },
            Err(e) => Event::MoreIssuesFailed {
                message: format!("{e:#}"),
                generation,
            },
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_fetch_options(
    client: JiraClient,
    tx: UnboundedSender<Event>,
    request: OptionsRequest,
) {
    match request.field {
        FieldId::Type => spawn_fetch_issue_types(client, tx, request.project_key),
        FieldId::Status => spawn_fetch_transitions(client, tx, request.issue_key),
        FieldId::Priority => spawn_fetch_priorities(client, tx, request.issue_key),
        FieldId::Assignee => spawn_fetch_assignees(client, tx, request.project_key),
        FieldId::Parent => {
            if let Some(jql) = request.jql {
                spawn_fetch_parents(client, tx, jql);
            }
        }
        FieldId::Labels => spawn_fetch_labels(client, tx),
        FieldId::FixVersions => spawn_fetch_versions(client, tx, request.project_key),
        FieldId::Reporter => {
            spawn_fetch_reporters(client, tx, request.project_key, request.issue_key)
        }
        _ => {}
    }
}

fn choice_options(choices: Vec<Choice>) -> Vec<PickerOption> {
    choices
        .into_iter()
        .map(|choice| PickerOption {
            id: choice.id.clone(),
            label: choice.name.clone(),
            target: Some(FieldValue::Choice(Some(choice))),
        })
        .collect()
}

pub fn spawn_fetch_issue_types(client: JiraClient, tx: UnboundedSender<Event>, project: String) {
    tokio::spawn(async move {
        let event = match client.list_issue_types(&project).await {
            Ok(types) => Event::PickerOptionsFetched {
                field: FieldId::Type,
                options: choice_options(types),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn transition_options(transitions: Vec<RawTransition>) -> Vec<PickerOption> {
    transitions
        .into_iter()
        .map(|t| {
            let label = if t.name == t.to.name {
                t.name
            } else {
                format!("{} -> {}", t.name, t.to.name)
            };
            PickerOption {
                id: t.id,
                label,
                target: Some(FieldValue::Status {
                    name: t.to.name,
                    category: t.to.status_category.key,
                }),
            }
        })
        .collect()
}

pub fn spawn_fetch_transitions(client: JiraClient, tx: UnboundedSender<Event>, issue_key: String) {
    tokio::spawn(async move {
        let event = match client.list_transitions(&issue_key).await {
            Ok(transitions) => Event::PickerOptionsFetched {
                field: FieldId::Status,
                options: transition_options(transitions),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_fetch_priorities(client: JiraClient, tx: UnboundedSender<Event>, issue_key: String) {
    tokio::spawn(async move {
        let event = match client.list_priorities(&issue_key).await {
            Ok(choices) => Event::PickerOptionsFetched {
                field: FieldId::Priority,
                options: choice_options(choices),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn user_options(users: Vec<User>) -> Vec<PickerOption> {
    let mut options: Vec<PickerOption> = users
        .into_iter()
        .map(|user| PickerOption {
            id: user.account_id.clone(),
            label: user.display_name.clone(),
            target: Some(FieldValue::User(Some(user))),
        })
        .collect();
    options.sort_by(|a, b| a.label.cmp(&b.label));

    options
}

fn assignee_options(users: Vec<User>) -> Vec<PickerOption> {
    let mut options = user_options(users);
    options.insert(
        0,
        PickerOption {
            id: String::new(),
            label: "Unassigned".into(),
            target: Some(FieldValue::User(None)),
        },
    );
    options
}

pub fn spawn_fetch_assignees(client: JiraClient, tx: UnboundedSender<Event>, project_key: String) {
    tokio::spawn(async move {
        let event = match client.list_assignable(&project_key).await {
            Ok(users) => Event::PickerOptionsFetched {
                field: FieldId::Assignee,
                options: assignee_options(users),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn parent_options(issues: Vec<Issue>) -> Vec<PickerOption> {
    let mut options = vec![PickerOption {
        id: String::new(),
        label: "No parent".into(),
        target: Some(FieldValue::Choice(None)),
    }];

    options.extend(issues.into_iter().map(|issue| {
        let parent = Choice {
            id: issue.key,
            name: issue.summary,
        };
        PickerOption {
            id: parent.id.clone(),
            label: issue::parent_label(&parent),
            target: Some(FieldValue::Choice(Some(parent))),
        }
    }));

    options
}

pub fn spawn_fetch_parents(client: JiraClient, tx: UnboundedSender<Event>, jql: String) {
    tokio::spawn(async move {
        let event = match client.search(&jql).await {
            Ok(issues) => Event::PickerOptionsFetched {
                field: FieldId::Parent,
                options: parent_options(issues),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn label_options(labels: Vec<String>) -> Vec<PickerOption> {
    labels
        .into_iter()
        .map(|label| PickerOption {
            id: label.clone(),
            label,
            target: None,
        })
        .collect()
}

pub fn spawn_fetch_labels(client: JiraClient, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        let event = match client.list_labels().await {
            Ok(labels) => Event::PickerOptionsFetched {
                field: FieldId::Labels,
                options: label_options(labels),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn version_options(versions: Vec<RawVersion>) -> Vec<PickerOption> {
    let mut versions: Vec<RawVersion> = versions.into_iter().filter(|v| !v.archived).collect();
    versions.sort_by_key(|v| v.released);

    versions
        .into_iter()
        .map(|version| PickerOption {
            id: version.name.clone(),
            label: version.name,
            target: None,
        })
        .collect()
}

pub fn spawn_fetch_versions(client: JiraClient, tx: UnboundedSender<Event>, project_key: String) {
    tokio::spawn(async move {
        let event = match client.list_versions(&project_key).await {
            Ok(versions) => Event::PickerOptionsFetched {
                field: FieldId::FixVersions,
                options: version_options(versions),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

fn reporter_options(users: Vec<User>, allow_empty: bool) -> Vec<PickerOption> {
    let mut options = user_options(users);
    if allow_empty {
        options.insert(
            0,
            PickerOption {
                id: String::new(),
                label: "No Reporter".into(),
                target: Some(FieldValue::User(None)),
            },
        );
    }
    options
}

pub fn spawn_fetch_reporters(
    client: JiraClient,
    tx: UnboundedSender<Event>,
    project_key: String,
    issue_key: String,
) {
    tokio::spawn(async move {
        let result = tokio::try_join!(
            client.list_reporters(&project_key),
            client.reporter_required(&issue_key),
        );

        let event = match result {
            Ok((users, required)) => Event::PickerOptionsFetched {
                field: FieldId::Reporter,
                options: reporter_options(users, !required),
            },
            Err(e) => Event::PickerOptionsFailed(format!("{e:#}")),
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_create_issue(client: JiraClient, tx: UnboundedSender<Event>, draft: Issue) {
    tokio::spawn(async move {
        let event = match client.create_issue(&draft).await {
            Err(e) => Event::IssueCreateFailed {
                key: None,
                message: format!("{e:#}"),
            },
            Ok(key) => match client.issue(&key).await {
                Ok(issue) => Event::IssueCreated(Box::new(issue)),
                Err(e) => Event::IssueCreateFailed {
                    message: format!("Created {key}, but couldn't load it: {e:#} - press r"),
                    key: Some(key),
                },
            },
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_delete_issue(client: JiraClient, tx: UnboundedSender<Event>, key: String) {
    tokio::spawn(async move {
        let event = match client.delete_issue(&key).await {
            Ok(()) => Event::IssueDeleted(key),
            Err(e) => Event::IssueDeleteFailed {
                key,
                message: format!("{e:#}"),
            },
        };
        let _ = tx.send(event);
    });
}

pub fn spawn_update_field(client: JiraClient, tx: UnboundedSender<Event>, change: FieldChange) {
    tokio::spawn(async move {
        let event = match client.update_field(&change).await {
            Ok(()) => Event::FieldUpdated(change),
            Err(e) => Event::FieldUpdateFailed {
                change,
                message: format!("{e:#}"),
            },
        };
        let _ = tx.send(event);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::jira::types::TransitionsResponse;

    #[test]
    fn a_transition_is_labelled_by_where_it_lands() {
        let payload = r#"{"transitions": [
            {"id": "11", "name": "Done", "to": {"name": "Done", "statusCategory": {"key": "done"}}},
            {"id": "21", "name": "Start", "to": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}}}
        ]}"#;

        let response: TransitionsResponse =
            serde_json::from_str(payload).expect("transitions payload parses");
        let options = transition_options(response.transitions);

        assert_eq!(options[0].label, "Done", "no arrow when the name matches");
        assert_eq!(options[1].label, "Start -> In Progress");
        assert_eq!(
            options[1].target,
            Some(FieldValue::Status {
                name: "In Progress".into(),
                category: "indeterminate".into()
            })
        );
    }

    fn user(id: &str, name: &str) -> User {
        User {
            account_id: id.into(),
            display_name: name.into(),
        }
    }

    #[test]
    fn assignee_options_lead_with_unassigned() {
        let options = assignee_options(vec![
            user("maple-2", "Mapleberry"),
            user("maple-1", "Maple"),
        ]);

        assert_eq!(
            options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
            ["Unassigned", "Maple", "Mapleberry"]
        );
        assert_eq!(options[0].id, "");
        assert_eq!(options[0].target, Some(FieldValue::User(None)));

        // the id carries the account id, the label the display name
        assert_eq!(
            (options[1].id.as_str(), options[1].label.as_str()),
            ("maple-1", "Maple")
        );
        assert_eq!(
            options[1].target,
            Some(FieldValue::User(Some(user("maple-1", "Maple"))))
        );
    }

    #[test]
    fn parent_options_lead_with_no_parent() {
        let issue = |key: &str, summary: &str| Issue {
            key: key.into(),
            summary: summary.into(),
            ..Default::default()
        };

        let options = parent_options(vec![issue("MAPLE-9", "a maple epic")]);

        // the blank id is what keeps the sentinel on top of App::set_picker_options' sort
        assert_eq!(options[0].id, "");
        assert_eq!(options[0].label, "No parent");
        assert_eq!(options[0].target, Some(FieldValue::Choice(None)));

        assert_eq!(options[1].id, "MAPLE-9");
        assert_eq!(options[1].label, "MAPLE-9: a maple epic");
        assert_eq!(
            options[1].target,
            Some(FieldValue::Choice(Some(Choice {
                id: "MAPLE-9".into(),
                name: "a maple epic".into()
            })))
        );
        assert_eq!(options.len(), 2);
    }

    fn labels(options: &[PickerOption]) -> Vec<&str> {
        options.iter().map(|o| o.label.as_str()).collect()
    }

    #[test]
    fn reporter_options_offers_no_reporter_only_when_optional() {
        let users = vec![user("maple-2", "Mapleberry"), user("maple-1", "Maple")];

        let optional = reporter_options(users.clone(), true);
        assert_eq!(labels(&optional), ["No Reporter", "Maple", "Mapleberry"]);
        assert_eq!(optional[0].id, "");
        assert_eq!(optional[0].target, Some(FieldValue::User(None)));

        // Jira rejects an empty reporter where the field is mandatory, so the
        // sentinel must not be offered at all rather than fail on confirm
        let required = reporter_options(users, false);
        assert_eq!(labels(&required), ["Maple", "Mapleberry"]);
        assert!(required.iter().all(|o| !o.id.is_empty()));
    }

    #[test]
    fn version_options_drop_the_archived_and_lead_with_the_unreleased() {
        let version = |name: &str, released: bool, archived: bool| RawVersion {
            name: name.into(),
            released,
            archived,
        };

        // Jira's own order: oldest release first, archived mixed in
        let options = version_options(vec![
            version("1.9", true, false),
            version("0.1", false, true),
            version("1.10", true, false),
            version("2.0", false, false),
            version("2.1", false, false),
        ]);

        assert_eq!(labels(&options), ["2.0", "2.1", "1.9", "1.10"]);
        assert!(
            !labels(&options).contains(&"0.1"),
            "archived versions are hidden in Jira's own UI too"
        );

        // the name is both the id and the label, because fix versions
        // round-trip by name through FieldValue::List
        assert_eq!(options[0].id, "2.0");
        assert_eq!(options[0].target, None);
    }

    #[test]
    fn choice_options_carry_the_choice_as_their_target() {
        let choice = |id: &str, name: &str| Choice {
            id: id.into(),
            name: name.into(),
        };
        let options = choice_options(vec![choice("3", "Medium"), choice("1", "Highest")]);

        // editmeta already returns priorities in Jira's own order, so it is kept
        assert_eq!(labels(&options), ["Medium", "Highest"]);
        assert_eq!(options[0].id, "3");
        assert_eq!(
            options[0].target,
            Some(FieldValue::Choice(Some(choice("3", "Medium"))))
        );
    }
}
