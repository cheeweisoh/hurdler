mod app;
mod browser;
mod cli;
mod config;
mod cycle;
mod domain;
mod editor;
mod event;
mod fetch;
mod handlers;
mod jira;
mod setup;
mod text_input;
mod theme;
mod ui;

use std::{path::PathBuf, time::Duration};

use clap::Parser;
use ratatui::DefaultTerminal;

use crate::app::{App, Notice};
use crate::cli::{Cli, Command};
use crate::config::{Config, ConfigError};
use crate::event::{Event, EventHandler};
use crate::jira::client::JiraClient;
use crate::setup::{Setup, SetupField, SetupOutcome};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut terminal = ratatui::init();
    let result = start(&mut terminal, cli).await;
    ratatui::restore();

    if let Some(message) = result? {
        println!("{message}")
    }
    Ok(())
}

async fn start(terminal: &mut DefaultTerminal, cli: Cli) -> anyhow::Result<Option<String>> {
    let override_path = cli.config.as_deref();
    let path = config::config_path(override_path);

    if let Some(Command::Init) = cli.command {
        let message = match run_setup(terminal, path.clone()).await? {
            SetupOutcome::Saved => format!("Config saved to {}", path.display()),
            SetupOutcome::Cancelled => "Config cancelled".to_string(),
        };
        return Ok(Some(message));
    }

    let config = match Config::load(override_path) {
        Err(ConfigError::Missing(_)) => match run_setup(terminal, path).await? {
            SetupOutcome::Saved => Config::load(override_path)?,
            SetupOutcome::Cancelled => return Ok(None),
        },
        other => other?,
    };

    run(terminal, config).await?;
    Ok(None)
}

async fn run_setup(terminal: &mut DefaultTerminal, path: PathBuf) -> anyhow::Result<SetupOutcome> {
    let (draft, notice) = match Config::read(&path) {
        Ok(config) => (config, None),
        Err(ConfigError::Missing(_)) => (Config::default(), None),
        Err(e) => (
            Config::default(),
            Some(Notice::Error(format!("{e} — saving replaces it"))),
        ),
    };
    let mut state = Setup {
        path,
        draft,
        selected: SetupField::BaseUrl,
        editing: None,
        testing: false,
        notice,
        outcome: None,
    };
    let mut events = EventHandler::new(Duration::from_millis(250));
    let tx = events.sender();

    while state.outcome.is_none() {
        terminal.draw(|frame| ui::setup::render(frame, &state))?;
        let Some(event) = events.next().await else {
            break;
        };

        match event {
            Event::Key(key) => setup::handle_key(&mut state, key, &tx),
            Event::SetupTested(result) => {
                state.testing = false;
                state.notice = Some(match result {
                    Ok(name) => Notice::Info(format!("Connection OK: signed in as {name}")),
                    Err(e) => Notice::Error(format!("Connection failed: {e}")),
                });
            }
            _ => {}
        }
    }

    Ok(state.outcome.unwrap_or(SetupOutcome::Cancelled))
}

async fn run(terminal: &mut DefaultTerminal, config: Config) -> anyhow::Result<()> {
    let mut app = App::new();
    app.account = config.email.clone();
    app.queries = config.queries.clone();

    let editor = config.editor.clone();
    let client = JiraClient::new(config);
    let mut events = EventHandler::new(Duration::from_millis(100));
    let tx = events.sender();

    app.list_jql = app.base_jql().to_string();
    let generation = app.begin_refresh();
    fetch::spawn_fetch_issues(client.clone(), tx.clone(), app.list_jql.clone(), generation);
    fetch::spawn_fetch_me(client.clone(), tx.clone());

    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        let Some(event) = events.next().await else {
            break;
        };

        match event {
            Event::Tick => {
                app.expire_notice();
                if let Some(jql) = app.due_search() {
                    let generation = app.begin_refresh();
                    fetch::spawn_fetch_issues(client.clone(), tx.clone(), jql, generation);
                }
            }
            Event::Key(key) => {
                handlers::handle_key(&mut app, key, &tx, &client);

                if let Some(edit) = app.external_edit.take() {
                    events.pause().await;
                    let result = tokio::task::block_in_place(|| {
                        editor::edit_text(terminal, &edit.input.buffer, editor.as_deref())
                    });
                    events.resume();
                    if let Some(change) = app.finish_external_edit(edit, result) {
                        fetch::spawn_update_field(client.clone(), tx.clone(), change);
                    }
                }
            }
            Event::SetupTested(_) => {}
            Event::MeFetched(user) => app.me = Some(user),
            Event::IssuesFetched {
                issues,
                next_page_token,
                generation,
            } => app.finish_refresh(issues, next_page_token, generation),
            Event::IssuesFetchFailed {
                message,
                generation,
            } => app.fail_refresh(message, generation),
            Event::MoreIssuesFetched {
                issues,
                next_page_token,
                generation,
            } => app.append_issues(issues, next_page_token, generation),
            Event::MoreIssuesFailed {
                message,
                generation,
            } => app.fail_more(message, generation),
            Event::PickerOptionsFetched { field, options } => {
                app.set_picker_options(field, options)
            }
            Event::PickerOptionsFailed(message) => app.fail_picker_options(message),
            Event::FieldUpdated(change) => {
                app.set_notice(Notice::Info(format!(
                    "Saved {} on {}",
                    change.field.label(),
                    change.issue_key
                )));
            }
            Event::IssueDeleted(key) => app.finish_delete(&key),
            Event::IssueDeleteFailed { key, message } => {
                app.set_notice(Notice::Error(format!("Couldn't delete {key}: {message}")))
            }
            Event::IssueCreated(issue) => app.finish_create(*issue),
            Event::IssueCreateFailed { key, message } => {
                app.creating = false;
                if key.is_some() {
                    app.draft = None;
                }
                app.set_notice(Notice::Error(message));
            }
            Event::FieldUpdateFailed { change, message } => {
                app.revert_change(&change);
                app.set_notice(Notice::Error(format!(
                    "Couldn't save {} on {}: {message}",
                    change.field.label(),
                    change.issue_key
                )))
            }
        }
    }

    Ok(())
}
