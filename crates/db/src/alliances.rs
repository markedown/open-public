//! Alliances (coalitions) and their member parties.

use chrono::NaiveDate;

use crate::{Pool, Result};

/// An alliance (coalition) with its own-words summary and lifespan.
#[derive(Debug, Clone)]
pub struct Alliance {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub summary: Option<String>,
    pub founded_date: Option<NaiveDate>,
    pub dissolved_date: Option<NaiveDate>,
}

/// One alliance on the index, with its current member count.
#[derive(Debug, Clone)]
pub struct AllianceCard {
    pub name: String,
    pub slug: String,
    pub founded_date: Option<NaiveDate>,
    pub dissolved_date: Option<NaiveDate>,
    pub member_count: i64,
}

/// One party in an alliance, for the alliance page's roster.
#[derive(Debug, Clone)]
pub struct AllianceMember {
    pub party_name: String,
    pub party_short_name: Option<String>,
    pub party_slug: String,
    pub party_color: Option<String>,
}

/// Fetch an alliance by slug.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Alliance>> {
    let row = sqlx::query_as!(
        Alliance,
        r#"
        select id, name, slug, summary, founded_date, dissolved_date
        from alliances where slug = $1
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// An alliance by slug, but only if it belongs to `country_id`. The per-country
/// detail page uses this so a slug from another country reads as not-found.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Alliance>> {
    let row = sqlx::query_as!(
        Alliance,
        r#"
        select id, name, slug, summary, founded_date, dissolved_date
        from alliances where slug = $1 and country_id = $2
        "#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// A country's alliances, active (not dissolved) first, then by founding date,
/// each with its current member count. Names show in `lang` where a published
/// translation exists, else in the original.
pub async fn list_for_country(
    pool: &Pool,
    country_id: i64,
    lang: &str,
) -> Result<Vec<AllianceCard>> {
    let rows = sqlx::query_as!(
        AllianceCard,
        r#"
        select coalesce(ntr.text, a.name) as "name!", a.slug, a.founded_date, a.dissolved_date,
               (select count(*) from party_alliances pa
                where pa.alliance_id = a.id and pa.end_date is null) as "member_count!"
        from alliances a
        left join translations ntr on ntr.entity_type = 'alliance' and ntr.entity_id = a.id
            and ntr.field = 'name' and ntr.lang = $2 and ntr.status = 'published'
        where a.country_id = $1
        order by a.dissolved_date is not null, a.founded_date desc nulls last, a.name
        "#,
        country_id,
        lang,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The parties currently in an alliance, ordered by name.
pub async fn members(pool: &Pool, alliance_id: i64) -> Result<Vec<AllianceMember>> {
    let rows = sqlx::query_as!(
        AllianceMember,
        r#"
        select p.name as party_name, p.short_name as party_short_name,
               p.slug as party_slug, p.color as party_color
        from party_alliances pa
        join parties p on p.id = pa.party_id
        where pa.alliance_id = $1 and pa.end_date is null
        order by p.name
        "#,
        alliance_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
