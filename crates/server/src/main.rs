use std::sync::Arc;

use anyhow::Context;

use server::config::Config;
use server::mail::Mailer;
use server::state::AppState;

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

    let state = AppState {
        pool,
        secret: Arc::new(config.app_secret.into_bytes()),
        mailer,
        cookie_secure: config.cookie_secure,
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
