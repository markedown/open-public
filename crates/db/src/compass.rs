//! The positions layer: policy theses and party stances that power the
//! preference-match compass. Visitor answers are never stored here; only the
//! curated, sourced positions live in the database. Scoring is done in the
//! server layer from `theses_for_country` + `positions_for_country`.

use crate::{Pool, Result};

/// One policy thesis a visitor answers and parties take positions on.
pub struct Thesis {
    pub id: i64,
    pub text: String,
    pub position: i32,
    /// The topic it belongs to, when set.
    pub topic: Option<String>,
}

/// A country's theses, in display order.
pub async fn theses_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Thesis>> {
    let rows = sqlx::query_as!(
        Thesis,
        r#"
        select t.id, t.text, t.position, tp.name as "topic?"
        from theses t
        left join topics tp on tp.id = t.topic_id
        where t.country_id = $1
        order by t.position, t.id
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many theses a country has (drives the compass entry point).
pub async fn count_theses(pool: &Pool, country_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from theses where country_id = $1"#,
        country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A party's stance on a thesis, for scoring the match.
pub struct Position {
    pub thesis_id: i64,
    pub party_id: i64,
    pub stance: i16,
}

/// Every recorded party position for a country's theses.
pub async fn positions_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Position>> {
    let rows = sqlx::query_as!(
        Position,
        r#"
        select pp.thesis_id, pp.party_id, pp.stance
        from party_positions pp
        join theses t on t.id = pp.thesis_id
        where t.country_id = $1
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert a thesis; returns its id.
pub async fn add_thesis(
    pool: &Pool,
    country_id: i64,
    text: &str,
    topic_id: Option<i64>,
    position: i32,
    source_id: i64,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        "insert into theses (country_id, text, topic_id, position, source_id)
         values ($1, $2, $3, $4, $5) returning id",
        country_id,
        text,
        topic_id,
        position,
        source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Set or update a party's stance on a thesis.
pub async fn set_position(
    pool: &Pool,
    thesis_id: i64,
    party_id: i64,
    stance: i16,
    justification: Option<&str>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        "insert into party_positions (thesis_id, party_id, stance, justification, source_id)
         values ($1, $2, $3, $4, $5)
         on conflict (thesis_id, party_id) do update set
           stance = excluded.stance, justification = excluded.justification,
           source_id = excluded.source_id",
        thesis_id,
        party_id,
        stance,
        justification,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}
