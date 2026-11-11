//! Configuration: flags > environment > user config file > system config file.
//!
//! The token is never printed: [`Secret`] redacts itself in `Debug`, and TOML
//! parse errors only report the message and line, never the offending source.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const SYSTEM_CONFIG: &str = "/etc/monarchctl/config.toml";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

/// A string that must never be shown.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// One config file. Every key is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub url: Option<String>,
    pub token_file: Option<PathBuf>,
    pub token_command: Option<String>,
    /// Literal token. Works, but `token_file` is the recommended way.
    pub token: Option<Secret>,
    pub tls_skip_verify: Option<bool>,
    pub timeout: Option<u64>,
}

impl FileConfig {
    fn has_token_source(&self) -> bool {
        self.token_file.is_some() || self.token_command.is_some() || self.token.is_some()
    }

    /// Layers `top` over `self`. The token keys move together: a file that sets
    /// any of them replaces the token source of the lower layer completely.
    #[must_use]
    pub fn overlaid_by(self, top: Self) -> Self {
        let (token_file, token_command, token) = if top.has_token_source() {
            (top.token_file, top.token_command, top.token)
        } else {
            (self.token_file, self.token_command, self.token)
        };
        Self {
            url: top.url.or(self.url),
            token_file,
            token_command,
            token,
            tls_skip_verify: top.tls_skip_verify.or(self.tls_skip_verify),
            timeout: top.timeout.or(self.timeout),
        }
    }

    pub fn parse(path: &Path, text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|err| {
            let line = err
                .span()
                .map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
            match line {
                Some(line) => anyhow::anyhow!("{}:{line}: {}", path.display(), err.message()),
                None => anyhow::anyhow!("{}: {}", path.display(), err.message()),
            }
        })
    }

    fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(path, &text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
        }
    }
}

/// `$XDG_CONFIG_HOME/monarchctl/config.toml`, falling back to `~/.config`.
pub fn user_config_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match (xdg, home) {
        (Some(xdg), _) if !xdg.is_empty() => PathBuf::from(xdg),
        (_, Some(home)) if !home.is_empty() => Path::new(home).join(".config"),
        _ => return None,
    };
    Some(base.join("monarchctl").join("config.toml"))
}

/// Both config files merged, the user's over the system's.
pub fn load_files() -> Result<FileConfig> {
    let system = FileConfig::load(Path::new(SYSTEM_CONFIG))?;
    let user = match user_config_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    ) {
        Some(path) => FileConfig::load(&path)?,
        None => FileConfig::default(),
    };
    Ok(system.overlaid_by(user))
}

/// Settings from command line flags.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    pub url: Option<String>,
    pub token_file: Option<PathBuf>,
    pub insecure: bool,
}

/// Settings from `MONARCH_*` environment variables.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub url: Option<String>,
    pub token: Option<Secret>,
    pub token_file: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Self {
            url: var("MONARCH_URL"),
            token: var("MONARCH_TOKEN").map(Secret::new),
            token_file: var("MONARCH_TOKEN_FILE").map(PathBuf::from),
        }
    }
}

/// Where the token comes from. Resolved lazily, see [`TokenSource::fetch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSource {
    Literal(Secret),
    File(PathBuf),
    Command(String),
}

impl TokenSource {
    pub async fn fetch(&self) -> Result<Secret> {
        let raw = match self {
            Self::Literal(secret) => secret.expose().to_owned(),
            Self::File(path) => tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("reading token file {}", path.display()))?,
            Self::Command(command) => run_token_command(command).await?,
        };
        // Files usually end in a newline; commands may print more than the token.
        let token = raw.lines().next().unwrap_or_default().trim();
        if token.is_empty() {
            bail!("the token is empty");
        }
        Ok(Secret::new(token))
    }
}

async fn run_token_command(command: &str) -> Result<String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .output()
        .await
        .context("running token_command")?;
    if !output.status.success() {
        bail!("token_command failed ({})", output.status);
    }
    String::from_utf8(output.stdout).context("token_command printed invalid UTF-8")
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub url: String,
    pub token: TokenSource,
    pub tls_skip_verify: bool,
    pub timeout: Duration,
}

pub fn resolve(flags: Flags, env: Env, file: FileConfig) -> Result<Resolved> {
    let url = flags
        .url
        .or(env.url)
        .or(file.url)
        .map(|url| url.trim().trim_end_matches('/').to_owned())
        .filter(|url| !url.is_empty())
        .context(
            "no Monarch URL configured: pass --url, set MONARCH_URL or add `url` to the config file",
        )?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("the URL must start with http:// or https://");
    }

    let token = flags
        .token_file
        .map(TokenSource::File)
        .or_else(|| env.token.map(TokenSource::Literal))
        .or_else(|| env.token_file.map(TokenSource::File))
        .or_else(|| file.token_file.map(TokenSource::File))
        .or_else(|| file.token_command.map(TokenSource::Command))
        .or_else(|| file.token.map(TokenSource::Literal))
        .context(
            "no API token configured: pass --token-file, set MONARCH_TOKEN or MONARCH_TOKEN_FILE, \
             or add `token_file` to the config file",
        )?;

    Ok(Resolved {
        url,
        token,
        tls_skip_verify: flags.insecure || file.tls_skip_verify.unwrap_or(false),
        timeout: Duration::from_secs(file.timeout.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> FileConfig {
        FileConfig::parse(Path::new("test.toml"), text).unwrap()
    }

    #[test]
    fn flags_beat_env_beat_files() {
        let files = file("url = \"http://file\"\ntoken_file = \"/file/token\"");
        let env = Env {
            url: Some("http://env".into()),
            token: None,
            token_file: Some("/env/token".into()),
        };

        let r = resolve(Flags::default(), Env::default(), files.clone()).unwrap();
        assert_eq!(r.url, "http://file");
        assert_eq!(r.token, TokenSource::File("/file/token".into()));

        let r = resolve(Flags::default(), env.clone(), files.clone()).unwrap();
        assert_eq!(r.url, "http://env");
        assert_eq!(r.token, TokenSource::File("/env/token".into()));

        let flags = Flags {
            url: Some("http://flag/".into()),
            token_file: Some("/flag/token".into()),
            insecure: false,
        };
        let r = resolve(flags, env, files).unwrap();
        assert_eq!(r.url, "http://flag");
        assert_eq!(r.token, TokenSource::File("/flag/token".into()));
    }

    #[test]
    fn env_token_beats_env_token_file() {
        let env = Env {
            url: Some("https://m".into()),
            token: Some(Secret::new("mnr_x")),
            token_file: Some("/f".into()),
        };
        let r = resolve(Flags::default(), env, FileConfig::default()).unwrap();
        assert_eq!(r.token, TokenSource::Literal(Secret::new("mnr_x")));
    }

    #[test]
    fn token_keys_within_a_file() {
        let r = resolve(
            Flags::default(),
            Env::default(),
            file("url = \"http://m\"\ntoken = \"mnr_a\"\ntoken_command = \"pass monarch\"\ntoken_file = \"/t\""),
        )
        .unwrap();
        assert_eq!(r.token, TokenSource::File("/t".into()));
        let r = resolve(
            Flags::default(),
            Env::default(),
            file("url = \"http://m\"\ntoken = \"mnr_a\"\ntoken_command = \"pass monarch\""),
        )
        .unwrap();
        assert_eq!(r.token, TokenSource::Command("pass monarch".into()));
    }

    #[test]
    fn user_file_replaces_the_system_token_source_as_a_whole() {
        let system = file("url = \"http://sys\"\ntoken_file = \"/etc/token\"\ntimeout = 5");
        let user = file("token_command = \"echo mnr_u\"");
        let merged = system.overlaid_by(user);
        let r = resolve(Flags::default(), Env::default(), merged).unwrap();
        assert_eq!(r.url, "http://sys");
        assert_eq!(r.token, TokenSource::Command("echo mnr_u".into()));
        assert_eq!(r.timeout, Duration::from_secs(5));
    }

    #[test]
    fn user_file_without_token_keeps_the_system_token() {
        let system = file("token_file = \"/etc/token\"");
        let user = file("url = \"http://u\"\ntls_skip_verify = true");
        let r = resolve(Flags::default(), Env::default(), system.overlaid_by(user)).unwrap();
        assert_eq!(r.token, TokenSource::File("/etc/token".into()));
        assert!(r.tls_skip_verify);
    }

    #[test]
    fn missing_url_or_token_is_an_error() {
        assert!(resolve(Flags::default(), Env::default(), FileConfig::default()).is_err());
        let err = resolve(
            Flags {
                url: Some("http://m".into()),
                ..Flags::default()
            },
            Env::default(),
            FileConfig::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("token"));
        let err = resolve(
            Flags {
                url: Some("monarch.example.com".into()),
                ..Flags::default()
            },
            Env {
                token: Some(Secret::new("t")),
                ..Env::default()
            },
            FileConfig::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("http"));
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let cfg = file("token = \"mnr_supersecret\"");
        assert!(!format!("{cfg:?}").contains("supersecret"));
        let r = resolve(
            Flags {
                url: Some("http://m".into()),
                ..Flags::default()
            },
            Env::default(),
            cfg,
        )
        .unwrap();
        assert!(!format!("{r:?}").contains("supersecret"));
    }

    #[test]
    fn parse_errors_do_not_echo_the_source() {
        let err = FileConfig::parse(
            Path::new("c.toml"),
            "token = \"mnr_supersecret\"\ntimeout = \"soon\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("c.toml:2"), "{err}");
        assert!(!err.contains("supersecret"), "{err}");
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert_eq!(
            file("colour = \"red\"\nurl = \"http://m\"").url.as_deref(),
            Some("http://m")
        );
    }

    #[test]
    fn xdg_path() {
        assert_eq!(
            user_config_path(Some("/x"), Some("/home/u")),
            Some(PathBuf::from("/x/monarchctl/config.toml"))
        );
        assert_eq!(
            user_config_path(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/monarchctl/config.toml"))
        );
        assert_eq!(user_config_path(Some(""), None), None);
    }

    #[tokio::test]
    async fn token_sources_are_trimmed_to_the_first_line() {
        let literal = TokenSource::Literal(Secret::new("  mnr_a \nignored"));
        assert_eq!(literal.fetch().await.unwrap().expose(), "mnr_a");
        let command = TokenSource::Command("printf 'mnr_cmd\\nmore\\n'".into());
        assert_eq!(command.fetch().await.unwrap().expose(), "mnr_cmd");
        assert!(TokenSource::Command("true".into()).fetch().await.is_err());
        assert!(TokenSource::Command("exit 3".into()).fetch().await.is_err());
        assert!(
            TokenSource::File("/nonexistent/monarch-token".into())
                .fetch()
                .await
                .is_err()
        );
    }
}
