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

// Past this point the datasets are invented and small. The published dataset
// contains no bad rows by construction, so the only way to exercise the ways a
// row can fail to resolve is to write rows that do not resolve.

const ONE_PERSON: &str = r#"[{"slug":"ayse-yilmaz","full_name":"Ayşe Yılmaz","country":"tt",
  "birth_date":"1975-04-03","birth_place":null,"photo_url":null,"photo_license":null,
  "summary":null,"wikidata_id":null,
  "source_url":"https://example.test/doc","source_hash":"h1"}]"#;
const ONE_PARTY: &str = r#"[{"slug":"test-partisi","name":"Test Partisi","short_name":"TP",
  "country":"tt","founded_date":"2020-01-01","dissolved_date":null,"ideology_tags":[],
  "color":null,"summary":"2020'de kurulmus bir test partisi.","wikidata_id":null,
  "source_url":"https://example.test/doc","source_hash":"h1"}]"#;
const ONE_THESIS: &str = r#"[{"country":"tt","text":"Vergi oranları düşürülmeli.","position":1,
  "scope":"party","topic":null,
  "source_url":"https://example.test/doc","source_hash":"h1"}]"#;

/// The rows the cases below hang off. They always carry the same natural keys, so
/// a dataset refused partway through leaves a database the next case can build on
/// rather than one it trips over.
const BASE: &[(&str, &str)] = &[
    ("sources", ONE_SOURCE),
    ("countries", ONE_COUNTRY),
    ("people", ONE_PERSON),
    ("parties", ONE_PARTY),
];

/// Import `BASE` plus `extra` from a fresh temporary directory and hand back the
/// error, which is what such a case is about.
async fn refusal(pool: &sqlx::PgPool, case: &str, extra: &[(&str, &str)]) -> String {
    let dir = std::env::temp_dir().join(format!("op-import-{case}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut files = BASE.to_vec();
    files.extend_from_slice(extra);
    write_dataset(&dir, &files);

    let err = ingest::import::run(pool, &dir)
        .await
        .expect_err("a row that resolves to nothing is refused");
    format!("{err:#}")
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_row_that_names_an_absent_country_is_refused(pool: sqlx::PgPool) {
    let cases = [
        (
            "election-country",
            "elections",
            r#"[{"slug":"secim-1","name":"Test Secimi","country":"yok","held_on":null,
              "kind":null,"description":null,"expected_note":null,"electorate":null,
              "votes_cast":null,"valid_votes":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "alliance-country",
            "alliances",
            r#"[{"slug":"ittifak-1","name":"Test Ittifakı","country":"yok",
              "founded_date":null,"dissolved_date":null,"summary":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "thesis-country",
            "theses",
            r#"[{"country":"yok","text":"Test tezi.","position":1,"scope":"party","topic":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "roll-call-country",
            "roll_calls",
            r#"[{"slug":"ot-1","country":"yok","chamber":"Test Meclisi","held_on":"2024-01-15",
              "title":"Test oylaması","yes":[],"no":[],"abstain":[],"absent":[],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "initiative-country",
            "initiatives",
            r#"[{"slug":"gk-1","country":"yok","kind":"bill","title":"Test Kanun Teklifi",
              "document":"https://example.test/doc","tabled_on":"2024-01-05","status":null,
              "by_government":false,"topics":[],"parties":[],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "group-vote-country",
            "group_votes",
            r#"[{"slug":"go-1","country":"yok","chamber":"Test Meclisi","held_on":"2024-01-15",
              "title":"Test grup oylaması","document":null,"initiative":null,"adopted":null,
              "excerpt":"Evet 3, hayır 1.","locator":null,"for":[],"against":[],"abstain":[],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "place-country",
            "constituency_places",
            r#"[{"country":"yok","place":"Test Ilce","part":null,"postal_code":null,
              "district":"Test Bölgesi","region":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
    ];

    for (case, file, body) in cases {
        let err = refusal(&pool, case, &[(file, body)]).await;
        assert!(
            err.contains("names a country not in the dataset"),
            "{case}: {err}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_row_that_names_an_absent_person_is_refused(pool: sqlx::PgPool) {
    let cases = [
        (
            "role-person",
            "roles",
            r#"[{"person":"yok","role_type":"mp","title":null,"org":null,"district":null,
              "start_date":null,"end_date":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "attribute-person",
            "person_attributes",
            r#"[{"person":"yok","kind":"meslek","value":"Avukat","value_wikidata_id":null,
              "start_date":null,"end_date":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "education-person",
            "person_education",
            r#"[{"person":"yok","institution":"Test Universitesi",
              "institution_wikidata_id":null,"degree":null,"field":null,"start_date":null,
              "end_date":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "roll-call-person",
            "roll_calls",
            r#"[{"slug":"ot-2","country":"tt","chamber":"Test Meclisi","held_on":"2024-01-16",
              "title":"Test oylaması","yes":["yok"],"no":[],"abstain":[],"absent":[],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
    ];

    for (case, file, body) in cases {
        let err = refusal(&pool, case, &[(file, body)]).await;
        assert!(err.contains("not in the dataset"), "{case}: {err}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_row_that_names_an_absent_parent_is_refused(pool: sqlx::PgPool) {
    let cases = [
        (
            "membership-party",
            "party_alliances",
            r#"[{"party":"test-partisi","alliance":"yok","start_date":null,"end_date":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "result-election",
            "election_results",
            r#"[{"election":"yok","party":"test-partisi","label":null,"seats":null,"votes":null,
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "initiative-party",
            "initiatives",
            r#"[{"slug":"gk-2","country":"tt","kind":"motion","title":"Test Önergesi",
              "document":"https://example.test/doc","tabled_on":"2024-01-06","status":null,
              "by_government":false,"topics":[],"parties":["yok"],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "group-vote-party",
            "group_votes",
            r#"[{"slug":"go-2","country":"tt","chamber":"Test Meclisi","held_on":"2024-01-16",
              "title":"Test grup oylaması","document":null,"initiative":null,"adopted":null,
              "excerpt":"Evet 3, hayır 1.","locator":null,"for":["yok"],"against":[],
              "abstain":[],
              "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        ),
        (
            "option-poll",
            "poll_options",
            r#"[{"poll":"yok","label":"Evet","position":1}]"#,
        ),
    ];

    for (case, file, body) in cases {
        let err = refusal(&pool, case, &[(file, body)]).await;
        assert!(err.contains("not in the dataset"), "{case}: {err}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn evidence_and_vote_readings_that_name_nothing_are_refused(pool: sqlx::PgPool) {
    // The thesis is named by its country and its text, so a piece of evidence
    // whose text is not one of the exported theses has nothing to attach to.
    let err = refusal(
        &pool,
        "evidence-thesis",
        &[(
            "position_evidence",
            r#"[{"country":"tt","thesis":"Yok tezi.","party":"test-partisi","person":null,
                  "kind":"statement","stance":1,"quote":null,"locator":null,"occurred_on":null,
                  "roll_call":null,
                  "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
        )],
    )
    .await;
    assert!(
        err.contains("evidence names a thesis not in the dataset"),
        "{err}"
    );

    // Evidence may point at a roll call to record the action behind it. The
    // pointer is set in a second pass over the same file, once roll calls have
    // loaded, and a slug that is not one of them is refused there.
    let err = refusal(
        &pool,
        "evidence-roll-call",
        &[
            ("theses", ONE_THESIS),
            (
                "position_evidence",
                r#"[{"country":"tt","thesis":"Vergi oranları düşürülmeli.",
                  "party":"test-partisi","person":null,"kind":"vote","stance":1,
                  "quote":"Evet.","locator":"s. 4","occurred_on":"2024-01-15",
                  "roll_call":"yok","source_url":"https://example.test/doc","source_hash":"h1"}]"#,
            ),
        ],
    )
    .await;
    assert!(
        err.contains("evidence names a roll call not in the dataset: yok"),
        "{err}"
    );

    let err = refusal(
        &pool,
        "reading-thesis",
        &[(
            "thesis_roll_calls",
            r#"[{"country":"tt","thesis":"Yok tezi.","roll_call":"ot-1","yes_stance":1,
              "no_stance":-1,"note":"Evet tarafı tezi destekliyor."}]"#,
        )],
    )
    .await;
    assert!(
        err.contains("a vote reading names a thesis not in the dataset"),
        "{err}"
    );

    let err = refusal(
        &pool,
        "reading-roll-call",
        &[
            ("theses", ONE_THESIS),
            (
                "thesis_roll_calls",
                r#"[{"country":"tt","thesis":"Vergi oranları düşürülmeli.","roll_call":"yok",
                  "yes_stance":null,"no_stance":null,"note":"Oylama bulunamadı."}]"#,
            ),
        ],
    )
    .await;
    assert!(
        err.contains("a vote reading names a roll call not in the dataset: yok"),
        "{err}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_translation_of_an_absent_entity_is_refused(pool: sqlx::PgPool) {
    // A translation names its entity by slug and type, so both an unknown type
    // and a known type whose row is absent leave it nowhere to attach to.
    let err = refusal(
        &pool,
        "translation-kind",
        &[(
            "translations",
            r#"[{"entity":"test-partisi","entity_type":"statement","field":"summary",
              "lang":"en","text":"A statement.","origin":null,"source_lang":null}]"#,
        )],
    )
    .await;
    assert!(
        err.contains("translation names a statement not in the dataset: test-partisi"),
        "{err}"
    );

    let err = refusal(
        &pool,
        "translation-entity",
        &[(
            "translations",
            r#"[{"entity":"yok-anket","entity_type":"poll","field":"question","lang":"en",
              "text":"A question?","origin":"human","source_lang":"tr"}]"#,
        )],
    )
    .await;
    assert!(
        err.contains("translation names a poll not in the dataset: yok-anket"),
        "{err}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_value_is_refused_before_anything_loads(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-malformed");
    let _ = std::fs::remove_dir_all(&dir);
    // A date that is not a date never reaches the database: the row does not
    // deserialize, and the error names the file it came from.
    write_dataset(
        &dir,
        &[
            ("sources", ONE_SOURCE),
            (
                "people",
                r#"[{"slug":"ayse-yilmaz","full_name":"Ayşe Yılmaz","country":null,
                  "birth_date":"dün","birth_place":null,"photo_url":null,
                  "photo_license":null,"summary":null,"wikidata_id":null,
                  "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
            ),
        ],
    );

    let err = ingest::import::run(&pool, &dir)
        .await
        .expect_err("a value that does not parse is refused");
    assert!(err.to_string().contains("parsing people.json"), "{err:#}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn files_that_hold_empty_lists_load_nothing(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-empty-lists");
    let _ = std::fs::remove_dir_all(&dir);
    let names = [
        "sources",
        "countries",
        "people",
        "parties",
        "party_memberships",
        "roles",
        "elections",
        "election_results",
        "alliances",
        "party_alliances",
        "events",
        "topics",
        "statements",
        "outlets",
        "person_attributes",
        "person_education",
        "theses",
        "position_evidence",
        "polls",
        "poll_options",
        "roll_calls",
        "initiatives",
        "group_votes",
        "thesis_roll_calls",
        "constituency_places",
        "translations",
    ];
    let files: Vec<(&str, &str)> = names.iter().map(|name| (*name, "[]")).collect();
    write_dataset(&dir, &files);

    let loaded = ingest::import::run(&pool, &dir).await.unwrap();
    assert_eq!(loaded.total(), 0, "an empty list contributes no rows");
}

/// One row of every kind the importer knows, small enough to read, so a rerun can
/// be checked against it rather than only against the published dataset.
const FIXTURE: &[(&str, &str)] = &[
    ("sources", ONE_SOURCE),
    ("countries", ONE_COUNTRY),
    ("people", ONE_PERSON),
    ("parties", ONE_PARTY),
    (
        "party_memberships",
        r#"[{"person":"ayse-yilmaz","party":"test-partisi","start_date":"2020-06-01",
          "end_date":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "roles",
        r#"[{"person":"ayse-yilmaz","role_type":"mp","title":"Milletvekili",
          "org":"Test Meclisi","district":"Test Bölgesi","start_date":"2023-06-01",
          "end_date":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    ("topics", r#"[{"slug":"vergi","name":"Vergi"}]"#),
    (
        "statements",
        r#"[{"person":"ayse-yilmaz","party":null,"topic":"vergi",
          "text_original":"Vergi oranı düşürülmeli.","is_paraphrase":false,
          "stated_at":"2024-02-01",
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "outlets",
        r#"[{"slug":"test-gazete","name":"Test Gazetesi","country":"tt",
          "homepage_url":null,"logo_url":null,"logo_license":null,"leaning":"center",
          "summary":null,"leaning_source_url":"https://example.test/doc",
          "leaning_source_hash":"h1"}]"#,
    ),
    (
        "events",
        r#"[{"country":"tt","party":null,"person":null,"kind":"kurultay",
          "title":"Test Kurultayı","happened_on":"2024-05-01",
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "elections",
        r#"[{"slug":"secim-1","name":"Test Secimi","country":"tt","held_on":"2023-06-01",
          "kind":"genel","description":null,"expected_note":null,"electorate":1000,
          "votes_cast":600,"valid_votes":580,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "election_results",
        r#"[{"election":"secim-1","party":"test-partisi","label":null,"seats":10,"votes":300,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "alliances",
        r#"[{"slug":"ittifak-1","name":"Test Ittifakı","country":"tt",
          "founded_date":"2023-01-01","dissolved_date":null,"summary":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "party_alliances",
        r#"[{"party":"test-partisi","alliance":"ittifak-1","start_date":"2023-01-01",
          "end_date":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "person_attributes",
        r#"[{"person":"ayse-yilmaz","kind":"meslek","value":"Avukat",
          "value_wikidata_id":null,"start_date":null,"end_date":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "person_education",
        r#"[{"person":"ayse-yilmaz","institution":"Test Universitesi",
          "institution_wikidata_id":null,"degree":"lisans","field":"Hukuk",
          "start_date":"1993-09-01","end_date":"1997-06-01",
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    ("theses", ONE_THESIS),
    (
        "position_evidence",
        r#"[{"country":"tt","thesis":"Vergi oranları düşürülmeli.","party":"test-partisi",
          "person":null,"kind":"vote","stance":1,"quote":"Evet.","locator":"s. 4",
          "occurred_on":"2024-01-15","roll_call":"ot-1",
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "polls",
        r#"[{"slug":"anket-1","question":"Test anketi?","country":"tt","party":null,
          "person":null,"kind":"yesno","opens_at":"2026-01-01T00:00:00Z",
          "closes_at":"2026-02-01T00:00:00Z"}]"#,
    ),
    (
        "poll_options",
        r#"[{"poll":"anket-1","label":"Evet","position":1},
           {"poll":"anket-1","label":"Hayır","position":2}]"#,
    ),
    (
        "roll_calls",
        r#"[{"slug":"ot-1","country":"tt","chamber":"Test Meclisi","held_on":"2024-01-15",
          "title":"Vergi oylaması","yes":["ayse-yilmaz"],"no":[],"abstain":[],"absent":[],
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "thesis_roll_calls",
        r#"[{"country":"tt","thesis":"Vergi oranları düşürülmeli.","roll_call":"ot-1",
          "yes_stance":1,"no_stance":-1,"note":"Evet tarafı tezi destekliyor."}]"#,
    ),
    (
        "initiatives",
        r#"[{"slug":"gk-1","country":"tt","kind":"bill","title":"Test Kanun Teklifi",
          "document":"https://example.test/doc","tabled_on":"2024-01-05",
          "status":"komisyonda","by_government":false,"topics":["vergi"],
          "parties":["test-partisi"],
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "group_votes",
        r#"[{"slug":"go-1","country":"tt","chamber":"Test Meclisi","held_on":"2024-01-15",
          "title":"Test grup oylaması","document":null,"initiative":"gk-1","adopted":true,
          "excerpt":"Evet 3, hayır 1.","locator":"s. 2","for":["test-partisi"],
          "against":[],"abstain":[],
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "constituency_places",
        r#"[{"country":"tt","place":"Test Ilce","part":null,"postal_code":null,
          "district":"Test Bölgesi","region":null,
          "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
    ),
    (
        "translations",
        r#"[{"entity":"test-partisi","entity_type":"party","field":"summary","lang":"en",
          "text":"A test party.","origin":"human","source_lang":"tr"}]"#,
    ),
];

/// Row counts of the tables the fixture writes, limited to those the schema gives
/// a natural key. statements and events have none, so their conflict clause can
/// never fire and a rerun appends them rather than leaving them be.
async fn fixture_counts(pool: &sqlx::PgPool) -> Vec<(String, i64)> {
    sqlx::query_as::<_, (String, i64)>(
        "select 'alliances', count(*) from alliances
         union all select 'constituency_places', count(*) from constituency_places
         union all select 'countries', count(*) from countries
         union all select 'election_results', count(*) from election_results
         union all select 'elections', count(*) from elections
         union all select 'group_vote_parties', count(*) from group_vote_parties
         union all select 'group_votes', count(*) from group_votes
         union all select 'initiative_parties', count(*) from initiative_parties
         union all select 'initiatives', count(*) from initiatives
         union all select 'outlets', count(*) from outlets
         union all select 'party_alliances', count(*) from party_alliances
         union all select 'party_memberships', count(*) from party_memberships
         union all select 'parties', count(*) from parties
         union all select 'people', count(*) from people
         union all select 'person_attributes', count(*) from person_attributes
         union all select 'person_education', count(*) from person_education
         union all select 'poll_options', count(*) from poll_options
         union all select 'polls', count(*) from polls
         union all select 'position_evidence', count(*) from position_evidence
         union all select 'roll_call_votes', count(*) from roll_call_votes
         union all select 'roll_calls', count(*) from roll_calls
         union all select 'roles', count(*) from roles
         union all select 'sources', count(*) from sources
         union all select 'theses', count(*) from theses
         union all select 'thesis_roll_calls', count(*) from thesis_roll_calls
         union all select 'topics', count(*) from topics
         union all select 'translations', count(*) from translations
         order by 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_fixture_dataset_loads_twice_and_an_update_replaces_in_place(pool: sqlx::PgPool) {
    let dir = std::env::temp_dir().join("op-import-fixture");
    let _ = std::fs::remove_dir_all(&dir);
    write_dataset(&dir, FIXTURE);

    let first = ingest::import::run(&pool, &dir)
        .await
        .expect("the fixture loads");
    assert_eq!(first.people, 1);
    assert_eq!(first.evidence, 1);
    let after_first = fixture_counts(&pool).await;
    for (table, n) in &after_first {
        assert!(*n > 0, "{table} should hold the fixture's row");
    }

    let again = ingest::import::run(&pool, &dir)
        .await
        .expect("the fixture loads a second time");
    assert_eq!(first, again, "the same files report the same counts");
    assert_eq!(
        after_first,
        fixture_counts(&pool).await,
        "the second import changed rows"
    );

    // A newer export of the same rows carries new values under the same natural
    // keys, so the rows are replaced where they stand rather than duplicated.
    write_dataset(
        &dir,
        &[
            (
                "sources",
                r#"[{"url":"https://example.test/doc","content_hash":"h1",
                  "content_sha256":"0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
                  "snapshot_url":"https://archive.test/doc","kind":"manual","title":"Doc v2",
                  "outlet":null,"fetched_at":null,"published_at":null}]"#,
            ),
            (
                "parties",
                r#"[{"slug":"test-partisi","name":"Test Partisi","short_name":"TP",
                  "country":"tt","founded_date":"2020-01-01","dissolved_date":null,
                  "ideology_tags":["merkez"],"color":null,
                  "summary":"2021'de yeniden kuruldu.","wikidata_id":null,
                  "source_url":"https://example.test/doc","source_hash":"h1"}]"#,
            ),
        ],
    );
    let updated = ingest::import::run(&pool, &dir)
        .await
        .expect("the updated fixture loads");
    assert_eq!(first, updated, "the same number of rows was read");
    assert_eq!(
        after_first,
        fixture_counts(&pool).await,
        "an update changes values, not row counts"
    );

    let (title, sha256, snapshot): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "select title, content_sha256, snapshot_url from sources
             where url = 'https://example.test/doc'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title.as_deref(), Some("Doc v2"));
    assert!(sha256.is_some(), "the hash the export added is stored");
    assert_eq!(snapshot.as_deref(), Some("https://archive.test/doc"));

    let summary: Option<String> = sqlx::query_scalar("select summary from parties where slug = $1")
        .bind("test-partisi")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(summary.as_deref(), Some("2021'de yeniden kuruldu."));

    let tags: Vec<String> = sqlx::query_scalar("select ideology_tags from parties where slug = $1")
        .bind("test-partisi")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tags, vec!["merkez".to_string()]);
}
