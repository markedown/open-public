//! The published dataset must be loadable, and loading it must be repeatable.
//!
//! This runs against the real files in `dataset/`, so it is the check that the
//! published data and the schema it describes have not drifted apart. It also
//! asserts idempotency, because a dataset that cannot be loaded twice is a
//! dataset nobody can keep up to date.

use std::path::Path;

/// Row counts of every table the dataset writes to, as one static query so no
/// SQL is built from a string at runtime.
async fn counts(pool: &sqlx::PgPool) -> Vec<(String, i64)> {
    sqlx::query_as::<_, (String, i64)>(
        "select 'sources', count(*) from sources
         union all select 'countries', count(*) from countries
         union all select 'people', count(*) from people
         union all select 'parties', count(*) from parties
         union all select 'party_memberships', count(*) from party_memberships
         union all select 'roles', count(*) from roles
         union all select 'elections', count(*) from elections
         union all select 'election_results', count(*) from election_results
         union all select 'theses', count(*) from theses
         union all select 'position_evidence', count(*) from position_evidence
         union all select 'person_attributes', count(*) from person_attributes
         union all select 'person_education', count(*) from person_education
         order by 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_published_dataset_loads_into_an_empty_database(pool: sqlx::PgPool) {
    let dir = Path::new("../../dataset");
    let loaded = ingest::import::run(&pool, dir)
        .await
        .expect("the published dataset loads");

    assert!(loaded.countries > 0, "countries were loaded");
    assert!(loaded.people > 0, "people were loaded");
    assert!(loaded.sources > 0, "sources were loaded");

    let after = counts(&pool).await;
    for (table, n) in &after {
        assert!(*n > 0, "{table} is not empty after the import");
    }

    // Every fact points at a source that came with the dataset, and every
    // reference resolves: the schema enforces both, so an import that got this
    // far has proved it for real rows rather than for a fixture.
    let orphans: i64 = sqlx::query_scalar(
        "select count(*) from position_evidence e
         left join theses t on t.id = e.thesis_id where t.id is null",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(orphans, 0, "no evidence is left without its thesis");

    // Loading the same dataset again changes nothing, which is what lets a
    // database be brought up to date by rerunning the import.
    ingest::import::run(&pool, dir)
        .await
        .expect("the dataset loads a second time");
    assert_eq!(after, counts(&pool).await, "the second import changed rows");
}

/// Write a minimal dataset to a temporary directory.
fn write_dataset(dir: &Path, files: &[(&str, &str)]) {
    std::fs::create_dir_all(dir).unwrap();
    for (name, body) in files {
        std::fs::write(dir.join(format!("{name}.json")), body).unwrap();
    }
}

const ONE_SOURCE: &str = r#"[{"url":"https://example.test/doc","content_hash":"h1",
  "content_sha256":null,"snapshot_url":null,"kind":"manual","title":"Doc",
  "outlet":null,"fetched_at":null,"published_at":null}]"#;
const ONE_COUNTRY: &str = r#"[{"slug":"tt","name":"Test Ulke","capital":null,
  "government_type":null,"legislature_name":null,"founded_date":null,"population":null,
  "summary":null,"flag_url":null,"source_url":"https://example.test/doc","source_hash":"h1"}]"#;

#[sqlx::test(migrations = "../../migrations")]
async fn a_fact_whose_source_is_missing_is_refused(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-no-source");
    let _ = std::fs::remove_dir_all(&dir);
    // The country cites a document that is not in the export. Loading it would
    // put an unsourced fact in the database, which the whole dataset promises
    // cannot happen, so the import refuses rather than inventing a source.
    write_dataset(&dir, &[("sources", "[]"), ("countries", ONE_COUNTRY)]);

    let err = ingest::import::run(&pool, &dir)
        .await
        .expect_err("an unsourced fact is refused");
    assert!(
        err.to_string().contains("source"),
        "the error says what is wrong: {err}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_reference_that_points_at_nothing_is_refused(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-dangling");
    let _ = std::fs::remove_dir_all(&dir);
    write_dataset(
        &dir,
        &[
            ("sources", ONE_SOURCE),
            ("countries", ONE_COUNTRY),
            (
                "party_memberships",
                r#"[{"person":"nobody","party":"nothing","start_date":null,"end_date":null,
                     "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
            ),
        ],
    );

    let err = ingest::import::run(&pool, &dir)
        .await
        .expect_err("a dangling reference is refused");
    assert!(err.to_string().contains("not in the dataset"), "{err}");

    // The country before it did load, so the failure is about the bad row and
    // not about the file being unreadable.
    let n: i64 = sqlx::query_scalar("select count(*) from countries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_directory_with_nothing_in_it_loads_nothing(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let loaded = ingest::import::run(&pool, &dir).await.unwrap();
    assert_eq!(loaded.total(), 0);
}
