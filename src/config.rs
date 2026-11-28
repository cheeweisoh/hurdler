use std::{
    env, error, fmt, fs,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_token: String,
    pub jql: String,
    pub page_size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            email: String::new(),
            api_token: String::new(),
            jql: "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC"
                .into(),
            page_size: 50,
            editor: None,
        }
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("email", &self.email)
            .field("api_token", &"***")
            .field("jql", &self.jql)
            .field("page_size", &self.page_size)
            .field("editor", &self.editor)
            .finish()
    }
}

impl Config {
    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(ConfigError::Missing(path.to_path_buf()));
            }
            Err(e) => {
                return Err(ConfigError::Invalid {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
            }
        };

        let config: Config = toml::from_str(&text).map_err(|e| ConfigError::Invalid {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        Ok(config)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let body = toml::to_string_pretty(self).map_err(io::Error::other)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_private(path, &body)
    }

    pub fn load(override_path: Option<&Path>) -> Result<Self, ConfigError> {
        let path = config_path(override_path);
        let mut config = Self::read(&path)?;

        config.apply_env(|name| env::var(name).ok());
        config.base_url = config.base_url.trim_end_matches('/').to_string();
        config.validate(&path)?;
        Ok(config)
    }

    fn apply_env(&mut self, var: impl Fn(&str) -> Option<String>) {
        let var = |name| var(name).filter(|v: &String| !v.is_empty());
        if let Some(token) = var("JIRA_API_TOKEN") {
            self.api_token = token;
        }
        if let Some(base_url) = var("JIRA_BASE_URL") {
            self.base_url = base_url;
        }
        if let Some(email) = var("JIRA_EMAIL") {
            self.email = email;
        }
        if let Some(jql) = var("JIRA_JQL") {
            self.jql = jql;
        }
    }

    fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        let mut missing = Vec::new();
        if self.api_token.trim().is_empty() {
            missing.push("api_token");
        }
        if self.base_url.trim().is_empty() {
            missing.push("base_url");
        }
        if self.email.trim().is_empty() {
            missing.push("email");
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Incomplete {
                path: path.to_path_buf(),
                fields: missing,
            })
        }
    }
}

pub fn config_path(override_path: Option<&Path>) -> PathBuf {
    if let Some(path) = override_path {
        return path.to_path_buf();
    }
    if let Some(path) = env::var_os("HURDLER_CONFIG").filter(|v| !v.is_empty()) {
        return PathBuf::from(path);
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"));
    base.join("hurdler").join("config.toml")
}

fn write_private(path: &Path, text: &str) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(PathBuf),
    Invalid {
        path: PathBuf,
        message: String,
    },
    Incomplete {
        path: PathBuf,
        fields: Vec<&'static str>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::Missing(path) => write!(
                f,
                "no config file at {}\n create one with the settings in config.example",
                path.display()
            ),
            ConfigError::Invalid { path, message } => {
                write!(f, "invalid config in {}: {message}", path.display())
            }
            ConfigError::Incomplete { path, fields } => {
                write!(f, "{} is missing: {}", path.display(), fields.join(", "))
            }
        }
    }
}

impl error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete() -> Config {
        Config {
            base_url: "https://maple.atlassian.net".into(),
            email: "maple@maplesyrup.example".into(),
            api_token: "maple-file-token".into(),
            jql: "project = MAPLE".into(),
            ..Default::default()
        }
    }

    #[test]
    fn env_vars_override_the_config_file() {
        let mut config = complete();
        config.apply_env(|name| match name {
            "JIRA_BASE_URL" => Some("https://maplebush.atlassian.net".into()),
            "JIRA_EMAIL" => Some("maple@maplebush.example".into()),
            "JIRA_API_TOKEN" => Some("maple-env-token".into()),
            "JIRA_JQL" => Some("project = MAPLEBUSH".into()),
            _ => None,
        });

        assert_eq!(config.base_url, "https://maplebush.atlassian.net");
        assert_eq!(config.email, "maple@maplebush.example");
        assert_eq!(config.api_token, "maple-env-token");
        assert_eq!(config.jql, "project = MAPLEBUSH");

        // page_size and editor have no env override and keep the file's values
        assert_eq!(config.page_size, 50);
        assert_eq!(config.editor, None);
    }

    #[test]
    fn an_empty_env_var_leaves_the_file_value_alone() {
        // an exported-but-empty JIRA_API_TOKEN must not blank a working token
        let mut config = complete();
        config.apply_env(|_| Some(String::new()));
        let untouched = complete();
        assert_eq!(config.api_token, untouched.api_token);
        assert_eq!(config.base_url, untouched.base_url);
        assert_eq!(config.email, untouched.email);
        assert_eq!(config.jql, untouched.jql);

        // an unset variable is the same no-op
        let mut config = complete();
        config.apply_env(|_| None);
        assert_eq!(config.api_token, untouched.api_token);
    }

    #[test]
    fn a_partial_config_file_keeps_the_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("maple.toml");
        fs::write(
            &path,
            "base_url = \"https://maple.atlassian.net\"\nemail = \"maple@maple.example\"\napi_token = \"maple-token\"\n",
        )
        .expect("write config");

        let config = Config::read(&path).expect("a partial file still parses");
        assert_eq!(config.base_url, "https://maple.atlassian.net");
        assert_eq!(config.page_size, 50);
        assert_eq!(config.jql, Config::default().jql);
        assert_eq!(config.editor, None);
    }

    #[test]
    fn a_missing_file_and_a_malformed_file_report_differently() {
        let dir = tempfile::tempdir().expect("temp dir");

        let absent = dir.path().join("nomaple.toml");
        match Config::read(&absent) {
            Err(ConfigError::Missing(path)) => assert_eq!(path, absent),
            other => panic!("expected Missing, got {other:?}"),
        }

        let malformed = dir.path().join("badmaple.toml");
        fs::write(&malformed, "base_url = ").expect("write config");
        let error = match Config::read(&malformed) {
            Err(error @ ConfigError::Invalid { .. }) => error,
            other => panic!("expected Invalid, got {other:?}"),
        };

        // each error names the file the user has to go fix
        assert!(
            error.to_string().contains(&malformed.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn validate_names_every_missing_field() {
        let path = Path::new("/tmp/maple-test.toml");

        // whitespace-only is as missing as empty
        let blank = Config {
            base_url: "   ".into(),
            email: String::new(),
            api_token: "\t".into(),
            ..Default::default()
        };
        match blank.validate(path) {
            Err(ConfigError::Incomplete { fields, .. }) => {
                assert_eq!(fields, ["api_token", "base_url", "email"])
            }
            other => panic!("expected Incomplete, got {other:?}"),
        }

        // one missing field is reported alone, not alongside the ones that are set
        let partial = Config {
            api_token: String::new(),
            ..complete()
        };
        match partial.validate(path) {
            Err(ConfigError::Incomplete { fields, .. }) => assert_eq!(fields, ["api_token"]),
            other => panic!("expected Incomplete, got {other:?}"),
        }

        assert!(complete().validate(path).is_ok());
    }

    #[test]
    fn debug_never_prints_the_api_token() {
        let config = Config {
            api_token: "maple-secret-token".into(),
            ..complete()
        };
        let text = format!("{config:?}");

        assert!(!text.contains("maple-secret-token"), "{text}");
        assert!(text.contains("***"), "{text}");
        // the fields that are safe to log still show up
        assert!(text.contains("maple@maplesyrup.example"), "{text}");
    }

    #[test]
    fn an_explicit_path_wins_over_the_environment() {
        let chosen = Path::new("/custom/maple.toml");
        assert_eq!(config_path(Some(chosen)), chosen);
        // ponytail: the HURDLER_CONFIG / XDG_CONFIG_HOME arms read the real
        // environment, and std::env::set_var is unsafe and racy under the test
        // harness. Cover them with an integration test if they ever grow logic.
    }
}
