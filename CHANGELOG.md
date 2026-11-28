# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Saved JQL queries: `f` opens a picker to switch between the named queries in `config.toml`, with `All Issues` as the built-in default
- Managing those queries from the picker: `a` adds, `e` edits and `d` deletes, saving back to `config.toml`

### Changed
- Jira errors report the message Jira returned instead of the raw response body
- A query Jira rejects is named in the status bar and the list that was on screen stays put
- Failures to load the list appear in the status bar instead of replacing the issue pane
- `?` opens help over the picker and dialogs rather than being swallowed by them

### Removed
- The `jql` config key and its `JIRA_JQL` override, replaced by a `queries` list; an existing `jql` key is ignored and dropped the next time the file is written

  ```toml
  [[queries]]
  name = "My open issues"
  jql = "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
  ```

## [0.1.0] - 2026-11-23

### Added
- Issue list with pagination and full-text search
- Inspector with inline editing of summary, status, assignee, priority, labels, parent, due date, and Markdown descriptions (inline or in `$EDITOR`)
- Creating issues from a draft in the inspector
- Quick actions: assign to me, change status
- Setup screen (`hurdler init`) with connection test

[Unreleased]: https://github.com/cheeweisoh/hurdler/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/cheeweisoh/hurdler/releases/tag/v0.1.0
