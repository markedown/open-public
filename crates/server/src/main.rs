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
