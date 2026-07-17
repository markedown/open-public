//! News outlets: the organizations that publish the articles we save. An outlet
//! carries a logo, a homepage, a neutral summary and a political-leaning
//! assessment (a five-point spectrum, sourced like every other fact), and it
//! owns a page listing the articles we have from it.

use crate::{Pool, Result};

/// An outlet with the fields its own page needs, plus how many articles we hold
/// from it. The outlet's own website is its reference, and its leaning is our
/// assessment, so no separate source citation is carried.
#[derive(Debug, Clone)]
pub struct Outlet {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub homepage_url: Option<String>,
    pub logo_url: Option<String>,
    pub logo_license: Option<String>,
    pub leaning: Option<String>,
    pub summary: Option<String>,
    pub article_count: i64,
}

/// A compact outlet for the index: name, logo, leaning and article count.
#[derive(Debug, Clone)]
pub struct OutletCard {
    pub name: String,
    pub slug: String,
    pub logo_url: Option<String>,
    pub leaning: Option<String>,
    pub article_count: i64,
}

/// Fields for creating or updating an outlet.
pub struct NewOutlet<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub homepage_url: Option<&'a str>,
    pub logo_url: Option<&'a str>,
    pub logo_license: Option<&'a str>,
    pub leaning: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub country_id: Option<i64>,
}

/// The five-point leaning spectrum, ordered left to right. UI and validation
/// share this list so a stored value never renders as an unknown position.
pub const LEANINGS: [&str; 5] = ["left", "lean_left", "center", "lean_right", "right"];

/// Insert an outlet, or update it in place if the slug already exists. Returns
/// the id, so ingestion can backfill and re-run idempotently.
pub async fn upsert(pool: &Pool, o: &NewOutlet<'_>) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into outlets
            (name, slug, homepage_url, logo_url, logo_license, leaning, summary, country_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        on conflict (slug) do update set
            name = excluded.name,
            homepage_url = excluded.homepage_url,
            logo_url = excluded.logo_url,
            logo_license = excluded.logo_license,
            leaning = excluded.leaning,
            summary = excluded.summary,
            country_id = coalesce(excluded.country_id, outlets.country_id),
            updated_at = now()
        returning id
        "#,
        o.name,
        o.slug,
        o.homepage_url,
        o.logo_url,
        o.logo_license,
        o.leaning,
        o.summary,
        o.country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Point every source with this outlet label at the outlet row, so its articles
/// list on the outlet page. Used by the backfill from the old free-text column.
pub async fn link_sources_by_label(pool: &Pool, outlet_id: i64, label: &str) -> Result<u64> {
    let affected = sqlx::query!(
        "update sources set outlet_id = $1 where outlet = $2 and outlet_id is null",
        outlet_id,
        label,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}

/// One outlet by slug, with its article count.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Outlet>> {
    let row = sqlx::query_as!(
        Outlet,
        r#"
        select o.id as "id!", o.name as "name!", o.slug as "slug!",
               o.homepage_url, o.logo_url, o.logo_license, o.leaning, o.summary,
               (select count(*) from news_items n
                join sources s on s.id = n.source_id
                where s.outlet_id = o.id) as "article_count!"
        from outlets o
        where o.slug = $1
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// One outlet by slug, but only if it belongs to `country_id`. The per-country
/// detail page uses this so a slug from another country reads as not-found.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Outlet>> {
    let row = sqlx::query_as!(
        Outlet,
        r#"
        select o.id as "id!", o.name as "name!", o.slug as "slug!",
               o.homepage_url, o.logo_url, o.logo_license, o.leaning, o.summary,
               (select count(*) from news_items n
                join sources s on s.id = n.source_id
                where s.outlet_id = o.id) as "article_count!"
        from outlets o
        where o.slug = $1 and o.country_id = $2
        "#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// A country's outlets for the index, ordered by name, each with its article
/// count.
pub async fn list(pool: &Pool, country_id: i64) -> Result<Vec<OutletCard>> {
    let rows = sqlx::query_as!(
        OutletCard,
        r#"
        select o.name, o.slug, o.logo_url, o.leaning,
               (select count(*) from news_items n
                join sources s on s.id = n.source_id
                where s.outlet_id = o.id) as "article_count!"
        from outlets o
        where o.country_id = $1
        order by o.name
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
