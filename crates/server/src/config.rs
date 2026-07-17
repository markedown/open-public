use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;

/// How verification mail is delivered.
pub enum MailTransport {
    /// Log the verification link to the server log instead of sending. Dev default.
    Console,
    /// Send over SMTP with STARTTLS.
    Smtp {
        host: String,
        port: u16,
        user: String,
        pass: String,
    },
}

/// Runtime configuration, read from the environment.
pub struct Config {
    /// Address the HTTP server binds to (`SITE_ADDR`, default `127.0.0.1:3000`).
    pub site_addr: SocketAddr,
    /// Directory served under `/static` (vendored htmx, generated CSS).
    pub static_dir: PathBuf,
    /// Postgres connection string (`DATABASE_URL`).
    pub database_url: String,
    /// Secret used to HMAC-hash user emails (`APP_SECRET`).
    pub app_secret: String,
    /// Public origin used to build verification links (`PUBLIC_BASE_URL`).
    pub base_url: String,
    /// Whether session cookies carry the `Secure` attribute (derived from an https base URL).
    pub cookie_secure: bool,
    /// Mail delivery transport (`MAIL_TRANSPORT`).
    pub mail_transport: MailTransport,
    /// Envelope sender for verification mail (`MAIL_FROM`).
    pub mail_from: String,
    /// Directory for re-encoded uploaded images (`ASSET_DIR`, default `./data/assets`).
    pub asset_dir: PathBuf,
    /// Automated poll-review provider. `None` when no key is set, in which case
    /// submissions are deferred to the admin queue instead of auto-screened.
    pub review: Option<ReviewConfig>,
}

/// Settings for the automated poll reviewer (an OpenAI-compatible chat API).
#[derive(Clone)]
pub struct ReviewConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_source(|key| std::env::var(key).ok())
    }

    /// Build configuration from an arbitrary source. `from_env` passes the
    /// process environment; tests pass a map. This keeps parsing free of global
    /// state so every branch is testable.
    fn from_source(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let site_addr = get("SITE_ADDR")
            .unwrap_or_else(|| "127.0.0.1:3000".to_string())
            .parse()
            .context("SITE_ADDR must be `host:port`")?;

        let static_dir = match get("STATIC_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        };

        let database_url = get("DATABASE_URL").context("DATABASE_URL is not set")?;

        let app_secret = get("APP_SECRET").context("APP_SECRET is not set")?;
        if app_secret.trim().is_empty() {
            anyhow::bail!("APP_SECRET must not be empty");
        }
        if app_secret == "dev-only-change-me" {
            tracing::warn!(
                "APP_SECRET is the development default; set a random value before deploying"
            );
        }

        let base_url =
            get("PUBLIC_BASE_URL").unwrap_or_else(|| "http://127.0.0.1:3000".to_string());
        let base_url = base_url.trim_end_matches('/').to_string();
        let cookie_secure = base_url.starts_with("https://");

        let mail_transport = mail_transport(&get)?;
        let mail_from = get("MAIL_FROM").unwrap_or_else(|| "noreply@localhost".to_string());

        let asset_dir = match get("ASSET_DIR") {
            Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
            _ => PathBuf::from("./data/assets"),
        };

        // The reviewer is configured only when an API key is present. Without one
        // the server still runs; submissions simply wait for an admin.
        let review = get("DEEPSEEK_API_KEY")
            .filter(|k| !k.trim().is_empty())
            .map(|api_key| ReviewConfig {
                api_key,
                model: get("DEEPSEEK_MODEL").unwrap_or_else(|| "deepseek-chat".to_string()),
                base_url: get("DEEPSEEK_BASE_URL")
                    .unwrap_or_else(|| "https://api.deepseek.com".to_string())
                    .trim_end_matches('/')
                    .to_string(),
            });

        Ok(Self {
            site_addr,
            static_dir,
            database_url,
            app_secret,
            base_url,
            cookie_secure,
            mail_transport,
            mail_from,
            asset_dir,
            review,
        })
    }
}

fn mail_transport(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<MailTransport> {
    match get("MAIL_TRANSPORT")
        .unwrap_or_else(|| "console".to_string())
        .as_str()
    {
        "console" => Ok(MailTransport::Console),
        "smtp" => {
            let host =
                get("SMTP_HOST").context("SMTP_HOST is required when MAIL_TRANSPORT=smtp")?;
            let port = get("SMTP_PORT")
                .unwrap_or_else(|| "587".to_string())
                .parse()
                .context("SMTP_PORT must be a number")?;
            let user = get("SMTP_USER").unwrap_or_default();
            let pass = get("SMTP_PASS").unwrap_or_default();
            Ok(MailTransport::Smtp {
                host,
                port,
                user,
                pass,
            })
        }
        other => anyhow::bail!("MAIL_TRANSPORT must be `console` or `smtp`, got `{other}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn source(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |key| map.get(key).map(|v| v.to_string())
    }

    #[test]
    fn minimal_valid_config() {
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "postgres://x"),
            ("APP_SECRET", "s"),
        ]))
        .unwrap();
        assert_eq!(cfg.database_url, "postgres://x");
        assert!(!cfg.cookie_secure);
        assert!(matches!(cfg.mail_transport, MailTransport::Console));
        assert_eq!(cfg.mail_from, "noreply@localhost");
    }

    #[test]
    fn static_dir_override_and_dev_secret_warning() {
        // The dev-default secret path emits a warning; a STATIC_DIR is honored.
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "dev-only-change-me"),
            ("STATIC_DIR", "/custom/static"),
        ]))
        .unwrap();
        assert_eq!(cfg.static_dir, std::path::PathBuf::from("/custom/static"));
    }

    #[test]
    fn asset_dir_defaults_and_overrides() {
        let cfg =
            Config::from_source(source(&[("DATABASE_URL", "x"), ("APP_SECRET", "s")])).unwrap();
        assert_eq!(cfg.asset_dir, std::path::PathBuf::from("./data/assets"));
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("ASSET_DIR", "/srv/assets"),
        ]))
        .unwrap();
        assert_eq!(cfg.asset_dir, std::path::PathBuf::from("/srv/assets"));
    }

    #[test]
    fn review_is_configured_only_with_a_key() {
        let cfg =
            Config::from_source(source(&[("DATABASE_URL", "x"), ("APP_SECRET", "s")])).unwrap();
        assert!(cfg.review.is_none());
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("DEEPSEEK_API_KEY", "sk-test"),
            ("DEEPSEEK_BASE_URL", "https://api.deepseek.com/"),
        ]))
        .unwrap();
        let review = cfg.review.expect("review configured");
        assert_eq!(review.model, "deepseek-chat"); // default model
        assert_eq!(review.base_url, "https://api.deepseek.com"); // trailing slash trimmed
    }

    #[test]
    fn https_base_url_enables_secure_cookies_and_trims_slash() {
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("PUBLIC_BASE_URL", "https://open-public.com/"),
        ]))
        .unwrap();
        assert!(cfg.cookie_secure);
        assert_eq!(cfg.base_url, "https://open-public.com");
    }

    #[test]
    fn missing_database_url_errors() {
        assert!(Config::from_source(source(&[("APP_SECRET", "s")])).is_err());
    }

    #[test]
    fn missing_or_empty_app_secret_errors() {
        assert!(Config::from_source(source(&[("DATABASE_URL", "x")])).is_err());
        assert!(
            Config::from_source(source(&[("DATABASE_URL", "x"), ("APP_SECRET", "   ")])).is_err()
        );
    }

    #[test]
    fn bad_site_addr_errors() {
        assert!(Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("SITE_ADDR", "not-an-addr"),
        ]))
        .is_err());
    }

    #[test]
    fn smtp_transport_is_parsed() {
        let cfg = Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("MAIL_TRANSPORT", "smtp"),
            ("SMTP_HOST", "smtp.example.com"),
            ("SMTP_PORT", "2525"),
        ]))
        .unwrap();
        match cfg.mail_transport {
            MailTransport::Smtp { host, port, .. } => {
                assert_eq!(host, "smtp.example.com");
                assert_eq!(port, 2525);
            }
            MailTransport::Console => panic!("expected smtp"),
        }
    }

    #[test]
    fn smtp_without_host_errors() {
        assert!(Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("MAIL_TRANSPORT", "smtp"),
        ]))
        .is_err());
    }

    #[test]
    fn bad_smtp_port_errors() {
        assert!(Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("MAIL_TRANSPORT", "smtp"),
            ("SMTP_HOST", "h"),
            ("SMTP_PORT", "not-a-port"),
        ]))
        .is_err());
    }

    #[test]
    fn unknown_mail_transport_errors() {
        assert!(Config::from_source(source(&[
            ("DATABASE_URL", "x"),
            ("APP_SECRET", "s"),
            ("MAIL_TRANSPORT", "carrier-pigeon"),
        ]))
        .is_err());
    }
}
