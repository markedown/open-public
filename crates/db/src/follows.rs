//! Following entities and the personal activity feed built from what a user
//! follows. Everything the feed surfaces is already sourced content elsewhere in
//! the platform; this only gathers it per follower. No user-to-user graph.

use chrono::{DateTime, NaiveTime, Utc};

use crate::{Pool, Result};

/// The entity kinds a user can follow.
pub const KINDS: &[&str] = &["person", "party", "country", "topic"];

/// Whether `kind` is a followable entity type.
pub fn is_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Follow an entity. Idempotent: following again is a no-op.
pub async fn follow(pool: &Pool, user_id: i64, entity_type: &str, entity_id: i64) -> Result<()> {
    sqlx::query!(
        "insert into follows (user_id, entity_type, entity_id) values ($1, $2, $3)
         on conflict (user_id, entity_type, entity_id) do nothing",
        user_id,
        entity_type,
        entity_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Unfollow an entity. Idempotent.
pub async fn unfollow(pool: &Pool, user_id: i64, entity_type: &str, entity_id: i64) -> Result<()> {
    sqlx::query!(
        "delete from follows where user_id = $1 and entity_type = $2 and entity_id = $3",
        user_id,
        entity_type,
        entity_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether the user already follows this entity.
pub async fn is_following(
    pool: &Pool,
    user_id: i64,
    entity_type: &str,
    entity_id: i64,
) -> Result<bool> {
    let exists = sqlx::query_scalar!(
        r#"select exists(
            select 1 from follows where user_id = $1 and entity_type = $2 and entity_id = $3
        ) as "exists!""#,
        user_id,
        entity_type,
        entity_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// How many entities the user follows.
pub async fn count_for_user(pool: &Pool, user_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from follows where user_id = $1"#,
        user_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// The followed entity ids of one kind, for building the feed.
async fn followed_ids(pool: &Pool, user_id: i64, kind: &str) -> Result<Vec<i64>> {
    let ids = sqlx::query_scalar!(
        "select entity_id from follows where user_id = $1 and entity_type = $2",
        user_id,
        kind,
    )
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

/// One entry in the personal feed.
pub struct FeedItem {
    /// `poll`, `news` or `election`.
    pub kind: String,
    pub title: String,
    /// A ready-to-use in-site path.
    pub href: String,
    /// When it happened, for ordering and display. `None` sorts last.
    pub occurred_at: Option<DateTime<Utc>>,
    /// A short context line (an outlet, a date label), when there is one.
    pub meta: Option<String>,
}

/// The personal feed: recent polls, news and elections about the entities the
/// user follows, newest first, capped at `limit`. Derived entirely from existing
/// sourced content.
pub async fn feed(pool: &Pool, user_id: i64, limit: i64) -> Result<Vec<FeedItem>> {
    let people = followed_ids(pool, user_id, "person").await?;
    let parties = followed_ids(pool, user_id, "party").await?;
    let countries = followed_ids(pool, user_id, "country").await?;
    let topics = followed_ids(pool, user_id, "topic").await?;

    if people.is_empty() && parties.is_empty() && countries.is_empty() && topics.is_empty() {
        return Ok(Vec::new());
    }

    let mut items: Vec<FeedItem> = Vec::new();

    // Polls attached to a followed country, party, person or topic. The country
    // slug for the link is resolved from whichever the poll hangs off.
    let polls = sqlx::query!(
        r#"
        select p.question, p.slug,
               coalesce(pc.slug, ptc.slug, pec.slug) as country_slug,
               p.opens_at
        from polls p
        left join countries pc on pc.id = p.country_id
        left join parties pt on pt.id = p.party_id
        left join countries ptc on ptc.id = pt.country_id
        left join people pe on pe.id = p.person_id
        left join countries pec on pec.id = pe.country_id
        where p.country_id = any($1) or p.party_id = any($2)
           or p.person_id = any($3) or p.topic_id = any($4)
        order by p.opens_at desc nulls last, p.id desc
        limit $5
        "#,
        &countries,
        &parties,
        &people,
        &topics,
        limit,
    )
    .fetch_all(pool)
    .await?;
    for r in polls {
        if let Some(cs) = r.country_slug {
            items.push(FeedItem {
                kind: "poll".into(),
                title: r.question,
                href: format!("/{}/poll/{}", cs, r.slug),
                occurred_at: r.opens_at,
                meta: None,
            });
        }
    }

    // News mentioning a followed person or party. A single item can be reached
    // through several follows, so dedupe on the item id, keeping the first.
    let news = sqlx::query!(
        r#"
        select distinct on (n.id) n.id, n.headline, s.published_at, co.slug as "country_slug!"
        from news_items n
        join sources s on s.id = n.source_id
        join (
            select nip.news_item_id, pe.country_id
            from news_item_people nip join people pe on pe.id = nip.person_id
            where nip.person_id = any($1)
            union
            select nipp.news_item_id, pt.country_id
            from news_item_parties nipp join parties pt on pt.id = nipp.party_id
            where nipp.party_id = any($2)
        ) m on m.news_item_id = n.id
        join countries co on co.id = m.country_id
        order by n.id, s.published_at desc nulls last
        limit $3
        "#,
        &people,
        &parties,
        limit,
    )
    .fetch_all(pool)
    .await?;
    for r in news {
        items.push(FeedItem {
            kind: "news".into(),
            title: r.headline,
            href: format!("/{}/news/{}", r.country_slug, r.id),
            occurred_at: r.published_at,
            meta: None,
        });
    }

    // Elections in a followed country.
    if !countries.is_empty() {
        let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default();
        let elections = sqlx::query!(
            r#"
            select e.name, e.slug, co.slug as "country_slug!", e.held_on
            from elections e join countries co on co.id = e.country_id
            where e.country_id = any($1)
            order by e.held_on desc nulls last, e.id desc
            limit $2
            "#,
            &countries,
            limit,
        )
        .fetch_all(pool)
        .await?;
        for r in elections {
            items.push(FeedItem {
                kind: "election".into(),
                title: r.name,
                href: format!("/{}/election/{}", r.country_slug, r.slug),
                occurred_at: r
                    .held_on
                    .map(|d| DateTime::<Utc>::from_naive_utc_and_offset(d.and_time(midnight), Utc)),
                meta: None,
            });
        }
    }

    // Newest first across all kinds; undated items sort last (Reverse sends the
    // None key to the end). Then cap.
    items.sort_by_key(|i| std::cmp::Reverse(i.occurred_at));
    items.truncate(limit as usize);
    Ok(items)
}
