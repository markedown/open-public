use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("wikidata-seed") => {
            eprintln!("wikidata-seed: not implemented yet");
            ExitCode::FAILURE
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

fn usage() {
    eprintln!("usage: ingest <task>");
    eprintln!();
    eprintln!("tasks:");
    eprintln!("  wikidata-seed   seed the initial dataset from Wikidata");
}
