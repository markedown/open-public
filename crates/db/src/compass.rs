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

/// A party's effective stance on a thesis: the single value the match is
/// scored against, resolved from that party's evidence.
pub struct Position {
    pub thesis_id: i64,
    pub party_id: i64,
    pub stance: i16,
}

/// The evidence kinds that record what a party actually did, as opposed to what
/// it said it would do. These outrank stated intention when a stance is
/// resolved, so a party is measured by its actions where they exist.
///
/// Includes the acts available to a party that cannot legislate: tabling a bill
/// and asking the Constitutional Court to annul a law. Without those, only a
/// governing party could ever be judged on its record.
pub const RECORDED_ACTION: [&str; 6] = ["bill", "court", "vote", "law", "decree", "alliance"];

/// Every party's effective stance on the country's theses.
///
/// One row per (thesis, party): the strongest evidence wins, meaning recorded
/// action before stated intention, then the most recent, then the latest
/// entered. A party that only ever pledged still gets a stance; a party that
/// also acted is judged on the action.
pub async fn positions_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Position>> {
    let rows = sqlx::query_as!(
        Position,
        r#"
        select distinct on (e.thesis_id, e.party_id)
               e.thesis_id, e.party_id, e.stance
        from position_evidence e
        join theses t on t.id = e.thesis_id
        where t.country_id = $1
        order by e.thesis_id, e.party_id,
                 case when e.kind = any($2) then 0 else 1 end,
                 e.occurred_on desc nulls last, e.id desc
        "#,
        country_id,
        &RECORDED_ACTION.map(String::from)[..],
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One piece of evidence behind a stance, for showing the reader what the
/// position rests on and where pledge and record disagree.
pub struct Evidence {
    pub thesis_id: i64,
    pub party_id: i64,
    pub kind: String,
    pub stance: i16,
    pub quote: Option<String>,
    pub occurred_on: Option<chrono::NaiveDate>,
    pub source_url: String,
}

/// All evidence for a country's theses, strongest first within each position.
pub async fn evidence_for_country(pool: &Pool, country_id: i64) -> Result<Vec<Evidence>> {
    let rows = sqlx::query_as!(
        Evidence,
        r#"
        select e.thesis_id, e.party_id, e.kind, e.stance, e.quote, e.occurred_on,
               s.url as source_url
        from position_evidence e
        join theses t on t.id = e.thesis_id
        join sources s on s.id = e.source_id
        where t.country_id = $1
        order by e.thesis_id, e.party_id,
                 case when e.kind = any($2) then 0 else 1 end,
                 e.occurred_on desc nulls last, e.id desc
        "#,
        country_id,
        &RECORDED_ACTION.map(String::from)[..],
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

/// Record one piece of evidence for a party's stance on a thesis. Re-importing
/// the same document for the same kind updates it rather than duplicating.
#[allow(clippy::too_many_arguments)]
pub async fn add_evidence(
    pool: &Pool,
    thesis_id: i64,
    party_id: i64,
    kind: &str,
    stance: i16,
    quote: Option<&str>,
    occurred_on: Option<chrono::NaiveDate>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        "insert into position_evidence
           (thesis_id, party_id, kind, stance, quote, occurred_on, source_id)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (thesis_id, party_id, kind, source_id) do update set
           stance = excluded.stance, quote = excluded.quote,
           occurred_on = excluded.occurred_on",
        thesis_id,
        party_id,
        kind,
        stance,
        quote,
        occurred_on,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove one piece of evidence.
pub async fn delete_evidence(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!("delete from position_evidence where id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A party of the thesis's country with the evidence recorded for it, for the
/// admin grid. Parties with no evidence yet still appear, so one can be added.
pub struct EvidenceRow {
    pub id: Option<i64>,
    pub party_id: i64,
    pub party_name: String,
    pub short_name: Option<String>,
    pub color: Option<String>,
    pub kind: Option<String>,
    pub stance: Option<i16>,
    pub quote: Option<String>,
    pub occurred_on: Option<chrono::NaiveDate>,
}

/// Every party of the thesis's country with each piece of evidence recorded.
pub async fn evidence_for_thesis(pool: &Pool, thesis_id: i64) -> Result<Vec<EvidenceRow>> {
    let rows = sqlx::query_as!(
        EvidenceRow,
        r#"
        select e.id as "id?", p.id as party_id, p.name as party_name, p.short_name,
               p.color, e.kind as "kind?", e.stance as "stance?", e.quote,
               e.occurred_on
        from theses t
        join parties p on p.country_id = t.country_id
        left join position_evidence e on e.thesis_id = t.id and e.party_id = p.id
        where t.id = $1
        order by p.name collate "name_sort", e.id
        "#,
        thesis_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete a thesis; its evidence cascades.
pub async fn delete_thesis(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!("delete from theses where id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}
