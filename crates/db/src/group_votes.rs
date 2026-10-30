//! What parliamentary groups did beyond recorded votes: their votes by show of
//! hands, read from the record of the sitting, and the bills and motions tabled.

use chrono::NaiveDate;

use crate::{Pool, Result};

/// One vote by show of hands. `side` on each party is relative to the subject: for
/// means for the bill or motion, even where the chamber voted on rejecting it.
#[derive(Debug, Clone)]
pub struct GroupVote {
    pub id: i64,
    pub held_on: NaiveDate,
    pub title: String,
    pub document: Option<String>,
    pub adopted: Option<bool>,
    pub excerpt: String,
    pub locator: Option<String>,
    pub source_url: String,
}

/// One party's side in a group vote.
#[derive(Debug, Clone)]
pub struct GroupVoteSide {
    pub group_vote_id: i64,
    pub side: String,
    pub party_slug: String,
    pub short_name: Option<String>,
    pub color: Option<String>,
}

/// One bill or motion, with the source that holds its text.
#[derive(Debug, Clone)]
pub struct Initiative {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub document: String,
    pub tabled_on: NaiveDate,
    pub status: Option<String>,
    pub source_url: String,
}

/// How many group votes a country has, optionally only those a party took part in.
pub async fn count(pool: &Pool, country_id: i64, party: Option<&str>) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "count!"
        from group_votes g
        where g.country_id = $1
          and ($2::text is null or exists (
              select 1 from group_vote_parties gp join parties p on p.id = gp.party_id
              where gp.group_vote_id = g.id and p.slug = $2))
        "#,
        country_id,
        party,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A page of group votes, most recent first.
pub async fn list(
    pool: &Pool,
    country_id: i64,
    party: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<GroupVote>> {
    let rows = sqlx::query_as!(
        GroupVote,
        r#"
        select g.id, g.held_on, g.title, g.document, g.adopted, g.excerpt, g.locator,
               s.url as source_url
        from group_votes g
        join sources s on s.id = g.source_id
        where g.country_id = $1
          and ($2::text is null or exists (
              select 1 from group_vote_parties gp join parties p on p.id = gp.party_id
              where gp.group_vote_id = g.id and p.slug = $2))
        order by g.held_on desc, g.id desc
        limit $3 offset $4
        "#,
        country_id,
        party,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The sides taken in these votes.
pub async fn sides(pool: &Pool, ids: &[i64]) -> Result<Vec<GroupVoteSide>> {
    let rows = sqlx::query_as!(
        GroupVoteSide,
        r#"
        select gp.group_vote_id, gp.side, p.slug as party_slug, p.short_name, p.color
        from group_vote_parties gp
        join parties p on p.id = gp.party_id
        where gp.group_vote_id = any($1)
        order by p.short_name
        "#,
        ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many bills and motions a party has tabled.
pub async fn count_initiatives(pool: &Pool, party_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from initiative_parties where party_id = $1"#,
        party_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A page of a party's bills and motions, most recent first.
pub async fn initiatives_for_party(
    pool: &Pool,
    party_id: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<Initiative>> {
    let rows = sqlx::query_as!(
        Initiative,
        r#"
        select i.id, i.kind, i.title, i.document, i.tabled_on, i.status, s.url as source_url
        from initiatives i
        join initiative_parties ip on ip.initiative_id = i.id
        join sources s on s.id = i.source_id
        where ip.party_id = $1
        order by i.tabled_on desc, i.id desc
        limit $2 offset $3
        "#,
        party_id,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
