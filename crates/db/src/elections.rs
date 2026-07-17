//! Elections and their per-party results.

use chrono::NaiveDate;

use crate::{Pool, Result};

/// An election as stored and listed.
#[derive(Debug, Clone)]
pub struct Election {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub held_on: Option<NaiveDate>,
    pub kind: Option<String>,
    /// An own-words description of what the election was about.
    pub description: Option<String>,
    /// Registered voters, total ballots cast, and valid votes. Optional until
    /// the totals are ingested; enable turnout and vote-share figures.
    pub electorate: Option<i64>,
    pub votes_cast: Option<i64>,
    pub valid_votes: Option<i64>,
}

/// Fields for creating an election.
pub struct NewElection<'a> {
    pub country_id: i64,
    pub name: &'a str,
    pub slug: &'a str,
    pub held_on: Option<NaiveDate>,
    pub kind: Option<&'a str>,
    pub source_id: i64,
}

/// One contestant's result in an election. The contestant is either a party
/// (party_* set) or a plain label (a presidential candidate's name, or a
/// referendum option), never both.
#[derive(Debug, Clone)]
pub struct ResultRow {
    pub party_name: Option<String>,
    pub party_short_name: Option<String>,
    pub party_slug: Option<String>,
    pub party_color: Option<String>,
    pub label: Option<String>,
    pub seats: Option<i32>,
    pub votes: Option<i64>,
}

/// One election in a party's history.
#[derive(Debug, Clone)]
pub struct PartyHistoryEntry {
    pub election_name: String,
    pub election_slug: String,
    pub held_on: Option<NaiveDate>,
    pub seats: Option<i32>,
    pub votes: Option<i64>,
}

/// Insert an election. Returns its id.
pub async fn create(pool: &Pool, e: &NewElection<'_>) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into elections (country_id, name, slug, held_on, kind, source_id)
        values ($1, $2, $3, $4, $5, $6)
        returning id
        "#,
        e.country_id,
        e.name,
        e.slug,
        e.held_on,
        e.kind,
        e.source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Insert or update one party's result in an election.
pub async fn add_result(
    pool: &Pool,
    election_id: i64,
    party_id: i64,
    seats: Option<i32>,
    votes: Option<i64>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"
        insert into election_results (election_id, party_id, seats, votes, source_id)
        values ($1, $2, $3, $4, $5)
        on conflict (election_id, party_id) do update set
            seats = excluded.seats,
            votes = excluded.votes,
            source_id = excluded.source_id
        "#,
        election_id,
        party_id,
        seats,
        votes,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Set an election's turnout totals (registered voters, ballots cast, valid
/// votes).
pub async fn set_turnout(
    pool: &Pool,
    election_id: i64,
    electorate: Option<i64>,
    votes_cast: Option<i64>,
    valid_votes: Option<i64>,
) -> Result<()> {
    sqlx::query!(
        r#"
        update elections
        set electorate = $2, votes_cast = $3, valid_votes = $4
        where id = $1
        "#,
        election_id,
        electorate,
        votes_cast,
        valid_votes,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The URL of an election's source (for a "full results at source" link).
pub async fn source_url(pool: &Pool, election_id: i64) -> Result<Option<String>> {
    let url = sqlx::query_scalar!(
        r#"select s.url from sources s join elections e on e.source_id = s.id where e.id = $1"#,
        election_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(url)
}

/// Set an election's own-words description.
pub async fn set_description(pool: &Pool, election_id: i64, description: &str) -> Result<()> {
    sqlx::query!(
        r#"update elections set description = $2 where id = $1"#,
        election_id,
        description,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch an election by slug.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Election>> {
    let row = sqlx::query_as!(
        Election,
        r#"select id, name, slug, held_on, kind, description, electorate, votes_cast, valid_votes from elections where slug = $1"#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// An election by slug, but only if it belongs to `country_id`. The per-country
/// detail page uses this so a slug from another country reads as not-found.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Election>> {
    let row = sqlx::query_as!(
        Election,
        r#"select id, name, slug, held_on, kind, description, electorate, votes_cast, valid_votes
           from elections where slug = $1 and country_id = $2"#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// A country's elections, most recent first. Names show in `lang` where a
/// published translation exists, else in the original.
pub async fn list_for_country(pool: &Pool, country_id: i64, lang: &str) -> Result<Vec<Election>> {
    let rows = sqlx::query_as!(
        Election,
        r#"
        select e.id, coalesce(ntr.text, e.name) as "name!", e.slug, e.held_on, e.kind,
               e.description, e.electorate, e.votes_cast, e.valid_votes
        from elections e
        left join translations ntr on ntr.entity_type = 'election' and ntr.entity_id = e.id
            and ntr.field = 'name' and ntr.lang = $2 and ntr.status = 'published'
        where e.country_id = $1
        order by e.held_on desc nulls last, e.id desc
        "#,
        country_id,
        lang,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The results of an election, most seats (then votes) first. Each row is a
/// party or a plain label.
pub async fn results(pool: &Pool, election_id: i64) -> Result<Vec<ResultRow>> {
    let rows = sqlx::query_as!(
        ResultRow,
        r#"
        select p.name as "party_name?", p.short_name as "party_short_name?",
               p.slug as "party_slug?", p.color as "party_color?", r.label,
               r.seats, r.votes
        from election_results r
        left join parties p on p.id = r.party_id
        where r.election_id = $1
        order by r.seats desc nulls last, r.votes desc nulls last,
                 coalesce(p.name, r.label)
        "#,
        election_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert or update a label result (a presidential candidate or a referendum
/// option), keyed by its label within the election.
pub async fn add_label_result(
    pool: &Pool,
    election_id: i64,
    label: &str,
    votes: Option<i64>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"
        insert into election_results (election_id, label, votes, source_id)
        values ($1, $2, $3, $4)
        on conflict (election_id, label) do update set
            votes = excluded.votes, source_id = excluded.source_id
        "#,
        election_id,
        label,
        votes,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// A party's results across elections, most recent first.
pub async fn history_for_party(pool: &Pool, party_id: i64) -> Result<Vec<PartyHistoryEntry>> {
    let rows = sqlx::query_as!(
        PartyHistoryEntry,
        r#"
        select e.name as election_name, e.slug as election_slug,
               e.held_on, r.seats, r.votes
        from election_results r
        join elections e on e.id = r.election_id
        where r.party_id = $1
        order by e.held_on desc nulls last, e.id desc
        "#,
        party_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
