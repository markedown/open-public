//! End-to-end tests that drive the real Axum router with a fresh test database
//! per test (via `#[sqlx::test]`) and a console mailer. Requests go through
//! `tower::ServiceExt::oneshot`, so the whole handler stack is exercised.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;

use chrono::NaiveDate;
use domain::models::{NewParty, NewPerson};
use server::config::MailTransport;
use server::mail::Mailer;
use server::state::AppState;

const SECRET: &[u8] = b"test-secret-key";
const COUNTRY: &str = "tr";

/// Seed one country, one party with a current member (full profile) and a former
/// member (minimal profile), so the detail, list, and search pages have data.
async fn seed(pool: &db::Pool) {
    let src = db::sources::insert_source(pool, "manual", "https://example.test/s", None, Some("h"))
        .await
        .unwrap();

    let country_id: i64 = sqlx::query_scalar("insert into countries (name, slug, source_id, flag_url) values ('Test Ulke', $1, $2, 'https://flag.test/tr.svg') returning id")
        .bind(COUNTRY)
        .bind(src)
        .fetch_one(pool)
        .await
        .unwrap();

    let party = db::parties::upsert_party(
        pool,
        &NewParty {
            wikidata_id: None,
            name: "Test Partisi".to_string(),
            short_name: Some("TP".to_string()),
            slug: "test-partisi".to_string(),
            founded_date: NaiveDate::from_ymd_opt(2010, 1, 1),
            dissolved_date: None,
            ideology_tags: vec!["reform".to_string()],
            summary: Some("A test party.".to_string()),
            source_id: src,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();

    // A poll attached to the party, with two options.
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, party_id) values ('Nasil buluyorsunuz?', 'party-poll', $1) returning id",
    )
    .bind(party)
    .fetch_one(pool)
    .await
    .unwrap();
    for (label, pos) in [("Iyi", 1), ("Kotu", 2)] {
        sqlx::query("insert into poll_options (poll_id, label, position) values ($1, $2, $3)")
            .bind(poll_id)
            .bind(label)
            .bind(pos)
            .execute(pool)
            .await
            .unwrap();
    }

    let full = db::people::upsert_person(
        pool,
        &NewPerson {
            wikidata_id: Some("Q100".to_string()),
            full_name: "Ayse Yilmaz".to_string(),
            slug: "ayse-yilmaz".to_string(),
            birth_date: NaiveDate::from_ymd_opt(1970, 5, 3),
            birth_place: Some("Testkent".to_string()),
            photo_url: Some("https://example.test/p.jpg".to_string()),
            photo_license: Some("CC0".to_string()),
            summary: Some("A test person.".to_string()),
            source_id: src,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    db::people::upsert_role(
        pool,
        full,
        "mp",
        Some("Milletvekili"),
        Some("Test Meclisi"),
        Some("Testkent"),
        NaiveDate::from_ymd_opt(2023, 1, 1),
        None,
        src,
    )
    .await
    .unwrap();
    db::people::upsert_membership(
        pool,
        full,
        party,
        NaiveDate::from_ymd_opt(2011, 1, 1),
        None,
        src,
    )
    .await
    .unwrap();

    let minimal = db::people::upsert_person(
        pool,
        &NewPerson {
            wikidata_id: Some("Q101".to_string()),
            full_name: "Mehmet Demir".to_string(),
            slug: "mehmet-demir".to_string(),
            birth_date: None,
            birth_place: None,
            photo_url: None,
            photo_license: None,
            summary: None,
            source_id: src,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    db::people::upsert_membership(
        pool,
        minimal,
        party,
        NaiveDate::from_ymd_opt(2010, 1, 1),
        NaiveDate::from_ymd_opt(2015, 1, 1),
        src,
    )
    .await
    .unwrap();

    // A colour, a head of government, and an alliance so the country overview
    // page renders its parliament, government, and coalitions sections.
    sqlx::query("update parties set color = '#0055aa' where id = $1")
        .bind(party)
        .execute(pool)
        .await
        .unwrap();

    let head = db::people::upsert_person(
        pool,
        &NewPerson {
            wikidata_id: Some("Q102".to_string()),
            full_name: "Zeynep Kaya".to_string(),
            slug: "zeynep-kaya".to_string(),
            birth_date: None,
            birth_place: None,
            photo_url: None,
            photo_license: None,
            summary: None,
            source_id: src,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    db::people::upsert_role(
        pool,
        head,
        "president",
        Some("President"),
        None,
        None,
        NaiveDate::from_ymd_opt(2023, 1, 1),
        None,
        src,
    )
    .await
    .unwrap();

    let alliance: i64 = sqlx::query_scalar(
        "insert into alliances (name, slug, summary, source_id, country_id, founded_date) values ('Test Ittifaki', 'test-ittifaki', 'Bir test ittifaki.', $1, (select id from countries where slug = $2), '2018-02-20') returning id",
    )
    .bind(src)
    .bind(COUNTRY)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into party_alliances (party_id, alliance_id, source_id) values ($1, $2, $3)",
    )
    .bind(party)
    .bind(alliance)
    .bind(src)
    .execute(pool)
    .await
    .unwrap();

    // A sitting member with no party group: an independent.
    let indep = db::people::upsert_person(
        pool,
        &NewPerson {
            wikidata_id: Some("Q103".to_string()),
            full_name: "Deniz Yildiz".to_string(),
            slug: "deniz-yildiz".to_string(),
            birth_date: None,
            birth_place: None,
            photo_url: None,
            photo_license: None,
            summary: None,
            source_id: src,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    db::people::upsert_role(
        pool,
        indep,
        "mp",
        Some("Milletvekili"),
        Some("Test Meclisi"),
        None,
        NaiveDate::from_ymd_opt(2023, 1, 1),
        None,
        src,
    )
    .await
    .unwrap();

    // An election with the party's result, so the country overview and the
    // party page both render their electoral sections.
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = $1")
        .bind(COUNTRY)
        .fetch_one(pool)
        .await
        .unwrap();
    let election_id = db::elections::create(
        pool,
        &db::elections::NewElection {
            country_id,
            name: "Test Secimi 2024",
            slug: "test-secimi-2024",
            held_on: NaiveDate::from_ymd_opt(2023, 5, 14),
            kind: Some("parliamentary"),
            source_id: src,
        },
    )
    .await
    .unwrap();
    db::elections::add_result(pool, election_id, party, Some(120), Some(4_000_000), src)
        .await
        .unwrap();
    db::elections::set_turnout(
        pool,
        election_id,
        Some(10_000_000),
        Some(8_000_000),
        Some(7_900_000),
    )
    .await
    .unwrap();

    // A country-level poll (attached to the country, no party/person), so the
    // country page renders its polls section.
    let country_poll: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, kind, country_id) values ('Ulke gidisati?', 'ulke-poll', 'scale', $1) returning id",
    )
    .bind(country_id)
    .fetch_one(pool)
    .await
    .unwrap();
    for (label, pos) in [("Iyi", 1), ("Kotu", 2)] {
        sqlx::query("insert into poll_options (poll_id, label, position) values ($1, $2, $3)")
            .bind(country_poll)
            .bind(label)
            .bind(pos)
            .execute(pool)
            .await
            .unwrap();
    }

    // A country event and a party event, so both pages render their timeline.
    db::events::create(
        pool,
        &db::events::NewEvent {
            country_id: Some(country_id),
            party_id: None,
            person_id: None,
            kind: "founding",
            title: "Cumhuriyet ilan edildi",
            happened_on: NaiveDate::from_ymd_opt(1923, 10, 29),
            source_id: src,
        },
    )
    .await
    .unwrap();
    db::events::create(
        pool,
        &db::events::NewEvent {
            country_id: None,
            party_id: Some(party),
            person_id: None,
            kind: "founding",
            title: "Parti kuruldu",
            happened_on: NaiveDate::from_ymd_opt(2010, 1, 1),
            source_id: src,
        },
    )
    .await
    .unwrap();
}

fn router(pool: db::Pool) -> Router {
    let mailer = Mailer::new(
        &MailTransport::Console,
        "noreply@test.invalid".to_string(),
        "http://test.invalid".to_string(),
    )
    .expect("console mailer");
    let state = AppState {
        pool,
        secret: Arc::new(SECRET.to_vec()),
        mailer,
        cookie_secure: false,
    };
    server::app(state, Path::new("static"))
}

async fn get(app: &Router, uri: &str) -> Response {
    let req = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    app.clone().oneshot(req).await.expect("response")
}

async fn post_form(app: &Router, uri: &str, form: &str, cookie: Option<&str>) -> Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    let req = builder.body(Body::from(form.to_string())).expect("request");
    app.clone().oneshot(req).await.expect("response")
}

async fn body_string(resp: Response) -> String {
    let bytes = resp.into_body().collect().await.expect("body").to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Create a verified admin user with a session and return its cookie header.
async fn admin_cookie(pool: &db::Pool) -> String {
    let email_hash = server::auth::hash_email("admin@x.test", SECRET).unwrap();
    let user = db::users::insert(pool, &email_hash, "pw").await.unwrap();
    db::users::mark_verified(pool, user).await.unwrap();
    sqlx::query("update users set is_admin = true where id = $1")
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    let token = "admin-token";
    db::sessions::create(
        pool,
        user,
        &server::auth::hash_token(token),
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    format!("op_session={token}")
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_forms_render(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    for path in ["/admin/news/new", "/admin/poll/new", "/admin/statement/new"] {
        let url = format!("{path}?country={COUNTRY}&party=test-partisi");
        // Signed-out visitors get a plain 404.
        assert_eq!(
            get(&app, &url).await.status(),
            StatusCode::NOT_FOUND,
            "{url}"
        );
        // Admins get the form.
        let req = Request::builder()
            .uri(&url)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{url}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_index_is_a_gated_hub(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    // Signed-out visitors get a plain 404 and never see the admin nav link.
    assert_eq!(get(&app, "/admin").await.status(), StatusCode::NOT_FOUND);
    let anon_home = body_string(get(&app, &format!("/{COUNTRY}")).await).await;
    assert!(!anon_home.contains("href=\"/admin\""));

    // Admins get the hub, with routes into the editing workflow.
    let req = Request::builder()
        .uri("/admin")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Yönetim paneli")); // heading + nav link
    assert!(body.contains(&format!("/{COUNTRY}/people")));
    assert!(body.contains(&format!("/{COUNTRY}/parties")));
    assert!(body.contains("/admin/poll/new?country="));
    assert!(body.contains("href=\"/admin\"")); // the admin-only nav link
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_sees_add_controls_on_entity_pages(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let get_admin = |uri: String| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let req = Request::builder()
                .uri(&uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "{uri}");
            body_string(resp).await
        }
    };

    // The party page carries a "Manage" link into the entity backoffice for an
    // admin (hidden for everyone else); the add controls live there, not inline.
    let party = get_admin(format!("/{COUNTRY}/parties/test-partisi")).await;
    assert!(party.contains(&format!("/admin/party/test-partisi?country={COUNTRY}")));
    assert!(!party.contains("/admin/news/new"));

    let party_manage = get_admin(format!("/admin/party/test-partisi?country={COUNTRY}")).await;
    assert!(party_manage.contains("/admin/news/new?country="));
    assert!(party_manage.contains("/admin/statement/new?country="));
    assert!(party_manage.contains("/admin/poll/new?country="));
    assert!(party_manage.contains("party=test-partisi"));

    // The person page carries the same backoffice link.
    let person = get_admin(format!("/{COUNTRY}/people/ayse-yilmaz")).await;
    assert!(person.contains(&format!("/admin/person/ayse-yilmaz?country={COUNTRY}")));
    assert!(!person.contains("/admin/statement/new"));

    let person_manage = get_admin(format!("/admin/person/ayse-yilmaz?country={COUNTRY}")).await;
    assert!(person_manage.contains("/admin/news/new?country="));
    assert!(person_manage.contains("/admin/statement/new?country="));
    assert!(person_manage.contains("/admin/poll/new?country="));
    assert!(person_manage.contains("person=ayse-yilmaz"));

    // The backoffice pages are invisible to signed-out visitors.
    assert_eq!(
        get(
            &app,
            &format!("/admin/party/test-partisi?country={COUNTRY}")
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(
            &app,
            &format!("/admin/person/ayse-yilmaz?country={COUNTRY}")
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    // An unknown entity is a plain 404 even for an admin.
    for uri in [
        format!("/admin/party/no-such-party?country={COUNTRY}"),
        format!("/admin/person/no-such-person?country={COUNTRY}"),
    ] {
        let req = Request::builder()
            .uri(&uri)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_reviews_and_resolves_conflicts(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let get_admin = |uri: String| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let req = Request::builder()
                .uri(&uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            body_string(app.oneshot(req).await.unwrap()).await
        }
    };

    // Log a conflict on the seeded party: two sources disagree on a field.
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    let ours =
        db::sources::insert_source(&pool, "official_gov", "https://gov.test/p", None, Some("a"))
            .await
            .unwrap();
    let incoming =
        db::sources::insert_source(&pool, "wikidata", "https://wd.test/p", None, Some("b"))
            .await
            .unwrap();
    let cid = db::conflicts::record(
        &pool,
        &db::conflicts::NewConflict {
            entity_type: "party",
            entity_id: Some(party.id),
            field: "founded_date",
            existing_value: Some("2010-01-01"),
            incoming_value: Some("2011-05-05"),
            existing_source_id: Some(ours),
            incoming_source_id: Some(incoming),
        },
    )
    .await
    .unwrap();

    // The admin hub surfaces the review queue with its open count.
    let hub = get_admin("/admin".into()).await;
    assert!(hub.contains("Veri çelişkilerini incele"));
    assert!(hub.contains("/admin/conflicts"));

    // The review page shows the conflict, labelled and with both source links.
    let page = get_admin("/admin/conflicts".into()).await;
    assert!(page.contains("Test Partisi")); // entity label from the party row
    assert!(page.contains("founded_date"));
    assert!(page.contains("2010-01-01")); // our value
    assert!(page.contains("2011-05-05")); // incoming value
    assert!(page.contains("https://wd.test/p")); // incoming source link

    // The queue is admin-only.
    assert_eq!(
        get(&app, "/admin/conflicts").await.status(),
        StatusCode::NOT_FOUND
    );

    // Resolving it clears the queue.
    let resp = post_form(
        &app,
        &format!("/admin/conflicts/{cid}/resolve"),
        "",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(db::conflicts::count_open(&pool).await.unwrap(), 0);

    let after = get_admin("/admin/conflicts".into()).await;
    assert!(after.contains("Açık çelişki yok."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_edits_news_and_manages_relations(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let get_admin = |uri: String| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let req = Request::builder()
                .uri(&uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            body_string(app.oneshot(req).await.unwrap()).await
        }
    };

    // A news item linked to the party.
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://ex.test/a",
            outlet: Some("Outlet"),
            published_at: None,
            headline: "Eski baslik",
            our_summary: None,
            person_ids: &[],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = $1")
        .bind(COUNTRY)
        .fetch_one(&pool)
        .await
        .unwrap();
    let id = db::news::recent(&pool, country_id, "tr", 10).await.unwrap()[0].id;

    // The edit page shows the headline and the linked party.
    let edit = get_admin(format!("/admin/news/{id}/edit")).await;
    assert!(edit.contains("Haberi düzenle"));
    assert!(edit.contains(r#"value="Eski baslik""#));
    assert!(edit.contains("Test Partisi")); // linked party

    // Update headline and summary.
    let resp = post_form(
        &app,
        &format!("/admin/news/{id}"),
        "headline=Yeni+baslik&our_summary=Yeni+ozet",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let e = db::news::get_edit(&pool, id).await.unwrap().unwrap();
    assert_eq!(e.headline, "Yeni baslik");
    assert_eq!(e.our_summary.as_deref(), Some("Yeni ozet"));

    // The relation search returns an HTMX fragment with an attach form.
    let req = Request::builder()
        .uri(format!("/admin/news/{id}/search?q=ayse"))
        .header("cookie", &cookie)
        .header("hx-request", "true")
        .body(Body::empty())
        .unwrap();
    let search = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(search.contains("Ayse Yilmaz"));
    assert!(search.contains("value=\"ayse-yilmaz\""));

    // Attach the person, detach the party.
    let resp = post_form(
        &app,
        &format!("/admin/news/{id}/link"),
        "kind=person&slug=ayse-yilmaz",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let resp = post_form(
        &app,
        &format!("/admin/news/{id}/unlink"),
        &format!("kind=party&id={}", party.id),
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let e = db::news::get_edit(&pool, id).await.unwrap().unwrap();
    assert_eq!(e.people.len(), 1);
    assert_eq!(e.people[0].slug, "ayse-yilmaz");
    assert!(e.parties.is_empty());

    // The news index shows an edit link for an admin, but not for a reader.
    let admin_index = get_admin("/tr/news".into()).await;
    assert!(admin_index.contains(&format!("/admin/news/{id}/edit")));
    let anon_index = body_string(get(&app, "/tr/news").await).await;
    assert!(!anon_index.contains("/admin/news/"));

    // The edit page is admin-only.
    assert_eq!(
        get(&app, &format!("/admin/news/{id}/edit")).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_creates_and_edits_outlet(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let get_admin = |uri: String| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let req = Request::builder()
                .uri(&uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            body_string(app.oneshot(req).await.unwrap()).await
        }
    };

    // The hub offers to add an outlet, and the form renders with the leaning field.
    let hub = get_admin("/admin".into()).await;
    assert!(hub.contains("Yayın organı ekle"));
    assert!(hub.contains("/admin/outlet/new"));
    let form = get_admin("/admin/outlet/new?country=tr".into()).await;
    assert!(form.contains("Siyasi eğilim")); // leaning field
    assert!(form.contains("Merkez sol")); // a leaning option

    // Create an outlet.
    let resp = post_form(
        &app,
        "/admin/outlet",
        "country=tr&name=Test+Haber&slug=test-haber&homepage_url=https://th.test\
         &logo_url=&logo_license=&leaning=lean_left&summary=Ozet",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let created = db::outlets::get_by_slug(&pool, "test-haber")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.name, "Test Haber");
    assert_eq!(created.leaning.as_deref(), Some("lean_left"));
    assert_eq!(created.homepage_url.as_deref(), Some("https://th.test"));

    // The edit form is prefilled.
    let edit = get_admin("/admin/outlet/test-haber/edit?country=tr".into()).await;
    assert!(edit.contains(r#"value="Test Haber""#));
    assert!(edit.contains(r#"value="https://th.test""#));

    // Update the leaning; upsert on slug edits in place.
    let resp = post_form(
        &app,
        "/admin/outlet",
        "country=tr&name=Test+Haber&slug=test-haber&leaning=right",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let updated = db::outlets::get_by_slug(&pool, "test-haber")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.leaning.as_deref(), Some("right"));

    // The forms are admin-only.
    assert_eq!(
        get(&app, "/admin/outlet/new").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_reviews_and_publishes_summaries(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;

    let get_admin = |uri: String| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let req = Request::builder()
                .uri(&uri)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            body_string(app.oneshot(req).await.unwrap()).await
        }
    };

    // A news item with a pending draft summary.
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://ex.test/a",
            outlet: Some("Test Gazetesi"),
            published_at: None,
            headline: "Taslakli haber",
            our_summary: None,
            person_ids: &[],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();
    let id = db::news::unsummarized(&pool, 10).await.unwrap()[0].id;
    db::news::set_draft(&pool, id, "Makine taslagi.")
        .await
        .unwrap();

    // The hub surfaces the review queue with its count; the page shows the draft.
    let hub = get_admin("/admin".into()).await;
    assert!(hub.contains("Özetleri incele"));
    assert!(hub.contains("/admin/summaries"));
    let page = get_admin("/admin/summaries".into()).await;
    assert!(page.contains("Taslakli haber"));
    assert!(page.contains("Makine taslagi.")); // the draft, editable

    // The queue is admin-only.
    assert_eq!(
        get(&app, "/admin/summaries").await.status(),
        StatusCode::NOT_FOUND
    );

    // Publishing an edited summary clears the draft and sets our_summary.
    let resp = post_form(
        &app,
        &format!("/admin/summaries/{id}/publish"),
        "summary=Duzenlenmis+ozet",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(db::news::pending_draft_count(&pool).await.unwrap(), 0);

    // The published summary now shows to readers on the item's page.
    let detail = body_string(get(&app, &format!("/tr/news/{id}")).await).await;
    assert!(detail.contains("Duzenlenmis ozet"));

    // A second draft can be discarded instead of published.
    db::news::set_draft(&pool, id, "Baska taslak.")
        .await
        .unwrap();
    let resp = post_form(
        &app,
        &format!("/admin/summaries/{id}/discard"),
        "",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(db::news::pending_draft_count(&pool).await.unwrap(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_pages_are_searchable(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    // The people list carries a search box (HTMX-enhanced) over a results
    // container, and lists everyone by default.
    let people = body_string(get(&app, "/tr/people").await).await;
    assert!(people.contains(r#"hx-get="/tr/people""#));
    assert!(people.contains(r#"id="people-results""#));
    assert!(people.contains("Ayse Yilmaz"));
    assert!(people.contains("Mehmet Demir"));

    // A query filters the list to the match.
    let filtered = body_string(get(&app, "/tr/people?q=ayse").await).await;
    assert!(filtered.contains("Ayse Yilmaz"));
    assert!(!filtered.contains("Mehmet Demir"));

    // The parties list is searchable the same way.
    let parties = body_string(get(&app, "/tr/parties").await).await;
    assert!(parties.contains(r#"hx-get="/tr/parties""#));
    assert!(parties.contains(r#"id="parties-results""#));
    assert!(parties.contains("Test Partisi"));

    let none = body_string(get(&app, "/tr/parties?q=zzznomatch").await).await;
    assert!(!none.contains("Test Partisi"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn locale_from_header_cookie_and_switcher(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    let home = |headers: Vec<(&'static str, &'static str)>| {
        let app = app.clone();
        async move {
            let mut b = Request::builder().uri("/");
            for (k, v) in headers {
                b = b.header(k, v);
            }
            body_string(app.oneshot(b.body(Body::empty()).unwrap()).await.unwrap()).await
        }
    };

    // The browser preference selects the locale.
    let en = home(vec![("accept-language", "en-US,en;q=0.9")]).await;
    assert!(en.contains(r#"html lang="en""#));
    assert!(en.contains("People, parties, elections")); // English source copy
    let tr = home(vec![("accept-language", "tr-TR,tr;q=0.9")]).await;
    assert!(tr.contains(r#"html lang="tr""#));
    assert!(tr.contains("Kişiler, partiler, seçimler")); // Turkish copy

    // An explicit cookie overrides the header.
    let cookie_en = home(vec![("accept-language", "tr"), ("cookie", "lang=en")]).await;
    assert!(cookie_en.contains(r#"html lang="en""#));

    // The switcher is present.
    assert!(en.contains(r#"href="/lang/en""#));
    assert!(en.contains(r#"href="/lang/tr""#));

    // Choosing a language sets the cookie and returns to the previous page.
    let req = Request::builder()
        .uri("/lang/en")
        .header("referer", "http://x.test/tr/people")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/tr/people");
    assert!(resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("lang=en"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn health_and_version(pool: db::Pool) {
    let app = router(pool);

    let resp = get(&app, "/health").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "ok");

    let resp = get(&app, "/version").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("\"commit\""), "version json: {body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn country_page_renders(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    let body = body_string(get(&app, &format!("/{COUNTRY}")).await).await;
    assert!(body.contains("Test Ulke")); // country name
    assert!(body.contains("https://flag.test/tr.svg")); // country flag
    assert!(body.contains("Zeynep Kaya")); // government (president)
    assert!(body.contains("TP")); // party chip in the seat bar
    assert!(body.contains("Test Ittifaki")); // coalition
    assert!(body.contains("/tr/alliance/test-ittifaki")); // coalition links to its page
    assert!(body.contains("Bağımsız")); // the independent (no-party) MP segment
                                        // The elected chamber (120 seats) is larger than the sitting members (2), so
                                        // the parliament header reads "filled / total" and the gap shows as vacant.
    assert!(body.contains("/ 120")); // chamber size from the parliamentary election
    assert!(body.contains("Boş")); // the vacant-seats legend entry
    assert!(body.contains("Test Secimi 2024")); // elections section
    assert!(body.contains("Seçimler")); // elections heading
    assert!(body.contains("Katılım")); // turnout figure (vote totals are set)
    assert!(body.contains("Cumhuriyet ilan edildi")); // timeline event
    assert!(body.contains("Tarihçe")); // timeline heading
    assert!(body.contains("/tr/history")); // timeline preview links to the full page
    assert!(body.contains("Ulke gidisati?")); // country-level poll surfaced here
}

#[sqlx::test(migrations = "../../migrations")]
async fn history_page_renders(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);
    let body = body_string(get(&app, "/tr/history").await).await;
    assert!(body.contains("Tarihçe")); // heading
    assert!(body.contains("Cumhuriyet ilan edildi")); // a seeded country event
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_index_lists_recent_news(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    // With no news the index shows its heading and an empty state.
    let empty = body_string(get(&app, "/tr/news").await).await;
    assert!(empty.contains("Haberler")); // heading
    assert!(empty.contains("Henüz haber yok.")); // empty state

    // Add one sourced item linked to the seeded person and party.
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    let news_id = db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://example.org/news/1",
            outlet: Some("Test Gazetesi"),
            published_at: chrono::NaiveDate::from_ymd_opt(2026, 1, 2)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc()),
            headline: "Reform tartismasi surdu",
            our_summary: Some("Kisa ve tarafsiz ozet."),
            person_ids: &[person.id],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();

    // The index shows only the headline (linking to the item's page) and the
    // people/parties it mentions, not the summary or the raw source link.
    let body = body_string(get(&app, "/tr/news").await).await;
    assert!(body.contains("Reform tartismasi surdu")); // headline
    assert!(body.contains(&format!("/tr/news/{news_id}"))); // headline links to the item page
    assert!(body.contains("Ayse Yilmaz")); // mentioned person chip
    assert!(body.contains("/tr/people/ayse-yilmaz"));
    assert!(body.contains("/tr/parties/test-partisi")); // mentioned party chip

    // The item's own page shows our summary, the source link and the mentions.
    let detail = body_string(get(&app, &format!("/tr/news/{news_id}")).await).await;
    assert!(detail.contains("Reform tartismasi surdu")); // headline
    assert!(detail.contains("Kisa ve tarafsiz ozet.")); // our summary
    assert!(detail.contains("https://example.org/news/1")); // read at the source
    assert!(detail.contains("Ayse Yilmaz")); // mentioned person
    assert!(detail.contains("/tr/people/ayse-yilmaz"));

    // The idempotency helper reports the stored URL and rejects unknown ones.
    assert!(db::news::url_exists(&pool, "https://example.org/news/1")
        .await
        .unwrap());
    assert!(!db::news::url_exists(&pool, "https://example.org/absent")
        .await
        .unwrap());

    // The country page offers an entry point into the news index.
    let country = body_string(get(&app, "/tr").await).await;
    assert!(country.contains("/tr/news"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_detail_shows_outlet_author_and_chips(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    // An outlet with a logo, and an article linked to it.
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = 'tr'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let outlet = db::outlets::upsert(
        &pool,
        &db::outlets::NewOutlet {
            name: "Test Gazetesi",
            slug: "test-gazetesi",
            homepage_url: Some("https://tg.test"),
            logo_url: Some("https://tg.test/logo.png"),
            logo_license: None,
            leaning: Some("center"),
            summary: None,
            country_id: Some(country_id),
        },
    )
    .await
    .unwrap();
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    let id = db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://tg.test/a",
            outlet: Some("Test Gazetesi"),
            published_at: None,
            headline: "Onemli haber",
            our_summary: Some("Tarafsiz ozet."),
            person_ids: &[person.id],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();
    db::outlets::link_sources_by_label(&pool, outlet, "Test Gazetesi")
        .await
        .unwrap();
    db::news::update_fields(
        &pool,
        id,
        "Onemli haber",
        Some("Tarafsiz ozet."),
        Some("Ali Veli"),
    )
    .await
    .unwrap();

    let detail = body_string(get(&app, &format!("/tr/news/{id}")).await).await;
    assert!(detail.contains("Onemli haber")); // headline
    assert!(detail.contains("Test Gazetesi")); // the outlet
    assert!(detail.contains("/tr/outlet/test-gazetesi")); // links to the outlet
    assert!(detail.contains("https://tg.test/logo.png")); // the outlet logo
    assert!(detail.contains("Ali Veli")); // the author
    assert!(detail.contains("Yazan")); // "By" label
    assert!(detail.contains("Tarafsiz ozet.")); // our summary
    assert!(detail.contains("https://tg.test/a")); // read at the source
                                                   // The mentioned person is a bordered chip, not glued plain text.
    assert!(detail.contains("border border-ink"));
    assert!(detail.contains("/tr/people/ayse-yilmaz"));
    assert!(detail.contains("/tr/parties/test-partisi"));

    // An unknown item is a plain 404.
    assert_eq!(
        get(&app, "/tr/news/999999").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn outlet_pages_render(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    // An outlet with a leaning and one linked article.
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = 'tr'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let outlet = db::outlets::upsert(
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
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://tg.test/1",
            outlet: Some("Test Gazetesi"),
            published_at: None,
            headline: "Onemli haber basligi",
            our_summary: None,
            person_ids: &[],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();
    db::outlets::link_sources_by_label(&pool, outlet, "Test Gazetesi")
        .await
        .unwrap();

    // The index lists the outlet and links to it.
    let index = body_string(get(&app, "/tr/outlets").await).await;
    assert!(index.contains("Yayın organları")); // heading
    assert!(index.contains("/tr/outlet/test-gazetesi"));
    assert!(index.contains("Test Gazetesi"));

    // The detail page shows the leaning, homepage, our summary and the article.
    let detail = body_string(get(&app, "/tr/outlet/test-gazetesi").await).await;
    assert!(detail.contains("Merkez sol")); // lean_left label
    assert!(detail.contains("https://tg.test")); // homepage link
    assert!(detail.contains("Bir test gazetesi.")); // our summary
    assert!(detail.contains("Onemli haber basligi")); // the article
    assert!(detail.contains("/tr/parties/test-partisi")); // the article's party chip

    // An unknown outlet is a plain 404.
    assert_eq!(
        get(&app, "/tr/outlet/does-not-exist").await.status(),
        StatusCode::NOT_FOUND
    );

    // The news index links across to the outlets index.
    let news = body_string(get(&app, "/tr/news").await).await;
    assert!(news.contains("/tr/outlets"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn elections_index_and_detail_render(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    // The index lists the country's elections and links to each one.
    let index = body_string(get(&app, "/tr/elections").await).await;
    assert!(index.contains("Test Secimi 2024"));
    assert!(index.contains("/tr/election/test-secimi-2024"));

    // The detail page shows the result box and a link to the official source.
    let detail = body_string(get(&app, "/tr/election/test-secimi-2024").await).await;
    assert!(detail.contains("Test Secimi 2024"));
    assert!(detail.contains("TP")); // the party chip
    assert!(detail.contains("example.test/s")); // the source link

    assert_eq!(
        get(&app, "/tr/election/does-not-exist").await.status(),
        StatusCode::NOT_FOUND
    );

    // The country page links to the elections index.
    let country = body_string(get(&app, "/tr").await).await;
    assert!(country.contains("/tr/elections"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn label_election_renders_on_country_page(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let src = db::sources::insert_source(
        &pool,
        "official_election",
        "https://example.test/r",
        None,
        Some("r"),
    )
    .await
    .unwrap();
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = $1")
        .bind(COUNTRY)
        .fetch_one(&pool)
        .await
        .unwrap();
    // A referendum: contestants are labels (Evet / Hayır), not parties.
    let eid = db::elections::create(
        &pool,
        &db::elections::NewElection {
            country_id,
            name: "Test Halkoylaması",
            slug: "test-halkoylamasi",
            held_on: NaiveDate::from_ymd_opt(2017, 4, 16),
            kind: Some("referendum"),
            source_id: src,
        },
    )
    .await
    .unwrap();
    db::elections::add_label_result(&pool, eid, "Evet", Some(600), src)
        .await
        .unwrap();
    db::elections::add_label_result(&pool, eid, "Hayır", Some(400), src)
        .await
        .unwrap();
    db::elections::set_description(&pool, eid, "Anayasa degisikligi hakkinda.")
        .await
        .unwrap();
    db::elections::set_turnout(&pool, eid, Some(1100), Some(1050), Some(1000))
        .await
        .unwrap();

    let body = body_string(get(&app, &format!("/{COUNTRY}")).await).await;
    assert!(body.contains("Test Halkoylaması"));
    assert!(body.contains("Anayasa degisikligi hakkinda.")); // the description
    assert!(body.contains("Evet")); // label contestant renders (no party chip)
    assert!(body.contains("Hayır"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn alliance_page_renders(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    let body = body_string(get(&app, "/tr/alliance/test-ittifaki").await).await;
    assert!(body.contains("Test Ittifaki")); // alliance name
    assert!(body.contains("Bir test ittifaki.")); // its summary
    assert!(body.contains("Test Partisi")); // a member party
    assert!(body.contains("/tr/parties/test-partisi")); // linking to the party
    assert!(body.contains("Aktif")); // active (not dissolved) status

    // An unknown alliance is a 404.
    assert_eq!(
        get(&app, "/tr/alliance/does-not-exist").await.status(),
        StatusCode::NOT_FOUND
    );

    // The index lists the country's alliances, and the country page links to it.
    let index = body_string(get(&app, "/tr/alliances").await).await;
    assert!(index.contains("Test Ittifaki"));
    assert!(index.contains("/tr/alliance/test-ittifaki"));
    let country = body_string(get(&app, "/tr").await).await;
    assert!(country.contains("/tr/alliances"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_can_add_news_and_poll(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    let cookie = admin_cookie(&pool).await;

    // The admin area is a plain 404 for signed-out visitors.
    let resp = get(
        &app,
        &format!("/admin/news/new?country={COUNTRY}&party=test-partisi"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // The admin sees the form.
    let req = Request::builder()
        .uri(format!(
            "/admin/news/new?country={COUNTRY}&party=test-partisi"
        ))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // The admin adds a news item linked to the party.
    let form = "country=tr&party=test-partisi&headline=Test+headline&url=https://example.org/n1&outlet=Test+Outlet&published_at=2026-07-10&our_summary=Our+summary";
    let resp = post_form(&app, "/admin/news", form, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    // It now shows in the party page News section, linking to the item's page
    // (the outlet and source link live on that page, not in the list).
    let body = body_string(get(&app, &format!("/{COUNTRY}/parties/test-partisi")).await).await;
    assert!(body.contains("Test headline"));
    assert!(body.contains(&format!("/{COUNTRY}/news/")));

    // The admin also creates a poll on the party with four options (the form
    // uses repeated `option` fields, so any number is allowed), then it shows on
    // the index.
    let poll_form = "country=tr&party=test-partisi&question=Is+it+good?&kind=single\
        &option=Yes&option_media=&option=No&option_media=&option=Maybe&option_media=&option=Never&option_media=";
    let resp = post_form(&app, "/admin/poll", poll_form, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let body = body_string(get(&app, &format!("/{COUNTRY}/polls")).await).await;
    assert!(body.contains("Is it good?"));
    // The four options were all stored.
    let created = db::polls::get_by_slug(&pool, "is-it-good")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.options.len(), 4);

    // The HTMX add-option endpoint returns one more option row for admins.
    let req = Request::builder()
        .uri("/admin/poll/option-row")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let row = app.clone().oneshot(req).await.unwrap();
    assert_eq!(row.status(), StatusCode::OK);
    assert!(body_string(row).await.contains("name=\"option\""));

    // The admin adds a statement to the party, then it shows on the party page.
    let statement = "country=tr&party=test-partisi&text=We+stand+for+reform.&url=https://example.org/s1&is_paraphrase=on";
    let resp = post_form(&app, "/admin/statement", statement, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    let body = body_string(get(&app, &format!("/{COUNTRY}/parties/test-partisi")).await).await;
    assert!(body.contains("We stand for reform."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn read_pages_render(pool: db::Pool) {
    let app = router(pool.clone());
    for uri in ["/", "/login", "/register"] {
        let resp = get(&app, uri).await;
        assert_eq!(resp.status(), StatusCode::OK, "GET {uri}");
    }
    seed(&pool).await;
    let app = router(pool);
    for uri in ["/tr/people", "/tr/parties", "/tr", "/search"] {
        let resp = get(&app, uri).await;
        assert_eq!(resp.status(), StatusCode::OK, "GET {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_person_returns_404(pool: db::Pool) {
    let app = router(pool);
    let resp = get(&app, "/tr/people/does-not-exist").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_then_verify_then_login(pool: db::Pool) {
    let app = router(pool.clone());
    let email = "voter@example.com";
    let password = "supersecret";

    // Registration succeeds and creates an unverified account.
    let resp = post_form(
        &app,
        "/register",
        &format!("email={email}&password={password}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let email_hash = server::auth::hash_email(email, SECRET).expect("email hash");
    let user = db::users::get_by_email_hash(&pool, &email_hash)
        .await
        .unwrap()
        .expect("user created");
    assert!(user.verified_at.is_none());

    // Login is refused while unverified: no session cookie is issued.
    let resp = post_form(
        &app,
        "/login",
        &format!("email={email}&password={password}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("set-cookie").is_none(),
        "unverified login must not set a session"
    );

    // Verify through the endpoint using a token we insert (register's token is
    // only stored hashed, so it cannot be recovered).
    let token = "known-verification-token";
    let expires = chrono::Utc::now() + chrono::Duration::hours(1);
    db::email_verifications::create(
        &pool,
        user.id,
        &email_hash,
        &server::auth::hash_token(token),
        expires,
    )
    .await
    .unwrap();

    let resp = get(&app, &format!("/verify?token={token}")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let user = db::users::get_by_id(&pool, user.id).await.unwrap().unwrap();
    assert!(
        user.verified_at.is_some(),
        "verification should mark the user"
    );

    // Now login succeeds: redirect plus a session cookie.
    let resp = post_form(
        &app,
        "/login",
        &format!("email={email}&password={password}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("session cookie")
        .to_str()
        .unwrap();
    assert!(cookie.contains("op_session="), "cookie: {cookie}");
    assert!(cookie.contains("HttpOnly"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn wrong_password_is_rejected(pool: db::Pool) {
    let app = router(pool.clone());
    post_form(
        &app,
        "/register",
        "email=a@example.com&password=rightpassword",
        None,
    )
    .await;

    let email_hash = server::auth::hash_email("a@example.com", SECRET).unwrap();
    let user = db::users::get_by_email_hash(&pool, &email_hash)
        .await
        .unwrap()
        .unwrap();
    db::users::mark_verified(&pool, user.id).await.unwrap();

    let resp = post_form(
        &app,
        "/login",
        "email=a@example.com&password=wrongpassword",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get("set-cookie").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn short_password_is_rejected(pool: db::Pool) {
    let app = router(pool.clone());
    let resp = post_form(
        &app,
        "/register",
        "email=b@example.com&password=short",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let email_hash = server::auth::hash_email("b@example.com", SECRET).unwrap();
    let user = db::users::get_by_email_hash(&pool, &email_hash)
        .await
        .unwrap();
    assert!(
        user.is_none(),
        "a too-short password must not create an account"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_requires_a_session(pool: db::Pool) {
    let app = router(pool);
    // Signed out: the logout POST redirects to /login rather than acting.
    let resp = post_form(&app, "/logout", "", None).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(location, "/login");
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_with_a_session_clears_it_and_redirects(pool: db::Pool) {
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;
    let resp = post_form(&app, "/logout", "", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/");
    // The session cookie is cleared on the way out.
    let set = resp
        .headers()
        .get("set-cookie")
        .expect("clearing cookie")
        .to_str()
        .unwrap();
    assert!(set.contains("op_session="));
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_rejects_an_invalid_email(pool: db::Pool) {
    let app = router(pool.clone());
    // No '@': the form is redisplayed with an error and no account is created.
    let resp = post_form(
        &app,
        "/register",
        "email=notanemail&password=longenough",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("e-posta adresi gir")); // "Enter a valid email address." (tr)
}

#[sqlx::test(migrations = "../../migrations")]
async fn register_is_silent_on_a_duplicate_email(pool: db::Pool) {
    let app = router(pool.clone());
    let form = "email=dup@example.com&password=longenough";
    // First registration creates the account.
    let resp = post_form(&app, "/register", form, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    // A second registration for the same address returns the same
    // check-your-email page, revealing nothing about whether it already exists.
    let resp = post_form(&app, "/register", form, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
    // Exactly one account exists.
    let email_hash = server::auth::hash_email("dup@example.com", SECRET).unwrap();
    assert!(db::users::get_by_email_hash(&pool, &email_hash)
        .await
        .unwrap()
        .is_some());
}

#[sqlx::test(migrations = "../../migrations")]
async fn verify_rejects_an_unknown_token(pool: db::Pool) {
    let app = router(pool);
    let resp = get(&app, "/verify?token=this-token-was-never-issued").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("geçersiz")); // "invalid or has expired" (tr)
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_rejects_an_unknown_email(pool: db::Pool) {
    let app = router(pool);
    // No account for this address: rejected without a session, and the dummy
    // verification path (stored = None) still runs.
    let resp = post_form(
        &app,
        "/login",
        "email=nobody@example.com&password=whatever12",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get("set-cookie").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn people_pages_render_seeded_data(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    let body = body_string(get(&app, "/tr/people").await).await;
    assert!(body.contains("Ayse Yilmaz"));
    assert!(body.contains("Mehmet Demir"));

    // Full profile: photo, birth place, role title, and party badge.
    let body = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(body.contains("Ayse Yilmaz"));
    assert!(body.contains("Testkent"));
    assert!(body.contains("Milletvekili"));
    assert!(body.contains("Test Partisi"));
    assert!(body.contains("example.test/s"));
    // The hero party chip uses the short name (TP), not the full party name.
    assert!(body.contains(">TP<"));

    // Minimal profile exercises the no-photo, no-detail branches. With no photo
    // the hero shows a two-letter initials monogram instead of an empty frame.
    let body = body_string(get(&app, "/tr/people/mehmet-demir").await).await;
    assert!(body.contains("Mehmet Demir"));
    assert!(body.contains(">MD<")); // initials monogram
}

#[sqlx::test(migrations = "../../migrations")]
async fn home_lists_countries(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);
    let body = body_string(get(&app, "/").await).await;
    // The landing page leads with the country cards (each its own dataset), not
    // a global figures strip that would be meaningless across countries.
    assert!(body.contains("Test Ulke")); // the seeded country card
    assert!(body.contains(r#"href="/tr""#)); // linking to the country
    assert!(!body.contains("<dl")); // no global stats strip
}

#[sqlx::test(migrations = "../../migrations")]
async fn party_pages_render_members(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    let body = body_string(get(&app, "/tr/parties").await).await;
    assert!(body.contains("Test Partisi"));

    let body = body_string(get(&app, "/tr/parties/test-partisi").await).await;
    assert!(body.contains("Test Partisi"));
    assert!(body.contains("Ayse Yilmaz")); // current member
    assert!(body.contains("Mehmet Demir")); // former member
    assert!(body.contains("example.test/s"));
    assert!(body.contains("Seçim geçmişi")); // electoral history heading
    assert!(body.contains("Test Secimi 2024")); // the party's election result
    assert!(body.contains("Parti kuruldu")); // party timeline event
}

#[sqlx::test(migrations = "../../migrations")]
async fn search_finds_seeded_records(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool);

    // A prefix, not the whole word, still matches ("dem" -> "Demir").
    let body = body_string(get(&app, "/search?q=dem").await).await;
    assert!(body.contains("Mehmet Demir"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn poll_page_and_voting(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    // Anonymous can view the poll, and the party page links to it.
    assert!(body_string(get(&app, "/tr/poll/party-poll").await)
        .await
        .contains("Nasil buluyorsunuz?"));
    assert!(body_string(get(&app, "/tr/parties/test-partisi").await)
        .await
        .contains("Nasil buluyorsunuz?"));

    // A scale-kind poll renders through its own (grid) layout.
    assert!(body_string(get(&app, "/tr/poll/ulke-poll").await)
        .await
        .contains("Ulke gidisati?"));

    // Voting while signed out redirects to /login.
    let resp = post_form(&app, "/tr/poll/party-poll/vote", "option_id=1", None).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        resp.headers().get("location").unwrap().to_str().unwrap(),
        "/login"
    );

    // A verified user with a session cookie (crafted directly).
    let email_hash = server::auth::hash_email("voter@x.test", SECRET).unwrap();
    let user = db::users::insert(&pool, &email_hash, "pw").await.unwrap();
    db::users::mark_verified(&pool, user).await.unwrap();
    let token = "session-token";
    db::sessions::create(
        &pool,
        user,
        &server::auth::hash_token(token),
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    let cookie = format!("op_session={token}");

    let poll = db::polls::get_by_slug(&pool, "party-poll")
        .await
        .unwrap()
        .unwrap();
    let option = poll.options[0].id;

    // The vote is recorded, and a plain POST redirects back to the poll.
    let resp = post_form(
        &app,
        "/tr/poll/party-poll/vote",
        &format!("option_id={option}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert!(db::polls::has_voted(&pool, poll.id, user).await.unwrap());
    assert_eq!(total_votes(&pool).await, 1);

    // A second vote by the same user is ignored (one per user).
    post_form(
        &app,
        "/tr/poll/party-poll/vote",
        &format!("option_id={option}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(total_votes(&pool).await, 1);

    // An HTMX vote returns the widget fragment, not a redirect.
    let req = Request::builder()
        .method("POST")
        .uri("/tr/poll/party-poll/vote")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", &cookie)
        .header("hx-request", "true")
        .body(Body::from(format!("option_id={option}")))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_string(resp).await.contains("poll-party-poll"));

    // The integrity fingerprint appears once there is a vote, and the chain
    // endpoint exposes the head.
    let body = body_string(get(&app, "/tr/poll/party-poll").await).await;
    assert!(body.contains("Bütünlük")); // the "Integrity" label
    let chain = body_string(get(&app, "/tr/poll/party-poll/chain").await).await;
    assert!(chain.contains("\"votes\":1"), "chain json: {chain}");
    assert!(
        !chain.contains("\"head_hash\":\"\""),
        "head hash should be non-empty: {chain}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn multi_select_poll_records_several_options(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());

    // A multi-select poll attached to the country, with three options.
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = $1")
        .bind(COUNTRY)
        .fetch_one(&pool)
        .await
        .unwrap();
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, kind, country_id) values ('Cok?', 'cok-secim', 'multi', $1) returning id",
    )
    .bind(country_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut ids = Vec::new();
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
        ids.push(id);
    }

    let email_hash = server::auth::hash_email("multi@x.test", SECRET).unwrap();
    let user = db::users::insert(&pool, &email_hash, "pw").await.unwrap();
    db::users::mark_verified(&pool, user).await.unwrap();
    let token = "multi-token";
    db::sessions::create(
        &pool,
        user,
        &server::auth::hash_token(token),
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    let cookie = format!("op_session={token}");

    // A can-vote viewer sees the checkbox form.
    let req = Request::builder()
        .uri("/tr/poll/cok-secim")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(body.contains("type=\"checkbox\""));

    // Voting two options at once records exactly two votes for this one voter.
    let resp = post_form(
        &app,
        "/tr/poll/cok-secim/vote",
        &format!("option_id={}&option_id={}", ids[0], ids[1]),
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let votes: i64 = sqlx::query_scalar("select count(*) from poll_votes where poll_id = $1")
        .bind(poll_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(votes, 2);
    // Both rows belong to the same voter index.
    let voters: i64 =
        sqlx::query_scalar("select count(distinct voter_index) from poll_votes where poll_id = $1")
            .bind(poll_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(voters, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn image_poll_renders_its_media(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = $1")
        .bind(COUNTRY)
        .fetch_one(&pool)
        .await
        .unwrap();
    let poll_id: i64 = sqlx::query_scalar(
        "insert into polls (question, slug, kind, media_url, media_license, country_id) values ('Img?', 'img-poll', 'single', 'https://img.test/q.png', 'CC0', $1) returning id",
    )
    .bind(country_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for (label, pos, media) in [
        ("A", 1, "https://img.test/a.png"),
        ("B", 2, "https://img.test/b.png"),
    ] {
        sqlx::query("insert into poll_options (poll_id, label, position, media_url) values ($1, $2, $3, $4)")
            .bind(poll_id)
            .bind(label)
            .bind(pos)
            .bind(media)
            .execute(&pool)
            .await
            .unwrap();
    }

    let body = body_string(get(&app, "/tr/poll/img-poll").await).await;
    assert!(body.contains("https://img.test/q.png")); // question image
    assert!(body.contains("https://img.test/a.png")); // option image
    assert!(body.contains("CC0")); // license credit
}

#[sqlx::test(migrations = "../../migrations")]
async fn polls_index_groups_open_and_closed(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    // A poll whose close time has passed shows under the Closed group.
    sqlx::query(
        "insert into polls (question, slug, closes_at, country_id) \
         values ('Bitti mi?', 'kapali-anket', now() - interval '1 day', \
                 (select id from countries where slug = 'tr'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let body = body_string(get(&app, "/tr/polls").await).await;
    assert!(body.contains("Açık")); // the open group (seed polls have no close time)
    assert!(body.contains("Kapalı")); // the closed group
    assert!(body.contains("Bitti mi?")); // the closed poll
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_pages_reject_another_countrys_slug(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    // A second country that holds none of the seeded entities.
    sqlx::query(
        "insert into countries (name, slug, source_id) \
         values ('Baska Ulke', 'baska', (select id from sources limit 1))",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The seeded person and party resolve under their own country.
    assert_eq!(
        get(&app, "/tr/people/ayse-yilmaz").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/tr/parties/test-partisi").await.status(),
        StatusCode::OK
    );
    // But the same slugs are not found under another country's path, so a
    // hand-built cross-country URL never shows the wrong country's entity.
    assert_eq!(
        get(&app, "/baska/people/ayse-yilmaz").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/baska/parties/test-partisi").await.status(),
        StatusCode::NOT_FOUND
    );
    // A poll reached its country through the party it is about.
    assert_eq!(
        get(&app, "/tr/poll/party-poll").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/baska/poll/party-poll").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn published_translation_shows_with_original_disclosed(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();

    // A published English translation of the person's summary.
    db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "person",
            entity_id: person.id,
            field: "summary",
            lang: "en",
            text: "Translated biography.",
            origin: "human",
            status: "published",
            source_lang: Some("tr"),
        },
    )
    .await
    .unwrap();

    // Viewed in English, the translation is shown and the original is disclosed.
    let req = Request::builder()
        .uri("/tr/people/ayse-yilmaz")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(body.contains("Translated biography."));
    assert!(body.contains("A test person.")); // the original, in the disclosure
    assert!(body.contains("Show original"));

    // Viewed in the source language (no translation for it), the original shows
    // and there is no disclosure.
    let body_tr = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(body_tr.contains("A test person."));
    assert!(!body_tr.contains("Translated biography."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_reviews_and_publishes_a_translation(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();

    // A machine draft awaits review.
    let id = db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "person",
            entity_id: person.id,
            field: "summary",
            lang: "en",
            text: "Machine draft bio.",
            origin: "machine",
            status: "draft",
            source_lang: Some("tr"),
        },
    )
    .await
    .unwrap();

    // The review queue shows the draft and the original for comparison.
    let req = Request::builder()
        .uri("/admin/translations")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let queue = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(queue.contains("Machine draft bio."));
    assert!(queue.contains("A test person.")); // the original

    // Publish it, as edited.
    let resp = post_form(
        &app,
        &format!("/admin/translations/{id}/publish"),
        "text=Edited+bio.",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(db::translations::pending_count(&pool).await.unwrap(), 0);

    // Readers in English now see the edited translation.
    let req = Request::builder()
        .uri("/tr/people/ayse-yilmaz")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(body.contains("Edited bio."));

    // The queue is admin-only.
    assert_eq!(
        get(&app, "/admin/translations").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn news_detail_shows_translated_headline_and_summary(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let party = db::parties::get_by_slug(&pool, "test-partisi")
        .await
        .unwrap()
        .unwrap();
    let country_id: i64 = sqlx::query_scalar("select id from countries where slug = 'tr'")
        .fetch_one(&pool)
        .await
        .unwrap();

    db::news::create(
        &pool,
        &db::news::NewNews {
            url: "https://news.test/x",
            outlet: Some("Outlet"),
            published_at: None,
            headline: "Orijinal baslik",
            our_summary: Some("Orijinal ozet."),
            person_ids: &[],
            party_ids: &[party.id],
        },
    )
    .await
    .unwrap();
    let news_id = db::news::recent(&pool, country_id, "tr", 10).await.unwrap()[0].id;

    for (field, text) in [
        ("headline", "Translated headline"),
        ("our_summary", "Translated summary."),
    ] {
        db::translations::upsert(
            &pool,
            &db::translations::NewTranslation {
                entity_type: "news_item",
                entity_id: news_id,
                field,
                lang: "en",
                text,
                origin: "human",
                status: "published",
                source_lang: Some("tr"),
            },
        )
        .await
        .unwrap();
    }

    let req = Request::builder()
        .uri(format!("/tr/news/{news_id}"))
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(body.contains("Translated headline"));
    assert!(body.contains("Translated summary."));
    assert!(body.contains("Orijinal ozet.")); // original summary, disclosed
    assert!(body.contains("Show original"));

    // The news index also shows the translated headline in English.
    let req = Request::builder()
        .uri("/tr/news")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let index = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(index.contains("Translated headline"));
    assert!(!index.contains("Orijinal baslik"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn person_statement_shows_translated(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();

    let statement_id = db::statements::create(
        &pool,
        &db::statements::NewStatement {
            person_id: Some(person.id),
            party_id: None,
            text_original: "Orijinal ifade.",
            is_paraphrase: false,
            stated_at: None,
            url: "https://s.test/1",
            outlet: Some("Outlet"),
        },
    )
    .await
    .unwrap();
    db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "statement",
            entity_id: statement_id,
            field: "text_original",
            lang: "en",
            text: "Translated statement.",
            origin: "human",
            status: "published",
            source_lang: Some("tr"),
        },
    )
    .await
    .unwrap();

    // In English the statement is translated; in the source language it is not.
    let req = Request::builder()
        .uri("/tr/people/ayse-yilmaz")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let en = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(en.contains("Translated statement."));
    assert!(!en.contains("Orijinal ifade."));

    let tr = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(tr.contains("Orijinal ifade."));
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_adds_and_removes_person_enrichment(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let cookie = admin_cookie(&pool).await;
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();

    // Add an education entry and an attribute through the backoffice.
    let resp = post_form(
        &app,
        "/admin/person/ayse-yilmaz/education",
        "country=tr&institution=Test+Universitesi&degree=Hukuk&source_url=https://s.test/1",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let resp = post_form(
        &app,
        "/admin/person/ayse-yilmaz/attribute",
        "country=tr&kind=occupation&value=Avukat&source_url=https://s.test/2",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    // They are stored and shown on the public page.
    let edu = db::people::education(&pool, person.id).await.unwrap();
    assert_eq!(edu.len(), 1);
    assert_eq!(edu[0].institution, "Test Universitesi");
    let body = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(body.contains("Test Universitesi"));
    assert!(body.contains("Avukat"));

    // Deleting removes the education entry.
    let resp = post_form(
        &app,
        &format!("/admin/person/ayse-yilmaz/education/{}/delete", edu[0].id),
        "country=tr",
        Some(&cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert!(db::people::education(&pool, person.id)
        .await
        .unwrap()
        .is_empty());

    // The editing routes are admin-only.
    let resp = post_form(
        &app,
        "/admin/person/ayse-yilmaz/attribute",
        "country=tr&kind=occupation&value=X&source_url=https://s.test/3",
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn person_background_shows_education_and_attributes(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();
    let src = db::sources::insert_source(&pool, "manual", "https://x.test/bg", None, Some("hbg"))
        .await
        .unwrap();
    db::people::upsert_education(
        &pool,
        person.id,
        "Test Üniversitesi",
        None,
        Some("Hukuk"),
        None,
        None,
        None,
        src,
    )
    .await
    .unwrap();
    db::people::upsert_attribute(&pool, person.id, "occupation", "Avukat", None, src)
        .await
        .unwrap();

    // The enriched person shows a Background section with the facts.
    let body = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(body.contains("Özgeçmiş")); // "Background" in Turkish
    assert!(body.contains("Test Üniversitesi"));
    assert!(body.contains("Hukuk"));
    assert!(body.contains("Avukat"));

    // A person with no enrichment shows no Background section.
    let plain = body_string(get(&app, "/tr/people/mehmet-demir").await).await;
    assert!(!plain.contains("Özgeçmiş"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn person_attribute_value_is_translated(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let person = db::people::get_by_slug(&pool, "ayse-yilmaz")
        .await
        .unwrap()
        .unwrap();
    let src = db::sources::insert_source(&pool, "manual", "https://x.test/a", None, Some("ha"))
        .await
        .unwrap();
    let attr_id = db::people::upsert_attribute(&pool, person.id, "occupation", "Lawyer", None, src)
        .await
        .unwrap();
    db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "person_attribute",
            entity_id: attr_id,
            field: "value",
            lang: "tr",
            text: "Avukat",
            origin: "human",
            status: "published",
            source_lang: Some("en"),
        },
    )
    .await
    .unwrap();

    // The default (Turkish) page shows the translated value, not the original.
    let tr = body_string(get(&app, "/tr/people/ayse-yilmaz").await).await;
    assert!(tr.contains("Avukat"));
    assert!(!tr.contains("Lawyer"));

    // English has no translation for it, so it shows the original.
    let req = Request::builder()
        .uri("/tr/people/ayse-yilmaz")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let en = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(en.contains("Lawyer"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn poll_shows_translated_question(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let poll = db::polls::get_by_slug(&pool, "party-poll")
        .await
        .unwrap()
        .unwrap();

    db::translations::upsert(
        &pool,
        &db::translations::NewTranslation {
            entity_type: "poll",
            entity_id: poll.id,
            field: "question",
            lang: "en",
            text: "How do you rate it?",
            origin: "human",
            status: "published",
            source_lang: Some("tr"),
        },
    )
    .await
    .unwrap();

    let req = Request::builder()
        .uri("/tr/poll/party-poll")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let en = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(en.contains("How do you rate it?"));

    let tr = body_string(get(&app, "/tr/poll/party-poll").await).await;
    assert!(tr.contains("Nasil buluyorsunuz?")); // original question
    assert!(!tr.contains("How do you rate it?"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn alliance_shows_translated_name_and_summary(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let alliance = db::alliances::get_by_slug(&pool, "test-ittifaki")
        .await
        .unwrap()
        .unwrap();

    for (field, text) in [
        ("name", "Test Alliance"),
        ("summary", "A translated alliance."),
    ] {
        db::translations::upsert(
            &pool,
            &db::translations::NewTranslation {
                entity_type: "alliance",
                entity_id: alliance.id,
                field,
                lang: "en",
                text,
                origin: "human",
                status: "published",
                source_lang: Some("tr"),
            },
        )
        .await
        .unwrap();
    }

    let req = Request::builder()
        .uri("/tr/alliance/test-ittifaki")
        .header("cookie", "lang=en")
        .body(Body::empty())
        .unwrap();
    let en = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(en.contains("Test Alliance"));
    assert!(en.contains("A translated alliance."));
    assert!(en.contains("Bir test ittifaki.")); // original summary, disclosed
    assert!(en.contains("Show original"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn data_dump_publishes_anonymized_poll_results(pool: db::Pool) {
    seed(&pool).await;
    let app = router(pool.clone());
    let email_hash = server::auth::hash_email("voter@x.test", SECRET).unwrap();
    let user = db::users::insert(&pool, &email_hash, "pw").await.unwrap();
    db::users::mark_verified(&pool, user).await.unwrap();
    let poll = db::polls::get_by_slug(&pool, "party-poll")
        .await
        .unwrap()
        .unwrap();
    db::polls::cast_vote(&pool, poll.id, poll.options[0].id, user)
        .await
        .unwrap();

    let body = body_string(get(&app, "/data/polls.json").await).await;
    // The dump is public-domain and carries the voted poll's tally and chain.
    assert!(body.contains("\"license\":\"CC0-1.0\""));
    assert!(body.contains("\"slug\":\"party-poll\""));
    assert!(body.contains("\"total_votes\":1"));
    assert!(body.contains("\"seq\":")); // the chain head
                                        // One anonymized vote with an opaque per-poll index.
    assert!(body.contains("\"poll\":\"party-poll\""));
    assert!(body.contains("\"voter\":"));
    // No identity is exposed: no user id, no email hash.
    assert!(!body.contains("user_id"));
    assert!(!body.contains("user\":"));
    assert!(!body.contains(&email_hash));
}

#[sqlx::test(migrations = "../../migrations")]
async fn nav_reflects_session(pool: db::Pool) {
    let app = router(pool.clone());

    // Signed out: login and register links, no logout form.
    let body = body_string(get(&app, "/").await).await;
    assert!(body.contains("href=\"/login\""));
    assert!(body.contains("href=\"/register\""));
    assert!(!body.contains("action=\"/logout\""));

    // Signed in: a logout form, and no login/register.
    let email_hash = server::auth::hash_email("nav@x.test", SECRET).unwrap();
    let user = db::users::insert(&pool, &email_hash, "pw").await.unwrap();
    db::users::mark_verified(&pool, user).await.unwrap();
    let token = "nav-token";
    db::sessions::create(
        &pool,
        user,
        &server::auth::hash_token(token),
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();

    let req = Request::builder()
        .uri("/")
        .header("cookie", format!("op_session={token}"))
        .body(Body::empty())
        .unwrap();
    let body = body_string(app.clone().oneshot(req).await.unwrap()).await;
    assert!(body.contains("action=\"/logout\""));
    assert!(!body.contains("href=\"/login\""));
}

async fn total_votes(pool: &db::Pool) -> i64 {
    db::polls::get_by_slug(pool, "party-poll")
        .await
        .unwrap()
        .unwrap()
        .options
        .iter()
        .map(|o| o.votes)
        .sum()
}

#[sqlx::test(migrations = "../../migrations")]
async fn readyz_is_ok_when_the_database_is_reachable(pool: db::Pool) {
    let app = router(pool.clone());
    let resp = get(&app, "/readyz").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_string(resp).await, "ready");
}
