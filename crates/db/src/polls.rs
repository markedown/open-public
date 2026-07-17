use domain::models::{Poll, PollOption};

use crate::{Pool, Result};

/// Fetch a poll by slug together with its options and current vote tallies.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Poll>> {
    let Some(poll) = sqlx::query!(
        r#"select id, question, slug, kind, media_url, media_license from polls where slug = $1"#,
        slug,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Poll {
        id: poll.id,
        question: poll.question,
        slug: poll.slug,
        kind: poll.kind,
        media_url: poll.media_url,
        media_license: poll.media_license,
        options: options_for(pool, poll.id).await?,
    }))
}

/// A poll by slug, but only if it belongs to `country_id` (attached directly, or
/// through its party or person). The per-country detail page uses this so a slug
/// from another country reads as not-found rather than rendering under the wrong
/// country's breadcrumb.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Poll>> {
    let Some(poll) = sqlx::query!(
        r#"
        select p.id, p.question, p.slug, p.kind, p.media_url, p.media_license
        from polls p
        where p.slug = $1 and (
              p.country_id = $2
              or exists (select 1 from parties pt where pt.id = p.party_id and pt.country_id = $2)
              or exists (select 1 from people pe where pe.id = p.person_id and pe.country_id = $2))
        "#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Poll {
        id: poll.id,
        question: poll.question,
        slug: poll.slug,
        kind: poll.kind,
        media_url: poll.media_url,
        media_license: poll.media_license,
        options: options_for(pool, poll.id).await?,
    }))
}

/// The integrity chain head for a poll: the number of votes and the current
/// chain hash. `None` when the poll has no votes yet.
#[derive(Debug, Clone)]
pub struct ChainHead {
    pub head_seq: i64,
    pub head_hash: Vec<u8>,
}

/// Fetch a poll's vote-chain head, for the integrity fingerprint and the
/// verifiable-data endpoint.
pub async fn chain_head(pool: &Pool, poll_id: i64) -> Result<Option<ChainHead>> {
    let row = sqlx::query_as!(
        ChainHead,
        r#"
        select head_seq, head_hash
        from poll_chains
        where poll_id = $1 and head_seq > 0
        "#,
        poll_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Whether a user has already voted in a poll.
pub async fn has_voted(pool: &Pool, poll_id: i64, user_id: i64) -> Result<bool> {
    let voted = sqlx::query_scalar!(
        r#"select exists(select 1 from poll_votes where poll_id = $1 and user_id = $2) as "voted!""#,
        poll_id,
        user_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(voted)
}

/// Record a single-choice vote. The first vote per user per poll wins; later
/// attempts are ignored (votes are never updated). The chain trigger enforces
/// the one-vote rule for non-multi polls; the unique constraint is the backstop.
/// Returns whether a vote was recorded.
pub async fn cast_vote(pool: &Pool, poll_id: i64, option_id: i64, user_id: i64) -> Result<bool> {
    let rows = sqlx::query!(
        r#"
        insert into poll_votes (poll_id, option_id, user_id)
        select $1, $2, $3
        where exists (select 1 from poll_options where id = $2 and poll_id = $1)
        on conflict (poll_id, user_id, option_id) do nothing
        "#,
        poll_id,
        option_id,
        user_id,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(rows == 1)
}

/// Record a multi-select vote: one row per chosen option, in a single
/// transaction so a voter's selections chain together and share one voter
/// index. Options not belonging to the poll, and repeats of an option the user
/// already chose, are ignored. Returns how many new option-votes were recorded.
pub async fn cast_votes(
    pool: &Pool,
    poll_id: i64,
    option_ids: &[i64],
    user_id: i64,
) -> Result<usize> {
    let mut tx = pool.begin().await?;
    let mut recorded = 0usize;
    for &option_id in option_ids {
        let rows = sqlx::query!(
            r#"
            insert into poll_votes (poll_id, option_id, user_id)
            select $1, $2, $3
            where exists (select 1 from poll_options where id = $2 and poll_id = $1)
            on conflict (poll_id, user_id, option_id) do nothing
            "#,
            poll_id,
            option_id,
            user_id,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        recorded += rows as usize;
    }
    tx.commit().await?;
    Ok(recorded)
}

/// One option to create: a label and an optional image.
pub struct NewOption<'a> {
    pub label: &'a str,
    pub media_url: Option<&'a str>,
}

/// Fields for creating a poll and its options.
pub struct NewPoll<'a> {
    pub question: &'a str,
    pub slug: &'a str,
    pub kind: &'a str,
    /// An optional image for the question, and the license covering the poll's
    /// images (question and options).
    pub media_url: Option<&'a str>,
    pub media_license: Option<&'a str>,
    pub country_id: Option<i64>,
    pub person_id: Option<i64>,
    pub party_id: Option<i64>,
    pub options: &'a [NewOption<'a>],
}

/// Insert a poll with its options, positioned in order. Returns the poll id.
pub async fn create(pool: &Pool, p: &NewPoll<'_>) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let poll_id: i64 = sqlx::query_scalar!(
        r#"
        insert into polls (question, slug, kind, media_url, media_license, country_id, person_id, party_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        returning id
        "#,
        p.question,
        p.slug,
        p.kind,
        p.media_url,
        p.media_license,
        p.country_id,
        p.person_id,
        p.party_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    for (i, opt) in p.options.iter().enumerate() {
        // An option carries the poll's shared license only when it has an image.
        let license = opt.media_url.and(p.media_license);
        sqlx::query!(
            "insert into poll_options (poll_id, label, position, media_url, media_license) values ($1, $2, $3, $4, $5)",
            poll_id,
            opt.label,
            (i as i32) + 1,
            opt.media_url,
            license,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(poll_id)
}

/// Whether a poll slug is already taken (used to generate a unique one).
pub async fn slug_exists(pool: &Pool, slug: &str) -> Result<bool> {
    let exists = sqlx::query_scalar!(
        r#"select exists(select 1 from polls where slug = $1) as "e!""#,
        slug,
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// One poll on the index page: the question, kind, close time, and total votes.
pub struct PollListItem {
    pub question: String,
    pub slug: String,
    pub kind: String,
    pub closes_at: Option<chrono::DateTime<chrono::Utc>>,
    pub votes: i64,
}

/// The total number of polls, for the home overview.
pub async fn count(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(r#"select count(*) as "count!" from polls"#)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Every poll with its total vote count, newest first, for the index page.
pub async fn list_all(pool: &Pool) -> Result<Vec<PollListItem>> {
    let rows = sqlx::query_as!(
        PollListItem,
        r#"
        select p.question, p.slug, p.kind, p.closes_at, count(v.id) as "votes!"
        from polls p
        left join poll_votes v on v.poll_id = p.id
        group by p.id
        order by p.id desc
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every poll belonging to a country, with its total vote count, newest first,
/// for the per-country index. A poll belongs to a country when it is attached
/// directly (a country-level question) or through the party or person it is
/// about.
pub async fn list_for_country(pool: &Pool, country_id: i64) -> Result<Vec<PollListItem>> {
    let rows = sqlx::query_as!(
        PollListItem,
        r#"
        select p.question, p.slug, p.kind, p.closes_at, count(v.id) as "votes!"
        from polls p
        left join poll_votes v on v.poll_id = p.id
        where p.country_id = $1
           or exists (select 1 from parties pt where pt.id = p.party_id and pt.country_id = $1)
           or exists (select 1 from people pe where pe.id = p.person_id and pe.country_id = $1)
        group by p.id
        order by p.id desc
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Full polls (with options and tallies) attached directly to a country
/// (country-level questions, not tied to a single party or person), newest
/// first, so the country page can show results inline.
pub async fn full_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Poll>> {
    let heads = sqlx::query!(
        r#"
        select id, question, slug, kind, media_url, media_license from polls
        where country_id = $1 and person_id is null and party_id is null
        order by id desc
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    let mut polls = Vec::with_capacity(heads.len());
    for h in heads {
        polls.push(Poll {
            id: h.id,
            question: h.question,
            slug: h.slug,
            kind: h.kind,
            media_url: h.media_url,
            media_license: h.media_license,
            options: options_for(pool, h.id).await?,
        });
    }
    Ok(polls)
}

/// Full polls (with options and tallies) attached to a party, newest first, so
/// the party page can show results inline.
pub async fn full_for_party(pool: &Pool, party_id: i64) -> Result<Vec<Poll>> {
    let heads = sqlx::query!(
        r#"select id, question, slug, kind, media_url, media_license from polls where party_id = $1 order by id desc"#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    let mut polls = Vec::with_capacity(heads.len());
    for h in heads {
        polls.push(Poll {
            id: h.id,
            question: h.question,
            slug: h.slug,
            kind: h.kind,
            media_url: h.media_url,
            media_license: h.media_license,
            options: options_for(pool, h.id).await?,
        });
    }
    Ok(polls)
}

/// Full polls (with options and tallies) attached to a person, newest first.
pub async fn full_for_person(pool: &Pool, person_id: i64) -> Result<Vec<Poll>> {
    let heads = sqlx::query!(
        r#"select id, question, slug, kind, media_url, media_license from polls where person_id = $1 order by id desc"#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    let mut polls = Vec::with_capacity(heads.len());
    for h in heads {
        polls.push(Poll {
            id: h.id,
            question: h.question,
            slug: h.slug,
            kind: h.kind,
            media_url: h.media_url,
            media_license: h.media_license,
            options: options_for(pool, h.id).await?,
        });
    }
    Ok(polls)
}

/// A poll's options with their current vote tallies, ordered by position.
async fn options_for(pool: &Pool, poll_id: i64) -> Result<Vec<PollOption>> {
    let rows = sqlx::query_as!(
        PollOption,
        r#"
        select o.id, o.label, o.position, o.media_url, count(v.id) as "votes!"
        from poll_options o
        left join poll_votes v on v.option_id = o.id
        where o.poll_id = $1
        group by o.id
        order by o.position
        "#,
        poll_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
