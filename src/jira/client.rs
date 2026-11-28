use anyhow::{Context, Result, bail};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::{
    config::Config,
    domain::issue::{Choice, FieldChange, Issue, User},
    jira::types::{
        EditMeta, ErrorBody, IssueTypesResponse, LabelPage, RawCreated, RawIssue, RawTransition,
        RawUser, RawVersion, SearchPage, SearchResponse, TransitionsResponse, create_payload,
        update_payload,
    },
};

const FIELDS: [&str; 14] = [
    "project",
    "issuetype",
    "summary",
    "status",
    "priority",
    "assignee",
    "parent",
    "duedate",
    "fixVersions",
    "labels",
    "reporter",
    "description",
    "created",
    "updated",
];

#[derive(Clone)]
pub struct JiraClient {
    http: Client,
    config: Config,
}

impl JiraClient {
    pub fn new(config: Config) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub fn browse_url(&self, issue_key: &str) -> String {
        format!("{}/browse/{issue_key}", self.config.base_url)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}/{}", self.config.base_url, path.trim_start_matches('/'));
        self.http
            .request(method, url)
            .basic_auth(&self.config.email, Some(&self.config.api_token))
    }

    async fn check(response: Response) -> Result<Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        match serde_json::from_str::<ErrorBody>(&body)
            .ok()
            .and_then(|e| e.summary())
        {
            Some(message) => bail!("{message}"),
            None => bail!("Jira returned {status}: {body}"),
        }
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response> {
        let response = request.send().await.context("could not reach Jira")?;
        Self::check(response).await
    }

    async fn fetch<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        self.send(request)
            .await?
            .json()
            .await
            .context("could not parse Jira's response")
    }

    pub async fn myself(&self) -> Result<User> {
        let raw: RawUser = self
            .fetch(self.request(Method::GET, "/rest/api/3/myself"))
            .await?;
        Ok(User::from(raw))
    }

    pub async fn search_page(&self, jql: &str, page_token: Option<&str>) -> Result<SearchPage> {
        let mut body = json!({
            "jql": jql,
            "maxResults": self.config.page_size,
            "fields": FIELDS
        });
        if let Some(token) = page_token {
            body["nextPageToken"] = json!(token);
        }

        let page: SearchResponse = self
            .fetch(
                self.request(Method::POST, "/rest/api/3/search/jql")
                    .json(&body),
            )
            .await?;

        let next_page_token = if page.is_last {
            None
        } else {
            page.next_page_token
        };
        Ok(SearchPage {
            issues: page.issues.into_iter().map(Issue::from).collect(),
            next_page_token,
        })
    }

    pub async fn search(&self, jql: &str) -> Result<Vec<Issue>> {
        self.search_page(jql, None).await.map(|page| page.issues)
    }

    pub async fn check_jql(&self, jql: &str) -> Result<()> {
        let body = json!({ "jql": jql, "maxResults": 1 });
        self.send(
            self.request(Method::POST, "/rest/api/3/search/jql")
                .json(&body),
        )
        .await
        .map(|_| ())
    }

    pub async fn issue(&self, key: &str) -> Result<Issue> {
        let raw: RawIssue = self
            .fetch(
                self.request(Method::GET, &format!("/rest/api/3/issue/{key}"))
                    .query(&[("fields", FIELDS.join(","))]),
            )
            .await?;
        Ok(Issue::from(raw))
    }

    pub async fn list_transitions(&self, issue_key: &str) -> Result<Vec<RawTransition>> {
        let page: TransitionsResponse = self
            .fetch(self.request(
                Method::GET,
                &format!("/rest/api/3/issue/{issue_key}/transitions"),
            ))
            .await?;

        Ok(page.transitions)
    }

    pub async fn transition_issue(&self, issue_key: &str, transition_id: &str) -> Result<()> {
        self.send(
            self.request(
                Method::POST,
                &format!("/rest/api/3/issue/{issue_key}/transitions"),
            )
            .json(&json!({"transition": {"id": transition_id}})),
        )
        .await?;

        Ok(())
    }

    async fn list_users(&self, path: &str, params: &[(&str, &str)]) -> Result<Vec<User>> {
        const PAGE_SIZE: usize = 100;
        const MAX_USERS: usize = 1000;

        let mut users = Vec::new();
        let mut start_at = 0;

        loop {
            let start = start_at.to_string();
            let page_size = PAGE_SIZE.to_string();

            let mut query: Vec<(&str, &str)> = params.to_vec();
            query.push(("startAt", &start));
            query.push(("maxResults", &page_size));

            let page: Vec<RawUser> = self
                .fetch(self.request(Method::GET, path).query(&query))
                .await?;

            if page.is_empty() {
                break;
            }
            users.extend(
                page.into_iter()
                    .filter(|u| u.account_type == "atlassian")
                    .map(User::from),
            );
            start_at += PAGE_SIZE;
            if users.len() >= MAX_USERS {
                break;
            }
        }

        Ok(users)
    }

    pub async fn list_assignable(&self, project_key: &str) -> Result<Vec<User>> {
        self.list_users(
            "/rest/api/3/user/assignable/search",
            &[("project", project_key)],
        )
        .await
    }

    pub async fn list_reporters(&self, project_key: &str) -> Result<Vec<User>> {
        self.list_users(
            "/rest/api/3/user/permission/search",
            &[
                ("permissions", "CREATE_ISSUES"),
                ("projectKey", project_key),
            ],
        )
        .await
    }

    async fn edit_meta(&self, issue_key: &str) -> Result<EditMeta> {
        self.fetch(self.request(
            Method::GET,
            &format!("/rest/api/3/issue/{issue_key}/editmeta"),
        ))
        .await
    }

    pub async fn list_priorities(&self, issue_key: &str) -> Result<Vec<Choice>> {
        let meta = self.edit_meta(issue_key).await?;
        let Some(priority) = meta.fields.priority else {
            bail!("priority can't be edited on {issue_key}");
        };

        Ok(priority
            .allowed_values
            .into_iter()
            .map(|p| Choice {
                id: p.id,
                name: p.name,
            })
            .collect())
    }

    pub async fn reporter_required(&self, issue_key: &str) -> Result<bool> {
        let meta = self.edit_meta(issue_key).await?;
        let Some(reporter) = meta.fields.reporter else {
            bail!("you can't change the reporter on {issue_key}");
        };

        Ok(reporter.required)
    }

    pub async fn list_issue_types(&self, project_key: &str) -> Result<Vec<Choice>> {
        let page: IssueTypesResponse = self
            .fetch(
                self.request(
                    Method::GET,
                    &format!("/rest/api/3/issue/createmeta/{project_key}/issuetypes"),
                )
                .query(&[("maxResults", "100")]),
            )
            .await?;

        Ok(page
            .issue_types
            .into_iter()
            .filter(|t| !t.subtask)
            .map(Choice::from)
            .collect())
    }

    pub async fn list_labels(&self) -> Result<Vec<String>> {
        let page: LabelPage = self
            .fetch(
                self.request(Method::GET, "/rest/api/3/label")
                    .query(&[("maxResults", "1000")]),
            )
            .await?;

        Ok(page.values)
    }

    pub async fn list_versions(&self, project_key: &str) -> Result<Vec<RawVersion>> {
        self.fetch(self.request(
            Method::GET,
            &format!("/rest/api/3/project/{project_key}/versions"),
        ))
        .await
    }

    pub async fn create_issue(&self, issue: &Issue) -> Result<String> {
        let created: RawCreated = self
            .fetch(
                self.request(Method::POST, "/rest/api/3/issue")
                    .json(&create_payload(issue)),
            )
            .await?;

        Ok(created.key)
    }

    pub async fn delete_issue(&self, issue_key: &str) -> Result<()> {
        self.send(self.request(Method::DELETE, &format!("/rest/api/3/issue/{issue_key}")))
            .await?;

        Ok(())
    }

    pub async fn update_field(&self, change: &FieldChange) -> Result<()> {
        if let Some(id) = &change.transition_id {
            return self.transition_issue(&change.issue_key, id).await;
        }

        let Some(body) = update_payload(change.field, &change.new_value) else {
            bail!("{} can't be edited yet", change.field.label());
        };

        self.send(
            self.request(
                Method::PUT,
                &format!("/rest/api/3/issue/{}", change.issue_key),
            )
            .json(&body),
        )
        .await?;

        Ok(())
    }
}
