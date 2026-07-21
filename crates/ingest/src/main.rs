use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    dotenvy::dotenv().ok();
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("import") => {
            let dir = PathBuf::from(args.next().unwrap_or_else(|| "dataset".to_string()));
            match import(&dir).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("import failed: {e:#}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(other) => {
            eprintln!("unknown task: {other}");
            usage();
            ExitCode::FAILURE
        }
        None => {
            usage();
            ExitCode::FAILURE
        }
    }
}

async fn import(dir: &std::path::Path) -> anyhow::Result<()> {
    let url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is not set"))?;
    let pool = sqlx::PgPool::connect(&url).await?;
    let loaded = ingest::import::run(&pool, dir).await?;
    println!(
        "loaded {} rows from {}: {} sources, {} people, {} parties, {} theses, {} evidence",
        loaded.total(),
        dir.display(),
        loaded.sources,
        loaded.people,
        loaded.parties,
        loaded.theses,
        loaded.evidence,
    );
    Ok(())
}

fn usage() {
    eprintln!("usage: ingest <task>");
    eprintln!();
    eprintln!("tasks:");
    eprintln!("  import [dir]   load the published dataset (default: dataset/)");
}
