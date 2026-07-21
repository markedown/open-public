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
pub async fn theses_for_country(pool: &Pool, country_id: i64, scope: &str) -> Result<Vec<Thesis>> {
    let rows = sqlx::query_as!(
        Thesis,
        r#"
        select t.id, t.text, t.position, tp.name as "topic?"
        from theses t
        left join topics tp on tp.id = t.topic_id
        where t.country_id = $1 and t.scope = $2
        order by t.position, t.id
        "#,
        country_id,
        scope,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many theses of one scope a country has (drives the compass entry point,
/// which only appears once there is something to answer).
pub async fn count_theses(pool: &Pool, country_id: i64, scope: &str) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from theses where country_id = $1 and scope = $2"#,
        country_id,
        scope,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A contestant's effective stance on a thesis: the single value the match is
/// scored against, resolved from that contestant's evidence.
///
/// A contestant is a party in a parliamentary compass and a person in a
/// presidential one, so the id is whichever the thesis set's scope selects.
pub struct Position {
    pub thesis_id: i64,
    pub contestant_id: i64,
    pub stance: i16,
}

/// The evidence kinds that record what a contestant actually did, as opposed to
/// what it said it would do. These outrank stated intention when a stance is
/// resolved, so a party is measured by its actions where they exist.
///
/// Includes the acts available to a party that cannot legislate: tabling a bill
/// and asking the Constitutional Court to annul a law. Without those, only a
/// governing party could ever be judged on its record.
pub const RECORDED_ACTION: [&str; 6] = ["bill", "court", "vote", "law", "decree", "alliance"];

/// Thesis scopes: a parliamentary compass ranks parties, a presidential one
/// ranks people.
pub const SCOPE_PARTY: &str = "party";
pub const SCOPE_PERSON: &str = "person";

/// Every contestant's effective stance on the country's theses of one scope.
///
/// One row per (thesis, contestant): the strongest evidence wins, meaning
/// recorded action before stated intention, then the most recent, then the
/// latest entered.
pub async fn positions_for_country(
    pool: &Pool,
    country_id: i64,
    scope: &str,
) -> Result<Vec<Position>> {
    let rows = sqlx::query_as!(
        Position,
        r#"
        select distinct on (e.thesis_id, coalesce(e.person_id, e.party_id))
               e.thesis_id,
               coalesce(e.person_id, e.party_id) as "contestant_id!",
               e.stance
        from position_evidence e
        join theses t on t.id = e.thesis_id
        where t.country_id = $1 and t.scope = $3
          and case when $3 = 'person' then e.person_id else e.party_id end is not null
        order by e.thesis_id, coalesce(e.person_id, e.party_id),
                 case when e.kind = any($2) then 0 else 1 end,
                 e.occurred_on desc nulls last, e.id desc
        "#,
        country_id,
        &RECORDED_ACTION.map(String::from)[..],
        scope,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One piece of evidence behind a stance, for showing the reader what the
/// position rests on and where pledge and record disagree.
pub struct Evidence {
    pub thesis_id: i64,
    pub contestant_id: i64,
    pub kind: String,
    pub stance: i16,
    pub quote: Option<String>,
    pub occurred_on: Option<chrono::NaiveDate>,
    pub source_url: String,
}

/// All evidence for a country's theses of one scope, strongest first.
pub async fn evidence_for_country(
    pool: &Pool,
    country_id: i64,
    scope: &str,
) -> Result<Vec<Evidence>> {
    let rows = sqlx::query_as!(
        Evidence,
        r#"
        select e.thesis_id,
               coalesce(e.person_id, e.party_id) as "contestant_id!",
               e.kind, e.stance, e.quote, e.occurred_on, s.url as source_url
        from position_evidence e
        join theses t on t.id = e.thesis_id
        join sources s on s.id = e.source_id
        where t.country_id = $1 and t.scope = $3
          and case when $3 = 'person' then e.person_id else e.party_id end is not null
        order by e.thesis_id, coalesce(e.person_id, e.party_id),
                 case when e.kind = any($2) then 0 else 1 end,
                 e.occurred_on desc nulls last, e.id desc
        "#,
        country_id,
        &RECORDED_ACTION.map(String::from)[..],
        scope,
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
    /// Whether this thesis is answered about parties or about people.
    pub scope: String,
}

/// One thesis by id, with its country.
pub async fn get_thesis(pool: &Pool, id: i64) -> Result<Option<ThesisDetail>> {
    let row = sqlx::query_as!(
        ThesisDetail,
        r#"
        select t.id, t.text, t.country_id, c.slug as country_slug, t.scope
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
#[allow(clippy::too_many_arguments)]
pub async fn add_thesis(
    pool: &Pool,
    country_id: i64,
    text: &str,
    topic_id: Option<i64>,
    position: i32,
    scope: &str,
    source_id: i64,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        "insert into theses (country_id, text, topic_id, position, scope, source_id)
         values ($1, $2, $3, $4, $5, $6) returning id",
        country_id,
        text,
        topic_id,
        position,
        scope,
        source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Record one piece of evidence for a contestant's stance on a thesis. The
/// contestant is a party or a person depending on the thesis's scope, so the
/// id lands in whichever column that scope names. Re-importing the same
/// document for the same kind updates it rather than duplicating.
#[allow(clippy::too_many_arguments)]
pub async fn add_evidence(
    pool: &Pool,
    thesis_id: i64,
    scope: &str,
    contestant_id: i64,
    kind: &str,
    stance: i16,
    quote: Option<&str>,
    occurred_on: Option<chrono::NaiveDate>,
    source_id: i64,
) -> Result<()> {
    let (party_id, person_id) = match scope {
        SCOPE_PERSON => (None, Some(contestant_id)),
        _ => (Some(contestant_id), None),
    };
    sqlx::query!(
        "insert into position_evidence
           (thesis_id, party_id, person_id, kind, stance, quote, occurred_on, source_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         on conflict (thesis_id, party_id, person_id, kind, source_id) do update set
           stance = excluded.stance, quote = excluded.quote,
           occurred_on = excluded.occurred_on",
        thesis_id,
        party_id,
        person_id,
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

/// A thesis as the admin index lists it: both scopes together, so a country's
/// full question set is visible on one page.
pub struct AdminThesis {
    pub id: i64,
    pub text: String,
    pub position: i32,
    pub scope: String,
}

/// Every thesis of a country, whichever scope it belongs to.
pub async fn admin_theses_for_country(pool: &Pool, country_id: i64) -> Result<Vec<AdminThesis>> {
    let rows = sqlx::query_as!(
        AdminThesis,
        "select id, text, position, scope from theses
         where country_id = $1 order by scope, position, id",
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One person contestant with a piece of evidence recorded for a thesis.
///
/// Unlike the party grid, this lists only people who already have evidence: a
/// country has thousands of people and no useful grid can list them all, so a
/// new contestant is added by naming them.
pub struct PersonEvidenceRow {
    pub id: i64,
    pub person_id: i64,
    pub person_name: String,
    pub person_slug: String,
    pub kind: String,
    pub stance: i16,
    pub quote: Option<String>,
    pub occurred_on: Option<chrono::NaiveDate>,
}

/// Every person contestant on a thesis, with their recorded evidence.
pub async fn person_evidence_for_thesis(
    pool: &Pool,
    thesis_id: i64,
) -> Result<Vec<PersonEvidenceRow>> {
    let rows = sqlx::query_as!(
        PersonEvidenceRow,
        r#"
        select e.id, p.id as person_id, p.full_name as person_name, p.slug as person_slug,
               e.kind, e.stance, e.quote, e.occurred_on
        from position_evidence e
        join people p on p.id = e.person_id
        where e.thesis_id = $1
        order by p.full_name collate "name_sort", e.id
        "#,
        thesis_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A person who is a contestant in a country's candidate compass, meaning
/// someone with at least one piece of evidence on a candidate thesis.
///
/// Only these people are loaded: a country holds thousands of people, and one
/// with no recorded stance cannot be scored.
pub struct PersonContestant {
    pub id: i64,
    pub full_name: String,
    pub slug: String,
}

/// Every person contesting a country's candidate theses.
pub async fn person_contestants(pool: &Pool, country_id: i64) -> Result<Vec<PersonContestant>> {
    let rows = sqlx::query_as!(
        PersonContestant,
        r#"
        select distinct p.id, p.full_name, p.slug
        from people p
        join position_evidence e on e.person_id = p.id
        join theses t on t.id = e.thesis_id
        where t.country_id = $1 and t.scope = 'person'
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
