//! Political events: a sourced, point-in-time timeline scoped to a country,
//! party, or person.

use chrono::NaiveDate;

use crate::{Pool, Result};

/// One timeline event, joined to its source for display.
#[derive(Debug, Clone)]
pub struct Event {
    pub kind: String,
    pub title: String,
    pub happened_on: Option<NaiveDate>,
    pub source_url: String,
}

/// Fields for creating an event. At least one scope must be set; the database
/// enforces this with a check constraint.
pub struct NewEvent<'a> {
    pub country_id: Option<i64>,
    pub party_id: Option<i64>,
    pub person_id: Option<i64>,
    pub kind: &'a str,
    pub title: &'a str,
    pub happened_on: Option<NaiveDate>,
    pub source_id: i64,
}

/// Insert an event. Returns its id.
pub async fn create(pool: &Pool, e: &NewEvent<'_>) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into events (country_id, party_id, person_id, kind, title, happened_on, source_id)
        values ($1, $2, $3, $4, $5, $6, $7)
        returning id
        "#,
        e.country_id,
        e.party_id,
        e.person_id,
        e.kind,
        e.title,
        e.happened_on,
        e.source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// A country's events, most recent first.
pub async fn for_country(pool: &Pool, country_id: i64) -> Result<Vec<Event>> {
    let rows = sqlx::query_as!(
        Event,
        r#"
        select e.kind, e.title, e.happened_on, s.url as source_url
        from events e
        join sources s on s.id = e.source_id
        where e.country_id = $1
        order by e.happened_on desc nulls last, e.id desc
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A party's events, most recent first.
pub async fn for_party(pool: &Pool, party_id: i64) -> Result<Vec<Event>> {
    let rows = sqlx::query_as!(
        Event,
        r#"
        select e.kind, e.title, e.happened_on, s.url as source_url
        from events e
        join sources s on s.id = e.source_id
        where e.party_id = $1
        order by e.happened_on desc nulls last, e.id desc
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A person's events, most recent first.
pub async fn for_person(pool: &Pool, person_id: i64) -> Result<Vec<Event>> {
    let rows = sqlx::query_as!(
        Event,
        r#"
        select e.kind, e.title, e.happened_on, s.url as source_url
        from events e
        join sources s on s.id = e.source_id
        where e.person_id = $1
        order by e.happened_on desc nulls last, e.id desc
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
