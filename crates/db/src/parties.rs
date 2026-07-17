use domain::models::{NewParty, Party};

use crate::{Pool, Result};

/// Insert or update a party, keyed on `wikidata_id`, and return its id.
pub async fn upsert_party(pool: &Pool, p: &NewParty) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into parties
            (wikidata_id, name, short_name, slug, founded_date, dissolved_date,
             ideology_tags, summary, source_id, country_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (wikidata_id) do update set
            name           = excluded.name,
            short_name     = excluded.short_name,
            slug           = excluded.slug,
            founded_date   = excluded.founded_date,
            dissolved_date = excluded.dissolved_date,
            ideology_tags  = excluded.ideology_tags,
            summary        = excluded.summary,
            country_id     = coalesce(excluded.country_id, parties.country_id),
            updated_at     = now()
        returning id
        "#,
        p.wikidata_id,
        p.name,
        p.short_name,
        p.slug,
        p.founded_date,
        p.dissolved_date,
        &p.ideology_tags,
        p.summary,
        p.source_id,
        p.country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Fetch a party by slug.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Party>> {
    let party = sqlx::query_as!(
        Party,
        r#"
        select id, wikidata_id, name, short_name, slug, founded_date, dissolved_date,
               ideology_tags, summary, color
        from parties
        where slug = $1
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(party)
}

/// A party by slug, but only if it belongs to `country_id`. The per-country
/// detail page uses this so a slug from another country reads as not-found.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Party>> {
    let party = sqlx::query_as!(
        Party,
        r#"
        select id, wikidata_id, name, short_name, slug, founded_date, dissolved_date,
               ideology_tags, summary, color
        from parties
        where slug = $1 and country_id = $2
        "#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(party)
}

/// List a country's parties alphabetically.
pub async fn list(pool: &Pool, country_id: i64) -> Result<Vec<Party>> {
    let parties = sqlx::query_as!(
        Party,
        r#"
        select id, wikidata_id, name, short_name, slug, founded_date, dissolved_date,
               ideology_tags, summary, color
        from parties
        where country_id = $1
        order by name
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(parties)
}

/// Number of parties in a country.
pub async fn count(pool: &Pool, country_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from parties where country_id = $1"#,
        country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A country's parties whose name or abbreviation contains the query, matched
/// diacritic- and case-insensitively, or all of the country's parties when the
/// query is blank. Used by the searchable list page.
pub async fn list_filtered(pool: &Pool, country_id: i64, query: &str) -> Result<Vec<Party>> {
    let q = query.trim();
    if q.is_empty() {
        return list(pool, country_id).await;
    }
    let pattern = format!("%{q}%");
    let parties = sqlx::query_as!(
        Party,
        r#"
        select id, wikidata_id, name, short_name, slug, founded_date, dissolved_date,
               ideology_tags, summary, color
        from parties
        where country_id = $1
          and (unaccent(name) ilike unaccent($2)
               or unaccent(coalesce(short_name, '')) ilike unaccent($2))
        order by name
        "#,
        country_id,
        pattern,
    )
    .fetch_all(pool)
    .await?;
    Ok(parties)
}

/// Members of a party, newest membership first.
pub async fn members(pool: &Pool, party_id: i64) -> Result<Vec<PartyMember>> {
    let rows = sqlx::query!(
        r#"
        select p.full_name, p.slug, m.start_date, m.end_date,
               s.id as source_id, s.url as source_url
        from party_memberships m
        join people p on p.id = m.person_id
        join sources s on s.id = m.source_id
        where m.party_id = $1
        order by m.start_date desc nulls last
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PartyMember {
            person_name: r.full_name,
            person_slug: r.slug,
            start_date: r.start_date,
            end_date: r.end_date,
            source_id: r.source_id,
            source_url: r.source_url,
        })
        .collect())
}

/// A person who is or was a member of a party, with dates.
#[derive(Debug, Clone)]
pub struct PartyMember {
    pub person_name: String,
    pub person_slug: String,
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
    pub source_id: i64,
    pub source_url: String,
}

/// The current leader of a party.
#[derive(Debug, Clone)]
pub struct PartyLeader {
    pub person_name: String,
    pub person_slug: String,
    pub title: Option<String>,
    pub source_id: i64,
    pub source_url: String,
}

/// The party's current leader: someone holding a `party_leader` role who is
/// also a current member of the party. `None` when no leader is recorded.
pub async fn leader(pool: &Pool, party_id: i64) -> Result<Option<PartyLeader>> {
    let row = sqlx::query!(
        r#"
        select p.full_name, p.slug, r.title, s.id as source_id, s.url as source_url
        from roles r
        join people p on p.id = r.person_id
        join party_memberships m
          on m.person_id = p.id and m.party_id = $1 and m.end_date is null
        join sources s on s.id = r.source_id
        where r.role_type = 'party_leader' and r.end_date is null
        order by r.start_date desc nulls last
        limit 1
        "#,
        party_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| PartyLeader {
        person_name: r.full_name,
        person_slug: r.slug,
        title: r.title,
        source_id: r.source_id,
        source_url: r.source_url,
    }))
}

/// How many of the party's current members currently hold an MP seat.
pub async fn parliament_seats(pool: &Pool, party_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "count!"
        from party_memberships m
        where m.party_id = $1 and m.end_date is null
          and exists (
            select 1 from roles r
            where r.person_id = m.person_id and r.role_type = 'mp' and r.end_date is null
          )
        "#,
        party_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// One member party of an alliance the party belongs to, flattened for grouping
/// in the handler. Includes the party itself so the alliance roster is complete.
#[derive(Debug, Clone)]
pub struct AllianceMemberRow {
    pub alliance_name: String,
    pub alliance_slug: String,
    pub party_id: i64,
    pub name: String,
    pub short_name: Option<String>,
    pub slug: String,
    pub color: Option<String>,
}

/// The alliances a party currently belongs to, each expanded to all of its
/// member parties. Empty when the party is unaligned.
pub async fn alliance_members(pool: &Pool, party_id: i64) -> Result<Vec<AllianceMemberRow>> {
    let rows = sqlx::query_as!(
        AllianceMemberRow,
        r#"
        select a.name as alliance_name, a.slug as alliance_slug,
               mp.id as party_id, mp.name, mp.short_name, mp.slug, mp.color
        from party_alliances self_pa
        join alliances a on a.id = self_pa.alliance_id
        join party_alliances pa on pa.alliance_id = a.id
        join parties mp on mp.id = pa.party_id
        where self_pa.party_id = $1 and self_pa.end_date is null
        order by a.name, mp.short_name
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
