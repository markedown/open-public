//! News items and their links to people and parties.
//!
//! A news item stores a headline, a short summary in our own words, and a
//! reference to the source article (URL, outlet, date). The full article body
//! is never stored. Creating one is a single transaction: insert the source,
//! the news item, and the entity links together.

use chrono::{DateTime, Utc};
use domain::models::NewsItem;

use crate::{Pool, Result};

/// Fields for creating a news item and linking it to people and/or parties.
pub struct NewNews<'a> {
    pub url: &'a str,
    pub outlet: Option<&'a str>,
    pub published_at: Option<DateTime<Utc>>,
    pub headline: &'a str,
    pub our_summary: Option<&'a str>,
    pub person_ids: &'a [i64],
    pub party_ids: &'a [i64],
}

/// Fields for the idempotent admin-API news upsert. The summary lands as a
/// draft, never published directly: the API speeds ingest, it does not bypass
/// review. Entity links are set separately, by slug.
pub struct ApiNews<'a> {
    pub url: &'a str,
    pub outlet: Option<&'a str>,
    pub published_at: Option<DateTime<Utc>>,
    pub content_hash: Option<&'a str>,
    pub headline: &'a str,
    pub summary_draft: Option<&'a str>,
}

/// The result of an idempotent upsert: the row id and whether it was created.
pub struct Upserted {
    pub id: i64,
    pub created: bool,
}

/// Idempotently upsert a news item by its article URL (which is its source), so
/// a re-delivered article updates in place rather than duplicating. The summary
/// is stored as a draft. Returns the news id and whether it was newly created.
pub async fn upsert(pool: &Pool, n: &ApiNews<'_>) -> Result<Upserted> {
    let mut tx = pool.begin().await?;

    // Find or create the article's source, keyed on url (a missing hash is the
    // same document), so the same URL never yields two sources.
    let existing = sqlx::query_scalar!(
        "select id from sources where url = $1 and content_hash is not distinct from $2",
        n.url,
        n.content_hash,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let source_id = match existing {
        Some(id) => {
            sqlx::query!(
                "update sources set outlet = $2, published_at = $3, fetched_at = now() where id = $1",
                id,
                n.outlet,
                n.published_at,
            )
            .execute(&mut *tx)
            .await?;
            id
        }
        None => {
            sqlx::query_scalar!(
                "insert into sources (kind, url, outlet, published_at, content_hash, fetched_at) \
                 values ('news_rss', $1, $2, $3, $4, now()) returning id",
                n.url,
                n.outlet,
                n.published_at,
                n.content_hash,
            )
            .fetch_one(&mut *tx)
            .await?
        }
    };

    let existed = sqlx::query_scalar!(
        r#"select exists(select 1 from news_items where source_id = $1) as "e!""#,
        source_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    // `source_id` is unique on news_items, so this upserts the article in place.
    // A new draft overwrites an old draft, but never clears a published summary.
    let id = sqlx::query_scalar!(
        "insert into news_items (source_id, headline, summary_draft) values ($1, $2, $3) \
         on conflict (source_id) do update set \
             headline = excluded.headline, \
             summary_draft = coalesce(excluded.summary_draft, news_items.summary_draft) \
         returning id",
        source_id,
        n.headline,
        n.summary_draft,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Upserted {
        id,
        created: !existed,
    })
}

/// Insert a news item with its source and entity links. Returns the news id.
pub async fn create(pool: &Pool, n: &NewNews<'_>) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let source_id: i64 = sqlx::query_scalar!(
        r#"
        insert into sources (kind, url, outlet, published_at, fetched_at)
        values ('news_rss', $1, $2, $3, now())
        returning id
        "#,
        n.url,
        n.outlet,
        n.published_at,
    )
    .fetch_one(&mut *tx)
    .await?;

    let news_id: i64 = sqlx::query_scalar!(
        r#"
        insert into news_items (source_id, headline, our_summary)
        values ($1, $2, $3)
        returning id
        "#,
        source_id,
        n.headline,
        n.our_summary,
    )
    .fetch_one(&mut *tx)
    .await?;

    for &person_id in n.person_ids {
        sqlx::query!(
            "insert into news_item_people (news_item_id, person_id) values ($1, $2) on conflict do nothing",
            news_id,
            person_id,
        )
        .execute(&mut *tx)
        .await?;
    }
    for &party_id in n.party_ids {
        sqlx::query!(
            "insert into news_item_parties (news_item_id, party_id) values ($1, $2) on conflict do nothing",
            news_id,
            party_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(news_id)
}

/// A person or party mentioned by a news item, for the entity chips on the
/// country-wide news index.
#[derive(Debug, Clone)]
pub struct PersonRef {
    pub slug: String,
    pub name: String,
}

/// A party mentioned by a news item, carried with the fields its chip needs.
#[derive(Debug, Clone)]
pub struct PartyRef {
    pub slug: String,
    pub short: String,
    pub color: Option<String>,
}

/// A news item for the country-wide index: the item itself plus the people and
/// parties it is linked to, so each entry can show who it is about.
#[derive(Debug, Clone)]
pub struct NewsCard {
    pub id: i64,
    pub headline: String,
    pub our_summary: Option<String>,
    pub url: String,
    pub outlet: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub people: Vec<PersonRef>,
    pub parties: Vec<PartyRef>,
}

/// The most recent news items for a country, newest first, each with its linked
/// people and parties. A news item belongs to a country when at least one of the
/// people or parties it links is in that country; items linked to no entity in
/// the country are excluded. Headlines are shown in `lang` where a published
/// translation exists, else in the original. `limit` caps the list.
pub async fn recent(pool: &Pool, country_id: i64, lang: &str, limit: i64) -> Result<Vec<NewsCard>> {
    // Linked entities are aggregated as tab-joined strings (slug, then display
    // fields) so each name stays paired with its slug; a plain two-array
    // aggregate could reorder the columns independently and mismatch them.
    let rows = sqlx::query!(
        r#"
        select n.id,
          coalesce(htr.text, n.headline) as "headline!",
          coalesce(str.text, n.our_summary) as our_summary,
          s.url as "url!", s.outlet, s.published_at,
          coalesce(
            (select array_agg(p.slug || E'\t' || p.full_name order by p.full_name collate "name_sort")
             from news_item_people nip join people p on p.id = nip.person_id
             where nip.news_item_id = n.id),
            '{}'::text[]
          ) as "people!: Vec<String>",
          coalesce(
            (select array_agg(
               pt.slug || E'\t' || coalesce(pt.short_name, pt.name) || E'\t' || coalesce(pt.color, '')
               order by pt.name collate "name_sort")
             from news_item_parties nap join parties pt on pt.id = nap.party_id
             where nap.news_item_id = n.id),
            '{}'::text[]
          ) as "parties!: Vec<String>"
        from news_items n
        join sources s on s.id = n.source_id
        left join translations htr on htr.entity_type = 'news_item' and htr.entity_id = n.id
            and htr.field = 'headline' and htr.lang = $2 and htr.status = 'published'
        -- Our own summary in the reader's language too: a translated headline
        -- above an untranslated summary is worse than neither.
        left join translations str on str.entity_type = 'news_item' and str.entity_id = n.id
            and str.field = 'our_summary' and str.lang = $2 and str.status = 'published'
        where exists (
              select 1 from news_item_people x
              join people pe on pe.id = x.person_id
              where x.news_item_id = n.id and pe.country_id = $1)
           or exists (
              select 1 from news_item_parties y
              join parties pa on pa.id = y.party_id
              where y.news_item_id = n.id and pa.country_id = $1)
        order by s.published_at desc nulls last, n.id desc
        limit $3
        "#,
        country_id,
        lang,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let cards = rows
        .into_iter()
        .map(|r| NewsCard {
            id: r.id,
            headline: r.headline,
            our_summary: r.our_summary,
            url: r.url,
            outlet: r.outlet,
            published_at: r.published_at,
            people: parse_people(r.people),
            parties: parse_parties(r.parties),
        })
        .collect();
    Ok(cards)
}

/// Split the tab-joined `slug\tname` people aggregate into typed refs.
fn parse_people(rows: Vec<String>) -> Vec<PersonRef> {
    rows.into_iter()
        .filter_map(|s| {
            let (slug, name) = s.split_once('\t')?;
            Some(PersonRef {
                slug: slug.to_string(),
                name: name.to_string(),
            })
        })
        .collect()
}

/// Split the tab-joined `slug\tshort\tcolor` parties aggregate into typed refs.
fn parse_parties(rows: Vec<String>) -> Vec<PartyRef> {
    rows.into_iter()
        .filter_map(|s| {
            let mut it = s.splitn(3, '\t');
            let slug = it.next()?.to_string();
            let short = it.next()?.to_string();
            let color = it.next().filter(|c| !c.is_empty()).map(str::to_string);
            Some(PartyRef { slug, short, color })
        })
        .collect()
}

/// One page of the news published by an outlet, newest first, each with its
/// linked people and parties. `limit`/`offset` drive pagination.
pub async fn for_outlet(
    pool: &Pool,
    outlet_id: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<NewsCard>> {
    let rows = sqlx::query!(
        r#"
        select n.id, n.headline, n.our_summary, s.url as "url!", s.outlet, s.published_at,
          coalesce(
            (select array_agg(p.slug || E'\t' || p.full_name order by p.full_name collate "name_sort")
             from news_item_people nip join people p on p.id = nip.person_id
             where nip.news_item_id = n.id),
            '{}'::text[]
          ) as "people!: Vec<String>",
          coalesce(
            (select array_agg(
               pt.slug || E'\t' || coalesce(pt.short_name, pt.name) || E'\t' || coalesce(pt.color, '')
               order by pt.name collate "name_sort")
             from news_item_parties nap join parties pt on pt.id = nap.party_id
             where nap.news_item_id = n.id),
            '{}'::text[]
          ) as "parties!: Vec<String>"
        from news_items n
        join sources s on s.id = n.source_id
        where s.outlet_id = $1
        order by s.published_at desc nulls last, n.id desc
        limit $2 offset $3
        "#,
        outlet_id,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    let cards = rows
        .into_iter()
        .map(|r| NewsCard {
            id: r.id,
            headline: r.headline,
            our_summary: r.our_summary,
            url: r.url,
            outlet: r.outlet,
            published_at: r.published_at,
            people: parse_people(r.people),
            parties: parse_parties(r.parties),
        })
        .collect();
    Ok(cards)
}

/// How many articles we hold from an outlet, for pagination.
pub async fn count_for_outlet(pool: &Pool, outlet_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "count!"
        from news_items n join sources s on s.id = n.source_id
        where s.outlet_id = $1
        "#,
        outlet_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A news item awaiting a summary: no published summary and no draft yet. The
/// summarizer pulls these, so it never re-summarizes an item already done or
/// already drafted. `limit` caps a batch.
#[derive(Debug, Clone)]
pub struct Unsummarized {
    pub id: i64,
    pub headline: String,
    pub url: String,
}

/// Items with neither a published summary nor a pending draft, newest first.
pub async fn unsummarized(pool: &Pool, limit: i64) -> Result<Vec<Unsummarized>> {
    let rows = sqlx::query_as!(
        Unsummarized,
        r#"
        select n.id, n.headline, s.url as "url!"
        from news_items n
        join sources s on s.id = n.source_id
        where n.our_summary is null and n.summary_draft is null
        order by s.published_at desc nulls last, n.id desc
        limit $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Store a proposed summary as a draft awaiting review. Overwrites any existing
/// draft for the item.
pub async fn set_draft(pool: &Pool, news_id: i64, draft: &str) -> Result<()> {
    sqlx::query!(
        "update news_items set summary_draft = $2 where id = $1",
        news_id,
        draft,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A news item whose draft summary is waiting for an editor.
#[derive(Debug, Clone)]
pub struct PendingDraft {
    pub id: i64,
    pub headline: String,
    pub url: String,
    pub outlet: Option<String>,
    pub summary_draft: String,
}

/// All items with a pending draft, newest first, for the review queue.
pub async fn pending_drafts(pool: &Pool) -> Result<Vec<PendingDraft>> {
    let rows = sqlx::query_as!(
        PendingDraft,
        r#"
        select n.id, n.headline, s.url as "url!", s.outlet,
               n.summary_draft as "summary_draft!"
        from news_items n
        join sources s on s.id = n.source_id
        where n.summary_draft is not null
        order by s.published_at desc nulls last, n.id desc
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many drafts await review, for a queue badge.
pub async fn pending_draft_count(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from news_items where summary_draft is not null"#,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Publish a reviewed summary: set it as the item's `our_summary` (the editor
/// may have edited the draft) and clear the pending draft.
pub async fn publish_summary(pool: &Pool, news_id: i64, summary: &str) -> Result<()> {
    sqlx::query!(
        "update news_items set our_summary = $2, summary_draft = null where id = $1",
        news_id,
        summary,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Discard a pending draft without publishing anything.
pub async fn discard_draft(pool: &Pool, news_id: i64) -> Result<()> {
    sqlx::query!(
        "update news_items set summary_draft = null where id = $1",
        news_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether a news item already exists for a source with this URL. Ingestion of
/// re-served feed items uses this to stay idempotent: the same article is never
/// stored twice.
pub async fn url_exists(pool: &Pool, url: &str) -> Result<bool> {
    let exists = sqlx::query_scalar!(
        r#"
        select exists (
            select 1 from news_items n
            join sources s on s.id = n.source_id
            where s.url = $1
        ) as "exists!"
        "#,
        url,
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// News items linked to a person, newest first.
pub async fn for_person(pool: &Pool, person_id: i64) -> Result<Vec<NewsItem>> {
    let rows = sqlx::query_as!(
        NewsItem,
        r#"
        select n.id, n.headline, n.our_summary,
               s.url as "url!", s.outlet, s.published_at
        from news_items n
        join news_item_people nip on nip.news_item_id = n.id
        join sources s on s.id = n.source_id
        where nip.person_id = $1
        order by s.published_at desc nulls last, n.id desc
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The outlet that published a news item, for the detail page header.
#[derive(Debug, Clone)]
pub struct OutletBrief {
    pub name: String,
    pub slug: String,
    pub logo_url: Option<String>,
}

/// A single news item's full detail: fields, its outlet, and the people and
/// parties it mentions.
#[derive(Debug, Clone)]
pub struct NewsDetail {
    pub id: i64,
    pub headline: String,
    pub our_summary: Option<String>,
    pub author: Option<String>,
    pub url: String,
    pub published_at: Option<DateTime<Utc>>,
    pub outlet: Option<OutletBrief>,
    pub people: Vec<PersonRef>,
    pub parties: Vec<PartyRef>,
}

/// One news item by id, with its outlet and linked entities, for its own page.
pub async fn get_detail(pool: &Pool, id: i64) -> Result<Option<NewsDetail>> {
    let row = sqlx::query!(
        r#"
        select n.headline, n.our_summary, n.author, s.url as "url!", s.published_at,
               o.name as "outlet_name?", o.slug as "outlet_slug?", o.logo_url as "outlet_logo?",
          coalesce(
            (select array_agg(p.slug || E'\t' || p.full_name order by p.full_name collate "name_sort")
             from news_item_people nip join people p on p.id = nip.person_id
             where nip.news_item_id = n.id),
            '{}'::text[]
          ) as "people!: Vec<String>",
          coalesce(
            (select array_agg(
               pt.slug || E'\t' || coalesce(pt.short_name, pt.name) || E'\t' || coalesce(pt.color, '')
               order by pt.name collate "name_sort")
             from news_item_parties nap join parties pt on pt.id = nap.party_id
             where nap.news_item_id = n.id),
            '{}'::text[]
          ) as "parties!: Vec<String>"
        from news_items n
        join sources s on s.id = n.source_id
        left join outlets o on o.id = s.outlet_id
        where n.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    let outlet = match (r.outlet_name, r.outlet_slug) {
        (Some(name), Some(slug)) => Some(OutletBrief {
            name,
            slug,
            logo_url: r.outlet_logo,
        }),
        _ => None,
    };
    Ok(Some(NewsDetail {
        id,
        headline: r.headline,
        our_summary: r.our_summary,
        author: r.author,
        url: r.url,
        published_at: r.published_at,
        outlet,
        people: parse_people(r.people),
        parties: parse_parties(r.parties),
    }))
}

/// A person or party currently linked to a news item, for the edit form.
#[derive(Debug, Clone)]
pub struct LinkedEntity {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

/// A news item's editable fields and its current entity links.
#[derive(Debug, Clone)]
pub struct NewsEdit {
    pub id: i64,
    pub headline: String,
    pub our_summary: Option<String>,
    pub author: Option<String>,
    pub url: String,
    pub outlet: Option<String>,
    pub people: Vec<LinkedEntity>,
    pub parties: Vec<LinkedEntity>,
}

/// The editable view of a news item, with its linked people and parties.
pub async fn get_edit(pool: &Pool, id: i64) -> Result<Option<NewsEdit>> {
    let head = sqlx::query!(
        r#"
        select n.headline, n.our_summary, n.author, s.url as "url!", s.outlet
        from news_items n join sources s on s.id = n.source_id
        where n.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    let Some(h) = head else {
        return Ok(None);
    };
    let people = sqlx::query_as!(
        LinkedEntity,
        r#"
        select p.id as "id!", p.full_name as "name!", p.slug as "slug!"
        from news_item_people nip join people p on p.id = nip.person_id
        where nip.news_item_id = $1 order by p.full_name collate "name_sort"
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;
    let parties = sqlx::query_as!(
        LinkedEntity,
        r#"
        select pt.id as "id!", pt.name as "name!", pt.slug as "slug!"
        from news_item_parties nap join parties pt on pt.id = nap.party_id
        where nap.news_item_id = $1 order by pt.name collate "name_sort"
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;
    Ok(Some(NewsEdit {
        id,
        headline: h.headline,
        our_summary: h.our_summary,
        author: h.author,
        url: h.url,
        outlet: h.outlet,
        people,
        parties,
    }))
}

/// Update a news item's headline, summary and author.
pub async fn update_fields(
    pool: &Pool,
    id: i64,
    headline: &str,
    summary: Option<&str>,
    author: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        "update news_items set headline = $2, our_summary = $3, author = $4 where id = $1",
        id,
        headline,
        summary,
        author,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Link a news item to a person by slug. Returns whether a new link was made
/// (false if the slug is unknown or the link already existed).
pub async fn link_person(pool: &Pool, news_id: i64, person_slug: &str) -> Result<bool> {
    let affected = sqlx::query!(
        r#"
        insert into news_item_people (news_item_id, person_id)
        select $1, id from people where slug = $2
        on conflict do nothing
        "#,
        news_id,
        person_slug,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// Link a news item to a party by slug. Returns whether a new link was made.
pub async fn link_party(pool: &Pool, news_id: i64, party_slug: &str) -> Result<bool> {
    let affected = sqlx::query!(
        r#"
        insert into news_item_parties (news_item_id, party_id)
        select $1, id from parties where slug = $2
        on conflict do nothing
        "#,
        news_id,
        party_slug,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

/// Remove a person link from a news item.
pub async fn unlink_person(pool: &Pool, news_id: i64, person_id: i64) -> Result<()> {
    sqlx::query!(
        "delete from news_item_people where news_item_id = $1 and person_id = $2",
        news_id,
        person_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a party link from a news item.
pub async fn unlink_party(pool: &Pool, news_id: i64, party_id: i64) -> Result<()> {
    sqlx::query!(
        "delete from news_item_parties where news_item_id = $1 and party_id = $2",
        news_id,
        party_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// News items linked to a party, newest first.
pub async fn for_party(pool: &Pool, party_id: i64) -> Result<Vec<NewsItem>> {
    let rows = sqlx::query_as!(
        NewsItem,
        r#"
        select n.id, n.headline, n.our_summary,
               s.url as "url!", s.outlet, s.published_at
        from news_items n
        join news_item_parties nip on nip.news_item_id = n.id
        join sources s on s.id = n.source_id
        where nip.party_id = $1
        order by s.published_at desc nulls last, n.id desc
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Set (or replace) a news item's similarity embedding. The vector is bound as a
/// pgvector text literal and cast in SQL, so no vector-typed Rust binding, and no
/// extra dependency, is needed. `embedding` must have the column's dimension.
pub async fn set_embedding(pool: &Pool, news_id: i64, embedding: &[f32]) -> Result<()> {
    use std::fmt::Write;
    let mut lit = String::with_capacity(embedding.len() * 8 + 2);
    lit.push('[');
    for (i, x) in embedding.iter().enumerate() {
        if i > 0 {
            lit.push(',');
        }
        let _ = write!(lit, "{x}");
    }
    lit.push(']');
    sqlx::query("update news_items set embedding = $1::vector where id = $2")
        .bind(lit)
        .bind(news_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A news item still needing an embedding: its id and the text to embed (the
/// headline plus whatever summary exists). For the pipeline's embedding pass.
pub struct ToEmbed {
    pub id: i64,
    pub headline: String,
    pub summary: Option<String>,
}

/// News items with no embedding yet, oldest first, capped at `limit`.
pub async fn needs_embedding(pool: &Pool, limit: i64) -> Result<Vec<ToEmbed>> {
    let rows = sqlx::query_as!(
        ToEmbed,
        r#"select id, headline, coalesce(our_summary, summary_draft) as summary
           from news_items where embedding is null order by id limit $1"#,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Link two news items as related (the same story), stored once in canonical
/// order so a pair never appears twice. `method` is how they were matched, and
/// `similarity` the cosine score when the match came from embeddings.
pub async fn link_related(
    pool: &Pool,
    x: i64,
    y: i64,
    method: &str,
    similarity: Option<f32>,
) -> Result<()> {
    if x == y {
        return Ok(());
    }
    let (a, b) = if x < y { (x, y) } else { (y, x) };
    sqlx::query!(
        "insert into news_related (a_id, b_id, method, similarity) values ($1, $2, $3, $4) \
         on conflict (a_id, b_id) \
         do update set method = excluded.method, similarity = excluded.similarity",
        a,
        b,
        method,
        similarity,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The ids of news items related to this one, from either direction of a pair.
pub async fn related_ids(pool: &Pool, news_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query_scalar!(
        r#"select case when a_id = $1 then b_id else a_id end as "other!"
           from news_related where a_id = $1 or b_id = $1 order by 1"#,
        news_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn a_news(pool: &Pool, url: &str) -> i64 {
        upsert(
            pool,
            &ApiNews {
                url,
                outlet: None,
                published_at: None,
                content_hash: None,
                headline: "H",
                summary_draft: Some("s"),
            },
        )
        .await
        .unwrap()
        .id
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn embeddings_and_related_links(pool: Pool) {
        let n1 = a_news(&pool, "https://o.test/1").await;
        let n2 = a_news(&pool, "https://o.test/2").await;

        // Both need an embedding at first.
        assert_eq!(needs_embedding(&pool, 10).await.unwrap().len(), 2);
        set_embedding(&pool, n1, &vec![0.1_f32; 1024])
            .await
            .unwrap();
        let pending = needs_embedding(&pool, 10).await.unwrap();
        assert_eq!(pending.len(), 1, "the embedded item drops out");
        assert_eq!(pending[0].id, n2);

        // Linking is symmetric and stored once, in canonical order.
        link_related(&pool, n2, n1, "embedding", Some(0.9))
            .await
            .unwrap();
        link_related(&pool, n1, n2, "verified", None).await.unwrap(); // same pair, upsert
        assert_eq!(related_ids(&pool, n1).await.unwrap(), vec![n2]);
        assert_eq!(related_ids(&pool, n2).await.unwrap(), vec![n1]);
        let count: i64 = sqlx::query_scalar("select count(*) from news_related")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "one row per pair");

        // A self-link is a no-op.
        link_related(&pool, n1, n1, "embedding", None)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("select count(*) from news_related")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
