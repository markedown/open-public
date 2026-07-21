use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;

use server::config::Config;
use server::mail::Mailer;
use server::reviewer::{self, DeepSeekReviewer, DeferReviewer};
use server::state::AppState;

/// How often the background loop screens pending poll submissions.
const REVIEW_INTERVAL: Duration = Duration::from_secs(15);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env().context("loading configuration")?;
    let pool = db::connect(&config.database_url)
        .await
        .context("connecting to database")?;

    // A one-off command rather than the server: a fresh instance has no
    // accounts, and every route that could make one is behind sign-in or behind
    // the construction gate, so the first administrator has to be appointed
    // from the server itself.
    if let Some(email) = create_admin_arg() {
        let password = read_password()?;
        let outcome = server::admin_account::ensure_admin(
            &pool,
            config.app_secret.as_bytes(),
            &email,
            &password,
        )
        .await?;
        // The address is never printed back: it is not stored, and echoing it
        // would put it in a shell history or a log.
        match outcome {
            server::admin_account::Outcome::Created => println!("admin account created"),
            server::admin_account::Outcome::Promoted => println!("existing account promoted"),
        }
        return Ok(());
    }

    let mailer = Mailer::new(
        &config.mail_transport,
        config.mail_from.clone(),
        config.base_url.clone(),
    )
    .context("configuring mailer")?;

    // The directory that holds re-encoded uploaded images.
    std::fs::create_dir_all(&config.asset_dir)
        .with_context(|| format!("creating asset dir {}", config.asset_dir.display()))?;

    // Screen user-submitted polls in the background, so the model call never
    // sits in a request. Without a configured provider, submissions still flow
    // straight to the admin queue.
    match &config.review {
        Some(cfg) => {
            let r = DeepSeekReviewer::new(cfg).context("configuring the poll reviewer")?;
            tracing::info!("poll reviewer: {}", cfg.model);
            tokio::spawn(reviewer::run(pool.clone(), r, REVIEW_INTERVAL));
        }
        None => {
            tracing::info!("poll reviewer: none configured; submissions go straight to admins");
            tokio::spawn(reviewer::run(pool.clone(), DeferReviewer, REVIEW_INTERVAL));
        }
    }

    let state = AppState {
        pool,
        secret: Arc::new(config.app_secret.into_bytes()),
        mailer,
        cookie_secure: config.cookie_secure,
        asset_dir: Arc::new(config.asset_dir),
        site_notice: config.site_notice.map(Arc::from),
        construction: config.construction,
    };

    let router = server::app(state, &config.static_dir);

    let listener = tokio::net::TcpListener::bind(config.site_addr)
        .await
        .with_context(|| format!("binding {}", config.site_addr))?;
    tracing::info!("listening on http://{}", config.site_addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

/// `server create-admin <email>`, or nothing.
fn create_admin_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    (args.next().as_deref() == Some("create-admin")).then(|| args.next())?
}

/// The password, read from standard input so it never reaches a shell history
/// or a process listing.
fn read_password() -> anyhow::Result<String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("reading the password from stdin")?;
    let password = buf.trim_end_matches(['\n', '\r']).to_string();
    if password.is_empty() {
        anyhow::bail!("no password on stdin; pipe one in");
    }
    Ok(password)
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));
    fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
