//! Integration tests for the repository layer. Each test runs against a fresh
//! database created by `#[sqlx::test]`, with the workspace migrations applied.

use domain::models::NewPerson;

fn sample_person(source_id: i64) -> NewPerson {
    NewPerson {
        wikidata_id: Some("Q1".to_string()),
        full_name: "Ayşe Yılmaz".to_string(),
        slug: "ayse-yilmaz".to_string(),
        birth_date: None,
        birth_place: None,
        photo_url: None,
        photo_license: None,
        summary: None,
        source_id,
        country_id: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_service_validates_and_creates(pool: sqlx::PgPool) {
    use db::service::news::{create, CreateNews};

    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();

    // A blank headline is rejected.
    let err = create(
        &pool,
        CreateNews {
            person_slug: Some("ayse-yilmaz".into()),
            headline: "   ".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // Nothing linked is rejected.
    let err = create(
        &pool,
        CreateNews {
            headline: "H".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // An unknown entity is a not-found.
    let err = create(
        &pool,
        CreateNews {
            party_slug: Some("does-not-exist".into()),
            headline: "H".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::NotFound { .. }));

    // Happy path: the item is created and linked to the person.
    let id = create(
        &pool,
        CreateNews {
            person_slug: Some("ayse-yilmaz".into()),
            headline: "Real headline".into(),
            url: "https://x.test/real".into(),
            outlet: Some("Outlet".into()),
            published_on: chrono::NaiveDate::from_ymd_opt(2026, 1, 2),
            our_summary: Some("Summary".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(id > 0);

    let items = db::news::for_person(&pool, person_id).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].headline, "Real headline");
    assert_eq!(items[0].outlet.as_deref(), Some("Outlet"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_recent_carries_linked_entities(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Test Ulke', 'test-ulke', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();
    let party_id: i64 = sqlx::query_scalar(
        "insert into parties (name, short_name, slug, color, source_id, country_id) values ('Test Partisi', 'TP', 'test-partisi', '#0055aa', $1, $2) returning id",
    )
    .bind(source_id)
    .bind(country_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // No news yet: the index is empty and the URL is unknown.
    assert!(db::news::recent(&pool, country_id, "tr", 10)
        .await
        .unwrap()
        .is_empty());
    assert!(!db::news::url_exists(&pool, "https://news.test/a")
        .await
        .unwrap());

    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://news.test/a",
            outlet: Some("Outlet"),
            published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 2)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc()),
            headline: "Headline one",
            our_summary: Some("Summary."),
            person_ids: &[person_id],
            party_ids: &[party_id],
        },
    )
    .await
    .unwrap();

    let cards = db::news::recent(&pool, country_id, "tr", 10).await.unwrap();
    assert_eq!(cards.len(), 1);
    let card = &cards[0];
    assert_eq!(card.headline, "Headline one");
    assert_eq!(card.outlet.as_deref(), Some("Outlet"));
    // Linked entities keep their slug paired with the display fields.
    assert_eq!(card.people.len(), 1);
    assert_eq!(card.people[0].slug, "ayse-yilmaz");
    assert_eq!(card.people[0].name, "Ayşe Yılmaz");
    assert_eq!(card.parties.len(), 1);
    assert_eq!(card.parties[0].slug, "test-partisi");
    assert_eq!(card.parties[0].short, "TP");
    assert_eq!(card.parties[0].color.as_deref(), Some("#0055aa"));

    // The URL is now known: an idempotent ingest would skip it.
    assert!(db::news::url_exists(&pool, "https://news.test/a")
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_summary_draft_workflow(pool: sqlx::PgPool) {
    // news::create inserts its own source row, so no source is set up here.
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://news.test/a",
            outlet: Some("Outlet"),
            published_at: None,
            headline: "Headline",
            our_summary: None,
            person_ids: &[],
            party_ids: &[],
        },
    )
    .await
    .unwrap();

    // With no summary and no draft the item is a summarizer candidate.
    let todo = db::news::unsummarized(&pool, 10).await.unwrap();
    assert_eq!(todo.len(), 1);
    let id = todo[0].id;
    assert_eq!(todo[0].url, "https://news.test/a");

    // A draft takes it out of the candidate set and into the review queue.
    db::news::set_draft(&pool, id, "Taslak ozet.")
        .await
        .unwrap();
    assert!(db::news::unsummarized(&pool, 10).await.unwrap().is_empty());
    assert_eq!(db::news::pending_draft_count(&pool).await.unwrap(), 1);
    let drafts = db::news::pending_drafts(&pool).await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].summary_draft, "Taslak ozet.");

    // Publishing an edited draft sets our_summary and clears the draft.
    db::news::publish_summary(&pool, id, "Duzenlenmis ozet.")
        .await
        .unwrap();
    assert_eq!(db::news::pending_draft_count(&pool).await.unwrap(), 0);
    let (published, draft): (Option<String>, Option<String>) =
        sqlx::query_as("select our_summary, summary_draft from news_items where id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(published.as_deref(), Some("Duzenlenmis ozet."));
    assert_eq!(draft, None);
    // A published item is no longer a candidate.
    assert!(db::news::unsummarized(&pool, 10).await.unwrap().is_empty());

    // Discarding a draft leaves the item unpublished and a candidate again.
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://news.test/b",
            outlet: Some("Outlet"),
            published_at: None,
            headline: "Headline B",
            our_summary: None,
            person_ids: &[],
            party_ids: &[],
        },
    )
    .await
    .unwrap();
    let id_b = db::news::unsummarized(&pool, 10).await.unwrap()[0].id;
    db::news::set_draft(&pool, id_b, "Bir taslak.")
        .await
        .unwrap();
    db::news::discard_draft(&pool, id_b).await.unwrap();
    assert_eq!(db::news::pending_draft_count(&pool).await.unwrap(), 0);
    assert_eq!(db::news::unsummarized(&pool, 10).await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_and_parties_list_filter(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Testland', 'testland', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    // A second country whose people must not leak into the first's list.
    let other: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Other', 'other', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let person = |wd: &str, name: &str, slug: &str, cid: i64| NewPerson {
        wikidata_id: Some(wd.to_string()),
        full_name: name.to_string(),
        slug: slug.to_string(),
        birth_date: None,
        birth_place: None,
        photo_url: None,
        photo_license: None,
        summary: None,
        source_id,
        country_id: Some(cid),
    };
    db::people::upsert_person(
        &pool,
        &person("Q1", "Ayşe Yılmaz", "ayse-yilmaz", country_id),
    )
    .await
    .unwrap();
    db::people::upsert_person(
        &pool,
        &person("Q2", "Mehmet Demir", "mehmet-demir", country_id),
    )
    .await
    .unwrap();
    // Belongs to the other country; must be excluded from Testland's list.
    db::people::upsert_person(&pool, &person("Q3", "Ayse Foreign", "ayse-foreign", other))
        .await
        .unwrap();
    sqlx::query("insert into parties (name, short_name, slug, source_id, country_id) values ('Test Partisi', 'TP', 'test-partisi', $1, $2)")
        .bind(source_id)
        .bind(country_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into parties (name, short_name, slug, source_id, country_id) values ('Other Party', 'OP', 'other-party', $1, $2)")
        .bind(source_id)
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();

    // Lists are scoped to the country: only its two people, not the third.
    assert_eq!(
        db::people::count_filtered(&pool, country_id, "")
            .await
            .unwrap(),
        2
    );
    // Diacritic-insensitive and country-scoped: "ayse" matches "Ayşe" here, not
    // the other country's "Ayse Foreign".
    assert_eq!(
        db::people::count_filtered(&pool, country_id, "ayse")
            .await
            .unwrap(),
        1
    );
    let hits = db::people::list_filtered(&pool, country_id, "AYSE", 50, 0)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].full_name, "Ayşe Yılmaz");
    assert_eq!(
        db::people::count_filtered(&pool, country_id, "demir")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db::people::count_filtered(&pool, country_id, "zzz")
            .await
            .unwrap(),
        0
    );

    // Parties are country-scoped too.
    assert_eq!(
        db::parties::list_filtered(&pool, country_id, "")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db::parties::list_filtered(&pool, country_id, "test")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(db::parties::list_filtered(&pool, country_id, "other")
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_edit_link_unlink(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();
    let party_id: i64 = sqlx::query_scalar(
        "insert into parties (name, short_name, slug, source_id) values ('Test Partisi', 'TP', 'test-partisi', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://news.test/a",
            outlet: Some("Outlet"),
            published_at: None,
            headline: "H",
            our_summary: None,
            person_ids: &[],
            party_ids: &[],
        },
    )
    .await
    .unwrap();
    let id = db::news::unsummarized(&pool, 10).await.unwrap()[0].id;

    // Edit the fields.
    db::news::update_fields(&pool, id, "New H", Some("Sum"), Some("Ali Veli"))
        .await
        .unwrap();
    let e = db::news::get_edit(&pool, id).await.unwrap().unwrap();
    assert_eq!(e.headline, "New H");
    assert_eq!(e.our_summary.as_deref(), Some("Sum"));
    assert_eq!(e.author.as_deref(), Some("Ali Veli"));
    assert!(e.people.is_empty() && e.parties.is_empty());

    // Link by slug: a new link returns true, a repeat or unknown slug false.
    assert!(db::news::link_person(&pool, id, "ayse-yilmaz")
        .await
        .unwrap());
    assert!(!db::news::link_person(&pool, id, "ayse-yilmaz")
        .await
        .unwrap());
    assert!(!db::news::link_person(&pool, id, "nope").await.unwrap());
    assert!(db::news::link_party(&pool, id, "test-partisi")
        .await
        .unwrap());
    let e = db::news::get_edit(&pool, id).await.unwrap().unwrap();
    assert_eq!(e.people.len(), 1);
    assert_eq!(e.parties.len(), 1);

    // Unlink by id.
    db::news::unlink_person(&pool, id, person_id).await.unwrap();
    db::news::unlink_party(&pool, id, party_id).await.unwrap();
    let e = db::news::get_edit(&pool, id).await.unwrap().unwrap();
    assert!(e.people.is_empty() && e.parties.is_empty());

    // An unknown item has no edit view.
    assert!(db::news::get_edit(&pool, 999_999).await.unwrap().is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn outlets_upsert_link_and_paginate(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://x.test/c", None, Some("hc"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Ulke', 'ulke', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Upsert an outlet, then upsert the same slug again: it updates in place.
    let id = db::outlets::upsert(
        &pool,
        &db::outlets::NewOutlet {
            name: "Test Gazetesi",
            slug: "test-gazetesi",
            homepage_url: Some("https://tg.test"),
            logo_url: None,
            logo_license: None,
            leaning: Some("lean_left"),
            summary: Some("Bir test gazetesi."),
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    let id2 = db::outlets::upsert(
        &pool,
        &db::outlets::NewOutlet {
            name: "Test Gazetesi Yeni",
            slug: "test-gazetesi",
            homepage_url: None,
            logo_url: None,
            logo_license: None,
            leaning: Some("center"),
            summary: None,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    assert_eq!(id, id2);

    // Two articles carrying the outlet label, then link them to the outlet.
    for i in 1..=2 {
        db::news::create(
            &pool,
            &db::news::NewNews {
                url: &format!("https://tg.test/{i}"),
                outlet: Some("Test Gazetesi"),
                published_at: None,
                headline: &format!("Haber {i}"),
                our_summary: None,
                person_ids: &[],
                party_ids: &[],
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(
        db::outlets::link_sources_by_label(&pool, id, "Test Gazetesi")
            .await
            .unwrap(),
        2
    );

    // The outlet reflects the last upsert and counts its linked articles.
    let o = db::outlets::get_by_slug(&pool, "test-gazetesi")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(o.name, "Test Gazetesi Yeni");
    assert_eq!(o.leaning.as_deref(), Some("center"));
    assert_eq!(o.article_count, 2);

    // Pagination walks the articles one page at a time.
    assert_eq!(db::news::count_for_outlet(&pool, id).await.unwrap(), 2);
    let page1 = db::news::for_outlet(&pool, id, 1, 0).await.unwrap();
    let page2 = db::news::for_outlet(&pool, id, 1, 1).await.unwrap();
    assert_eq!(page1.len(), 1);
    assert_eq!(page2.len(), 1);
    assert_ne!(page1[0].url, page2[0].url);

    let list = db::outlets::list(&pool, country_id).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].slug, "test-gazetesi");
    assert_eq!(list[0].article_count, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn statement_service_validates_and_creates(pool: sqlx::PgPool) {
    use db::service::statements::{create, CreateStatement};

    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();

    // Blank text is rejected.
    let err = create(
        &pool,
        CreateStatement {
            person_slug: Some("ayse-yilmaz".into()),
            text: "   ".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // No attributed entity is rejected.
    let err = create(
        &pool,
        CreateStatement {
            text: "Something".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // An unknown entity is a not-found.
    let err = create(
        &pool,
        CreateStatement {
            person_slug: Some("nope".into()),
            text: "Something".into(),
            url: "https://x.test/1".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::NotFound { .. }));

    // Happy path: created and attributed to the person.
    let id = create(
        &pool,
        CreateStatement {
            person_slug: Some("ayse-yilmaz".into()),
            text: "Reform matters.".into(),
            is_paraphrase: true,
            stated_on: chrono::NaiveDate::from_ymd_opt(2026, 1, 2),
            url: "https://x.test/real".into(),
            outlet: Some("Outlet".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(id > 0);

    let items = db::statements::for_person(&pool, person_id).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text_original, "Reform matters.");
    assert!(items[0].is_paraphrase);
}

#[sqlx::test(migrations = "../../migrations")]
async fn poll_service_validates_and_creates(pool: sqlx::PgPool) {
    use db::service::polls::{create, CreatePoll};

    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();

    // A blank question is rejected.
    let err = create(
        &pool,
        CreatePoll {
            question: "  ".into(),
            options: vec!["a".into(), "b".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // Fewer than two non-blank options is rejected.
    let err = create(
        &pool,
        CreatePoll {
            question: "Q?".into(),
            options: vec!["only".into(), "  ".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // An unknown linked entity is a not-found.
    let err = create(
        &pool,
        CreatePoll {
            person_slug: Some("nope".into()),
            question: "Q?".into(),
            options: vec!["a".into(), "b".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::NotFound { .. }));

    // Happy path: a slug is derived from the question, twice unique.
    let s1 = create(
        &pool,
        CreatePoll {
            person_slug: Some("ayse-yilmaz".into()),
            question: "How is it going?".into(),
            options: vec!["Good".into(), "Bad".into(), "  ".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(s1, "how-is-it-going");

    let s2 = create(
        &pool,
        CreatePoll {
            question: "How is it going?".into(),
            options: vec!["a".into(), "b".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(s2, "how-is-it-going-2");

    let poll = db::polls::get_by_slug(&pool, &s1).await.unwrap().unwrap();
    assert_eq!(poll.options.len(), 2); // the blank option was dropped
    assert_eq!(poll.kind, "single"); // empty kind defaults to single

    // An unknown kind is rejected.
    let err = create(
        &pool,
        CreatePoll {
            question: "Rate it".into(),
            kind: "wildcard".into(),
            options: vec!["a".into(), "b".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // A recognised kind is stored.
    let s3 = create(
        &pool,
        CreatePoll {
            question: "How do you rate it?".into(),
            kind: "scale".into(),
            options: vec!["1".into(), "2".into(), "3".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let scale = db::polls::get_by_slug(&pool, &s3).await.unwrap().unwrap();
    assert_eq!(scale.kind, "scale");

    // An image without a license is rejected.
    let err = create(
        &pool,
        CreatePoll {
            question: "Which logo?".into(),
            options: vec!["A".into(), "B".into()],
            option_media: vec!["https://img.test/a.png".into(), String::new()],
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, db::service::Error::Validation(_)));

    // With a license, an image-only option (blank label) is kept, and the media
    // is stored on the option and the question.
    let s4 = create(
        &pool,
        CreatePoll {
            question: "Which logo?".into(),
            media_url: Some("https://img.test/q.png".into()),
            media_license: Some("CC0".into()),
            options: vec![String::new(), String::new()],
            option_media: vec![
                "https://img.test/a.png".into(),
                "https://img.test/b.png".into(),
            ],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let media_poll = db::polls::get_by_slug(&pool, &s4).await.unwrap().unwrap();
    assert_eq!(
        media_poll.media_url.as_deref(),
        Some("https://img.test/q.png")
    );
    assert_eq!(media_poll.options.len(), 2);
    assert_eq!(
        media_poll.options[0].media_url.as_deref(),
        Some("https://img.test/a.png")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn upsert_person_is_idempotent(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    let person = sample_person(source_id);

    let id1 = db::people::upsert_person(&pool, &person).await.unwrap();
    let id2 = db::people::upsert_person(&pool, &person).await.unwrap();

    assert_eq!(id1, id2, "upsert on wikidata_id must not duplicate");
    let total: i64 = sqlx::query_scalar("select count(*) from people")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);

    let got = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .expect("person should exist");
    assert_eq!(got.full_name, "Ayşe Yılmaz");
}

#[sqlx::test(migrations = "../../migrations")]
async fn search_finds_person(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h1"))
            .await
            .unwrap();
    db::people::upsert_person(&pool, &sample_person(source_id))
        .await
        .unwrap();

    let hits = db::search::search(&pool, "Yılmaz", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slug, "ayse-yilmaz");

    // A prefix of a name matches, not only the whole word.
    let hits = db::search::search(&pool, "ay", 10).await.unwrap();
    assert!(hits.iter().any(|h| h.slug == "ayse-yilmaz"));

    // A blank or operator-only query returns nothing (and never errors).
    assert!(db::search::search(&pool, "  &  ", 10)
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn one_vote_per_user(pool: sqlx::PgPool) {
    let user_id = sqlx::query_scalar!(
        r#"insert into users (email_hash, password_hash, verified_at) values ('vh', 'ph', now()) returning id"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let poll_id: i64 =
        sqlx::query_scalar("insert into polls (question, slug) values ('Q', 'p') returning id")
            .fetch_one(&pool)
            .await
            .unwrap();
    let option_id: i64 = sqlx::query_scalar(
        "insert into poll_options (poll_id, label, position) values ($1, 'A', 1) returning id",
    )
    .bind(poll_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(!db::polls::has_voted(&pool, poll_id, user_id).await.unwrap());
    assert!(db::polls::cast_vote(&pool, poll_id, option_id, user_id)
        .await
        .unwrap());
    // A second vote by the same user is ignored, never overwritten.
    assert!(!db::polls::cast_vote(&pool, poll_id, option_id, user_id)
        .await
        .unwrap());
    assert!(db::polls::has_voted(&pool, poll_id, user_id).await.unwrap());

    let poll = db::polls::get_by_slug(&pool, "p")
        .await
        .unwrap()
        .expect("poll should exist");
    assert_eq!(poll.options.len(), 1);
    assert_eq!(poll.options[0].votes, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn verification_marks_user_verified_once(pool: sqlx::PgPool) {
    let user_id = db::users::insert(&pool, "eh", "ph").await.unwrap();

    // New accounts start unverified.
    let user = db::users::get_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(user.verified_at.is_none());

    let expires = chrono::Utc::now() + chrono::Duration::hours(24);
    db::email_verifications::create(&pool, user_id, "eh", "codehash", expires)
        .await
        .unwrap();

    // A valid token verifies the user and returns its id.
    let verified = db::email_verifications::consume_and_verify(&pool, "codehash")
        .await
        .unwrap();
    assert_eq!(verified, Some(user_id));

    let user = db::users::get_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(user.verified_at.is_some());

    // The same token cannot be reused.
    let again = db::email_verifications::consume_and_verify(&pool, "codehash")
        .await
        .unwrap();
    assert_eq!(again, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_verification_is_rejected(pool: sqlx::PgPool) {
    let user_id = db::users::insert(&pool, "eh", "ph").await.unwrap();
    let expired = chrono::Utc::now() - chrono::Duration::hours(1);
    db::email_verifications::create(&pool, user_id, "eh", "codehash", expired)
        .await
        .unwrap();

    let result = db::email_verifications::consume_and_verify(&pool, "codehash")
        .await
        .unwrap();
    assert_eq!(result, None);

    let user = db::users::get_by_id(&pool, user_id).await.unwrap().unwrap();
    assert!(
        user.verified_at.is_none(),
        "an expired token must not verify"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_email_is_rejected(pool: sqlx::PgPool) {
    db::users::insert(&pool, "same", "ph").await.unwrap();
    let second = db::users::insert(&pool, "same", "ph2").await;
    assert!(matches!(second, Err(db::Error::UniqueViolation)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn poll_summaries_and_vote_guard(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h"))
            .await
            .unwrap();
    let party_id: i64 = sqlx::query_scalar(
        "insert into parties (name, slug, source_id) values ('P', 'p', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, party_id) values ('Q', 'poll-q', $1) returning id",
    )
    .bind(party_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let option_id: i64 = sqlx::query_scalar(
        "insert into poll_options (poll_id, label, position) values ($1, 'A', 1) returning id",
    )
    .bind(poll_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // The poll is listed for its party (with its options), not for an unrelated
    // person.
    let polls = db::polls::full_for_party(&pool, party_id).await.unwrap();
    assert_eq!(polls.len(), 1);
    assert_eq!(polls[0].slug, "poll-q");
    assert_eq!(polls[0].options.len(), 1);
    assert!(db::polls::full_for_person(&pool, party_id)
        .await
        .unwrap()
        .is_empty());

    let user_id = db::users::insert(&pool, "vh", "ph").await.unwrap();

    // A vote for an option that does not belong to the poll is rejected.
    assert!(!db::polls::cast_vote(&pool, poll_id, 999_999, user_id)
        .await
        .unwrap());
    assert!(!db::polls::has_voted(&pool, poll_id, user_id).await.unwrap());

    // A valid vote is recorded once; a second attempt is ignored.
    assert!(db::polls::cast_vote(&pool, poll_id, option_id, user_id)
        .await
        .unwrap());
    assert!(!db::polls::cast_vote(&pool, poll_id, option_id, user_id)
        .await
        .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn single_choice_rejects_a_second_option(pool: sqlx::PgPool) {
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, kind) values ('Q', 'q', 'single') returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let a: i64 = sqlx::query_scalar(
        "insert into poll_options (poll_id, label, position) values ($1, 'A', 1) returning id",
    )
    .bind(poll_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let b: i64 = sqlx::query_scalar(
        "insert into poll_options (poll_id, label, position) values ($1, 'B', 2) returning id",
    )
    .bind(poll_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let user = db::users::insert(&pool, "vh", "ph").await.unwrap();

    // First choice is recorded; a second, different choice is refused (one vote
    // per user in a single-choice poll).
    assert!(db::polls::cast_vote(&pool, poll_id, a, user).await.unwrap());
    assert!(!db::polls::cast_vote(&pool, poll_id, b, user).await.unwrap());
    let n: i64 = sqlx::query_scalar("select count(*) from poll_votes where poll_id = $1")
        .bind(poll_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn multi_select_chains_and_shares_voter_index(pool: sqlx::PgPool) {
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, kind) values ('M', 'm', 'multi') returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut opt = Vec::new();
    for (label, pos) in [("A", 1), ("B", 2), ("C", 3)] {
        let id: i64 = sqlx::query_scalar(
            "insert into poll_options (poll_id, label, position) values ($1, $2, $3) returning id",
        )
        .bind(poll_id)
        .bind(label)
        .bind(pos)
        .fetch_one(&pool)
        .await
        .unwrap();
        opt.push(id);
    }
    let u1 = db::users::insert(&pool, "u1", "p").await.unwrap();
    let u2 = db::users::insert(&pool, "u2", "p").await.unwrap();

    // u1 picks A and B.
    assert_eq!(
        db::polls::cast_votes(&pool, poll_id, &[opt[0], opt[1]], u1)
            .await
            .unwrap(),
        2
    );
    // u1 re-submits A (a repeat) and adds C: only C is new.
    assert_eq!(
        db::polls::cast_votes(&pool, poll_id, &[opt[0], opt[2]], u1)
            .await
            .unwrap(),
        1
    );
    // u2 picks B.
    assert_eq!(
        db::polls::cast_votes(&pool, poll_id, &[opt[1]], u2)
            .await
            .unwrap(),
        1
    );

    // u1's three option rows share one voter index; u2 has a distinct one.
    let u1_indexes: i64 = sqlx::query_scalar(
        "select count(distinct voter_index) from poll_votes where poll_id = $1 and user_id = $2",
    )
    .bind(poll_id)
    .bind(u1)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(u1_indexes, 1, "a voter keeps one index across their picks");
    let distinct_voters: i64 =
        sqlx::query_scalar("select count(distinct voter_index) from poll_votes where poll_id = $1")
            .bind(poll_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(distinct_voters, 2);

    // Four vote rows, chained: head at seq 4, and every row hashes correctly
    // from its predecessor (genesis for seq 1), so the chain is intact.
    let head = db::polls::chain_head(&pool, poll_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.head_seq, 4);
    let chain_ok: bool = sqlx::query_scalar(
        r#"
        select bool_and(
            v.row_hash = vote_chain_hash(
                coalesce(prev.row_hash, vote_chain_genesis(v.poll_id)),
                v.poll_id, v.seq, v.option_id, v.voter_index, v.cast_at
            )
        )
        from poll_votes v
        left join poll_votes prev on prev.poll_id = v.poll_id and prev.seq = v.seq - 1
        where v.poll_id = $1
        "#,
    )
    .bind(poll_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(chain_ok, "every vote row must chain from its predecessor");
}

#[sqlx::test(migrations = "../../migrations")]
async fn country_poll_is_scoped_to_country_only(pool: sqlx::PgPool) {
    use db::service::polls::{create, CreatePoll};

    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/a", None, Some("h"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Testland', 'testland', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let party_id: i64 = sqlx::query_scalar(
        "insert into parties (name, slug, source_id) values ('P', 'p', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // A country-level poll (no person/party) attaches to the country.
    create(
        &pool,
        CreatePoll {
            country_slug: Some("testland".into()),
            question: "How is the country doing?".into(),
            options: vec!["Good".into(), "Bad".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // A party poll also carries the country slug, but the party attachment wins:
    // it must not leak onto the country page.
    create(
        &pool,
        CreatePoll {
            country_slug: Some("testland".into()),
            party_slug: Some("p".into()),
            question: "How is the party doing?".into(),
            options: vec!["Good".into(), "Bad".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let country_polls = db::polls::full_for_country(&pool, country_id)
        .await
        .unwrap();
    assert_eq!(country_polls.len(), 1);
    assert_eq!(country_polls[0].slug, "how-is-the-country-doing");

    let party_polls = db::polls::full_for_party(&pool, party_id).await.unwrap();
    assert_eq!(party_polls.len(), 1);
    assert_eq!(party_polls[0].slug, "how-is-the-party-doing");
}

#[sqlx::test(migrations = "../../migrations")]
async fn elections_results_and_party_history(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/e", None, Some("h"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Testland', 'testland', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let party_a: i64 = sqlx::query_scalar(
        "insert into parties (name, short_name, slug, source_id) values ('Alpha', 'ALP', 'alpha', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let party_b: i64 = sqlx::query_scalar(
        "insert into parties (name, short_name, slug, source_id) values ('Beta', 'BET', 'beta', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let election_id = db::elections::create(
        &pool,
        &db::elections::NewElection {
            country_id,
            name: "2023 General Election",
            slug: "2023-general",
            held_on: chrono::NaiveDate::from_ymd_opt(2023, 5, 14),
            kind: Some("parliamentary"),
            source_id,
        },
    )
    .await
    .unwrap();

    db::elections::add_result(
        &pool,
        election_id,
        party_a,
        Some(120),
        Some(4_000_000),
        source_id,
    )
    .await
    .unwrap();
    db::elections::add_result(&pool, election_id, party_b, Some(80), None, source_id)
        .await
        .unwrap();

    // The election is listed for its country and found by slug.
    let listed = db::elections::list_for_country(&pool, country_id, "tr")
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].slug, "2023-general");
    assert!(db::elections::get_by_slug(&pool, "2023-general")
        .await
        .unwrap()
        .is_some());

    // Results come back most seats first.
    let rows = db::elections::results(&pool, election_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].party_slug.as_deref(), Some("alpha"));
    assert_eq!(rows[0].seats, Some(120));
    assert_eq!(rows[1].party_slug.as_deref(), Some("beta"));

    // A party's history carries its own result.
    let history = db::elections::history_for_party(&pool, party_a)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].seats, Some(120));
    assert_eq!(history[0].votes, Some(4_000_000));

    // add_result upserts on (election, party): a re-run corrects the figure
    // rather than duplicating the row.
    db::elections::add_result(
        &pool,
        election_id,
        party_a,
        Some(125),
        Some(4_100_000),
        source_id,
    )
    .await
    .unwrap();
    let rows = db::elections::results(&pool, election_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].seats, Some(125));

    // A label result (a referendum option / candidate) sits alongside party
    // results and comes back with no party.
    db::elections::add_label_result(&pool, election_id, "Evet", Some(200), source_id)
        .await
        .unwrap();
    let labelled = db::elections::results(&pool, election_id).await.unwrap();
    assert_eq!(labelled.len(), 3);
    let evet = labelled
        .iter()
        .find(|r| r.label.as_deref() == Some("Evet"))
        .unwrap();
    assert!(evet.party_slug.is_none());
    assert_eq!(evet.votes, Some(200));
}

#[sqlx::test(migrations = "../../migrations")]
async fn chamber_size_reads_latest_parliamentary_election(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/c", None, Some("h"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Testland', 'testland', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let party: i64 = sqlx::query_scalar(
        "insert into parties (name, short_name, slug, source_id) values ('Alpha', 'ALP', 'alpha', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // No parliamentary election yet: the chamber size is unknown.
    assert_eq!(
        db::country::chamber_size(&pool, country_id).await.unwrap(),
        None
    );

    // Helper: create an election for this country and return its id.
    async fn make(
        pool: &sqlx::PgPool,
        country_id: i64,
        source_id: i64,
        name: &str,
        slug: &str,
        year: i32,
        kind: &str,
    ) -> i64 {
        db::elections::create(
            pool,
            &db::elections::NewElection {
                country_id,
                name,
                slug,
                held_on: chrono::NaiveDate::from_ymd_opt(year, 5, 14),
                kind: Some(kind),
                source_id,
            },
        )
        .await
        .unwrap()
    }

    // An older parliamentary election (100 seats) and a later local election
    // (300 seats) must both be ignored: only the most recent parliamentary one
    // sets the chamber size.
    let old = make(
        &pool,
        country_id,
        source_id,
        "2018 General",
        "2018-general",
        2018,
        "parliamentary",
    )
    .await;
    db::elections::add_result(&pool, old, party, Some(100), None, source_id)
        .await
        .unwrap();
    let newer = make(
        &pool,
        country_id,
        source_id,
        "2023 General",
        "2023-general",
        2023,
        "parliamentary",
    )
    .await;
    db::elections::add_result(&pool, newer, party, Some(200), None, source_id)
        .await
        .unwrap();
    let local = make(
        &pool,
        country_id,
        source_id,
        "2024 Local",
        "2024-local",
        2024,
        "local",
    )
    .await;
    db::elections::add_result(&pool, local, party, Some(300), None, source_id)
        .await
        .unwrap();

    assert_eq!(
        db::country::chamber_size(&pool, country_id).await.unwrap(),
        Some(200)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn conflicts_dedupe_list_and_resolve(pool: sqlx::PgPool) {
    let existing_src = db::sources::insert_source(
        &pool,
        "official_gov",
        "https://example.test/x",
        None,
        Some("t"),
    )
    .await
    .unwrap();
    let incoming_src = db::sources::insert_source(
        &pool,
        "wikidata",
        "https://wikidata.test/x",
        None,
        Some("w"),
    )
    .await
    .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(existing_src))
        .await
        .unwrap();

    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 0);

    let mk = |incoming: &'static str| db::conflicts::NewConflict {
        entity_type: "person",
        entity_id: Some(person_id),
        field: "party",
        existing_value: Some("Alpha"),
        incoming_value: Some(incoming),
        existing_source_id: Some(existing_src),
        incoming_source_id: Some(incoming_src),
    };

    let first = db::conflicts::record(&pool, &mk("Beta")).await.unwrap();
    // The same conflict recorded again returns the same row, not a duplicate.
    let again = db::conflicts::record(&pool, &mk("Beta")).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 1);

    // A different incoming value is a distinct conflict.
    db::conflicts::record(&pool, &mk("Gamma")).await.unwrap();
    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 2);

    let open = db::conflicts::list_open(&pool).await.unwrap();
    assert_eq!(open.len(), 2);
    let one = open.iter().find(|c| c.id == first).unwrap();
    assert_eq!(one.entity_label.as_deref(), Some("Ayşe Yılmaz")); // labelled from people
    assert_eq!(one.field, "party");
    assert_eq!(one.existing_value.as_deref(), Some("Alpha"));
    assert_eq!(one.incoming_value.as_deref(), Some("Beta"));
    assert_eq!(
        one.existing_source_url.as_deref(),
        Some("https://example.test/x")
    );
    assert_eq!(
        one.incoming_source_url.as_deref(),
        Some("https://wikidata.test/x")
    );

    // Resolving removes it from the open queue; a second resolve is a no-op.
    assert!(db::conflicts::resolve(&pool, first).await.unwrap());
    assert!(!db::conflicts::resolve(&pool, first).await.unwrap());
    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 1);

    // Now that the first is resolved, an identical conflict records fresh again.
    let rerecorded = db::conflicts::record(&pool, &mk("Beta")).await.unwrap();
    assert_ne!(rerecorded, first);
    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn events_are_scoped_and_ordered(pool: sqlx::PgPool) {
    let source_id =
        db::sources::insert_source(&pool, "manual", "https://example.test/ev", None, Some("h"))
            .await
            .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Testland', 'testland', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let party_id: i64 = sqlx::query_scalar(
        "insert into parties (name, slug, source_id) values ('Alpha', 'alpha', $1) returning id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Two country events (out of chronological order on insert) and one party
    // event.
    db::events::create(
        &pool,
        &db::events::NewEvent {
            country_id: Some(country_id),
            party_id: None,
            person_id: None,
            kind: "founding",
            title: "Republic proclaimed",
            happened_on: chrono::NaiveDate::from_ymd_opt(1923, 10, 29),
            source_id,
        },
    )
    .await
    .unwrap();
    db::events::create(
        &pool,
        &db::events::NewEvent {
            country_id: Some(country_id),
            party_id: None,
            person_id: None,
            kind: "election",
            title: "General election",
            happened_on: chrono::NaiveDate::from_ymd_opt(2023, 5, 14),
            source_id,
        },
    )
    .await
    .unwrap();
    db::events::create(
        &pool,
        &db::events::NewEvent {
            country_id: None,
            party_id: Some(party_id),
            person_id: None,
            kind: "founding",
            title: "Party founded",
            happened_on: chrono::NaiveDate::from_ymd_opt(2001, 8, 14),
            source_id,
        },
    )
    .await
    .unwrap();

    // Country events come back most recent first and carry their source url.
    let country_events = db::events::for_country(&pool, country_id).await.unwrap();
    assert_eq!(country_events.len(), 2);
    assert_eq!(country_events[0].title, "General election");
    assert_eq!(country_events[1].title, "Republic proclaimed");
    assert_eq!(country_events[0].source_url, "https://example.test/ev");

    // The party event is scoped to the party, not to the country.
    let party_events = db::events::for_party(&pool, party_id).await.unwrap();
    assert_eq!(party_events.len(), 1);
    assert_eq!(party_events[0].title, "Party founded");
    assert!(db::events::for_person(&pool, party_id)
        .await
        .unwrap()
        .is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn translations_draft_review_and_publish(pool: sqlx::PgPool) {
    // A reviewer, needed for the publish step (reviewed_by references users).
    let reviewer: i64 = sqlx::query_scalar(
        "insert into users (email_hash, password_hash, verified_at) values ('rh', 'ph', now()) returning id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // A machine draft lands for a person's summary in Turkish.
    let id = db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "person",
            entity_id: 42,
            field: "summary",
            lang: "tr",
            text: "Makine çevirisi taslağı.",
            origin: "machine",
            status: "draft",
            source_lang: Some("en"),
        },
    )
    .await
    .unwrap();

    // A draft is not visible to readers, but is in the review queue.
    assert!(
        db::translations::published_for_entity(&pool, "person", 42, "tr")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(db::translations::pending_count(&pool).await.unwrap(), 1);
    assert_eq!(db::translations::pending(&pool, 10).await.unwrap().len(), 1);

    // An admin edits then publishes it.
    db::translations::set_text(&pool, id, "Düzeltilmiş çeviri.")
        .await
        .unwrap();
    db::translations::publish(&pool, id, reviewer)
        .await
        .unwrap();

    // Now readers see it, and it has left the queue.
    let published = db::translations::published_for_entity(&pool, "person", 42, "tr")
        .await
        .unwrap();
    assert_eq!(
        published.get("summary").map(String::as_str),
        Some("Düzeltilmiş çeviri.")
    );
    assert_eq!(db::translations::pending_count(&pool).await.unwrap(), 0);

    // The batch loader finds it too, keyed by (id, field).
    let batch = db::translations::published_for_entities(&pool, "person", &[42, 99], "tr")
        .await
        .unwrap();
    assert_eq!(
        batch.get(&(42, "summary".to_string())).map(String::as_str),
        Some("Düzeltilmiş çeviri.")
    );

    // A different language still falls back (no row).
    assert!(
        db::translations::published_for_entity(&pool, "person", 42, "en")
            .await
            .unwrap()
            .is_empty()
    );

    // The registry gates which fields are translatable.
    assert!(db::translations::is_registered("person", "summary"));
    assert!(!db::translations::is_registered("person", "birth_place"));

    // Discarding removes it entirely.
    let id2 = db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "party",
            entity_id: 7,
            field: "summary",
            lang: "tr",
            text: "Taslak.",
            origin: "machine",
            status: "draft",
            source_lang: Some("en"),
        },
    )
    .await
    .unwrap();
    db::translations::discard(&pool, id2).await.unwrap();
    assert!(db::translations::get(&pool, id2).await.unwrap().is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn chambers_groups_a_bicameral_legislature(pool: sqlx::PgPool) {
    let src = db::sources::insert_source(&pool, "manual", "https://x.test/ch", None, Some("hch"))
        .await
        .unwrap();
    let country_id: i64 = sqlx::query_scalar(
        "insert into countries (name, slug, source_id) values ('Ulke', 'ulke2', $1) returning id",
    )
    .bind(src)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mk_party = "insert into parties (name, short_name, slug, source_id, country_id) \
                    values ($1, $1, $1, $2, $3) returning id";
    let a: i64 = sqlx::query_scalar(mk_party)
        .bind("A")
        .bind(src)
        .bind(country_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let b: i64 = sqlx::query_scalar(mk_party)
        .bind("B")
        .bind(src)
        .bind(country_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Four legislators: two senators (A, B), one representative (A), one
    // independent representative (no party).
    let mk = |slug: &str| NewPerson {
        wikidata_id: None,
        full_name: slug.to_string(),
        slug: slug.to_string(),
        birth_date: None,
        birth_place: None,
        photo_url: None,
        photo_license: None,
        summary: None,
        source_id: src,
        country_id: Some(country_id),
    };
    let members = [
        ("s1", "senator", "United States Senate", Some(a)),
        ("s2", "senator", "United States Senate", Some(b)),
        (
            "r1",
            "representative",
            "United States House of Representatives",
            Some(a),
        ),
        (
            "r2",
            "representative",
            "United States House of Representatives",
            None,
        ),
    ];
    for (slug, role, org, party_id) in members {
        let pid = db::people::upsert_person(&pool, &mk(slug)).await.unwrap();
        db::people::upsert_role(
            &pool,
            pid,
            role,
            Some(role),
            Some(org),
            None,
            None,
            None,
            src,
        )
        .await
        .unwrap();
        if let Some(party_id) = party_id {
            db::people::upsert_membership(&pool, pid, party_id, None, None, src)
                .await
                .unwrap();
        }
    }

    let chambers = db::country::chambers(&pool, country_id).await.unwrap();
    assert_eq!(chambers.len(), 2);
    // Upper chamber (senate) first.
    assert_eq!(chambers[0].chamber, "United States Senate");
    assert_eq!(chambers[0].total, 2);
    assert_eq!(chambers[0].independents, 0);
    assert_eq!(chambers[0].parties.len(), 2);
    assert_eq!(
        chambers[1].chamber,
        "United States House of Representatives"
    );
    assert_eq!(chambers[1].total, 2);
    assert_eq!(chambers[1].independents, 1);
    assert_eq!(chambers[1].parties.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn person_enrichment_upsert_read_and_idempotency(pool: sqlx::PgPool) {
    let src = db::sources::insert_source(&pool, "manual", "https://x.test/e", None, Some("he"))
        .await
        .unwrap();
    let person_id = db::people::upsert_person(&pool, &sample_person(src))
        .await
        .unwrap();
    let start = chrono::NaiveDate::from_ymd_opt(2000, 9, 1);
    let end = chrono::NaiveDate::from_ymd_opt(2004, 6, 1);

    db::people::upsert_education(
        &pool,
        person_id,
        "Test University",
        Some("Q1"),
        Some("Bachelor of Laws"),
        Some("Law"),
        start,
        end,
        src,
    )
    .await
    .unwrap();
    db::people::upsert_attribute(&pool, person_id, "occupation", "Lawyer", None, src)
        .await
        .unwrap();
    db::people::upsert_attribute(&pool, person_id, "ideology", "Social democracy", None, src)
        .await
        .unwrap();

    let edu = db::people::education(&pool, person_id).await.unwrap();
    assert_eq!(edu.len(), 1);
    assert_eq!(edu[0].institution, "Test University");
    assert_eq!(edu[0].degree.as_deref(), Some("Bachelor of Laws"));
    assert_eq!(edu[0].source_url, "https://x.test/e");

    let attrs = db::people::attributes(&pool, person_id).await.unwrap();
    assert_eq!(attrs.len(), 2);
    // Ordered by kind (occupation before ideology).
    assert_eq!(attrs[0].kind, "occupation");
    assert_eq!(attrs[1].kind, "ideology");

    // Re-upserting the same keys updates in place, never duplicates.
    db::people::upsert_education(
        &pool,
        person_id,
        "Test University",
        Some("Q1"),
        Some("Bachelor of Laws"),
        Some("Law"),
        start,
        end,
        src,
    )
    .await
    .unwrap();
    db::people::upsert_attribute(&pool, person_id, "occupation", "Lawyer", None, src)
        .await
        .unwrap();
    assert_eq!(
        db::people::education(&pool, person_id).await.unwrap().len(),
        1
    );
    assert_eq!(
        db::people::attributes(&pool, person_id)
            .await
            .unwrap()
            .len(),
        2
    );

    // Deleting removes an entry.
    let edu_id = edu[0].id;
    db::people::delete_education(&pool, edu_id).await.unwrap();
    assert!(db::people::education(&pool, person_id)
        .await
        .unwrap()
        .is_empty());
}
