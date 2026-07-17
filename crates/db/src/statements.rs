//! Sourced statements attributed to a person or party.
//!
//! A statement stores a short excerpt or a paraphrase in our own words, the
//! date, and a reference to its source. Creating one inserts the source and the
//! statement together.

use chrono::NaiveDate;
use domain::models::Statement;

use crate::{Pool, Result};

/// Fields for creating a statement.
pub struct NewStatement<'a> {
    pub person_id: Option<i64>,
    pub party_id: Option<i64>,
    pub text_original: &'a str,
    pub is_paraphrase: bool,
    pub stated_at: Option<NaiveDate>,
    pub url: &'a str,
    pub outlet: Option<&'a str>,
}

/// Insert a statement with its source. Returns the statement id.
pub async fn create(pool: &Pool, s: &NewStatement<'_>) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let source_id: i64 = sqlx::query_scalar!(
        r#"
        insert into sources (kind, url, outlet, fetched_at)
        values ('manual', $1, $2, now())
        returning id
        "#,
        s.url,
        s.outlet,
    )
    .fetch_one(&mut *tx)
    .await?;

    let id: i64 = sqlx::query_scalar!(
        r#"
        insert into statements
            (person_id, party_id, text_original, is_paraphrase, stated_at, source_id)
        values ($1, $2, $3, $4, $5, $6)
        returning id
        "#,
        s.person_id,
        s.party_id,
        s.text_original,
        s.is_paraphrase,
        s.stated_at,
        source_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(id)
}

/// Statements attributed to a person, newest first.
pub async fn for_person(pool: &Pool, person_id: i64) -> Result<Vec<Statement>> {
    let rows = sqlx::query_as!(
        Statement,
        r#"
        select st.id, st.text_original, st.is_paraphrase, st.stated_at,
               s.url as "url!", s.outlet
        from statements st
        join sources s on s.id = st.source_id
        where st.person_id = $1
        order by st.stated_at desc nulls last, st.id desc
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Statements attributed to a party, newest first.
pub async fn for_party(pool: &Pool, party_id: i64) -> Result<Vec<Statement>> {
    let rows = sqlx::query_as!(
        Statement,
        r#"
        select st.id, st.text_original, st.is_paraphrase, st.stated_at,
               s.url as "url!", s.outlet
        from statements st
        join sources s on s.id = st.source_id
        where st.party_id = $1
        order by st.stated_at desc nulls last, st.id desc
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
