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

/// A party's stance on a thesis, for scoring the match and for showing the
/// reader where the stance came from. The source travels with the stance so a
/// visitor can check any position rather than take it on trust.
pub struct Position {
    pub thesis_id: i64,
    pub party_id: i64,
    pub stance: i16,
    pub justification: Option<String>,
    pub source_url: String,
}

/// Every recorded party position for a country's theses, with its source.
pub async fn positions_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Position>> {
    let rows = sqlx::query_as!(
        Position,
        r#"
        select pp.thesis_id, pp.party_id, pp.stance, pp.justification, s.url as source_url
        from party_positions pp
        join theses t on t.id = pp.thesis_id
        join sources s on s.id = pp.source_id
        where t.country_id = $1
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A thesis with the country it belongs to, for the admin pages that work on a
/// single thesis and need to scope parties and links to its country.
pub struct ThesisDetail {
    pub id: i64,
    pub text: String,
    pub country_id: i64,
    pub country_slug: String,
}

/// One thesis by id, with its country.
pub async fn get_thesis(pool: &Pool, id: i64) -> Result<Option<ThesisDetail>> {
    let row = sqlx::query_as!(
        ThesisDetail,
        r#"
        select t.id, t.text, t.country_id, c.slug as country_slug
        from theses t
        join countries c on c.id = t.country_id
        where t.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
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

/// Delete a thesis; its party positions cascade.
pub async fn delete_thesis(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!("delete from theses where id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A party of the thesis's country with its stance (if any) on that thesis, for
/// the admin positions grid.
pub struct StanceRow {
    pub party_id: i64,
    pub party_name: String,
    pub short_name: Option<String>,
    pub color: Option<String>,
    pub stance: Option<i16>,
    pub justification: Option<String>,
}

/// Every party of the thesis's country with its stance, where one is recorded.
pub async fn stances_for_thesis(pool: &Pool, thesis_id: i64) -> Result<Vec<StanceRow>> {
    let rows = sqlx::query_as!(
        StanceRow,
        r#"
        select p.id as party_id, p.name as party_name, p.short_name, p.color,
               pp.stance as "stance?", pp.justification
        from theses t
        join parties p on p.country_id = t.country_id
        left join party_positions pp on pp.thesis_id = t.id and pp.party_id = p.id
        where t.id = $1
        order by p.name collate "name_sort"
        "#,
        thesis_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Remove a party's stance on a thesis.
pub async fn clear_position(pool: &Pool, thesis_id: i64, party_id: i64) -> Result<()> {
    sqlx::query!(
        "delete from party_positions where thesis_id = $1 and party_id = $2",
        thesis_id,
        party_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}
