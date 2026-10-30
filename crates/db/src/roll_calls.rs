//! Recorded votes in a chamber and how each member voted.
//!
//! A member's party is read from `party_memberships` as of the day of the vote,
//! so a later change of party does not rewrite the record.

use chrono::NaiveDate;

use crate::{Pool, Result};

/// One recorded vote, with its totals.
#[derive(Debug, Clone)]
pub struct RollCall {
    pub id: i64,
    pub slug: String,
    pub chamber: String,
    pub held_on: NaiveDate,
    pub title: String,
    pub source_url: String,
    pub yes: i64,
    pub no: i64,
    pub abstain: i64,
    pub absent: i64,
}

/// How one party's members voted. `party_slug` is `None` for those in no party.
#[derive(Debug, Clone)]
pub struct PartyVotes {
    pub party_slug: Option<String>,
    pub short_name: Option<String>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub yes: i64,
    pub no: i64,
    pub abstain: i64,
    pub absent: i64,
}

/// One member's vote in one roll call.
#[derive(Debug, Clone)]
pub struct MemberVote {
    pub person_name: String,
    pub person_slug: String,
    pub party_slug: Option<String>,
    pub choice: String,
}

/// One of a person's votes, and what most of their party did.
#[derive(Debug, Clone)]
pub struct PersonVote {
    pub slug: String,
    pub title: String,
    pub held_on: NaiveDate,
    pub source_url: String,
    pub choice: String,
    /// The most common choice among those of their party who voted, if any did.
    pub party_majority: Option<String>,
}

/// A person's record over all roll calls we hold for them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersonRecord {
    pub total: i64,
    pub absent: i64,
    pub against_party: i64,
}

/// How many roll calls a country has, matching an optional title search.
pub async fn count(pool: &Pool, country_id: i64, query: &str) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "n!" from roll_calls
        where country_id = $1
          and ($2 = '' or unaccent(title) ilike '%' || unaccent($2) || '%')
        "#,
        country_id,
        query,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A page of a country's roll calls, newest first.
pub async fn list(
    pool: &Pool,
    country_id: i64,
    query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<RollCall>> {
    let rows = sqlx::query_as!(
        RollCall,
        r#"
        select rc.id, rc.slug, rc.chamber, rc.held_on, rc.title, s.url as source_url,
               count(*) filter (where v.choice = 'yes') as "yes!",
               count(*) filter (where v.choice = 'no') as "no!",
               count(*) filter (where v.choice = 'abstain') as "abstain!",
               count(*) filter (where v.choice = 'absent') as "absent!"
        from roll_calls rc
        join sources s on s.id = rc.source_id
        left join roll_call_votes v on v.roll_call_id = rc.id
        where rc.country_id = $1
          and ($2 = '' or unaccent(rc.title) ilike '%' || unaccent($2) || '%')
        group by rc.id, s.url
        order by rc.held_on desc, rc.slug desc
        limit $3 offset $4
        "#,
        country_id,
        query,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One roll call of a country, by slug.
pub async fn get(pool: &Pool, country_id: i64, slug: &str) -> Result<Option<RollCall>> {
    let row = sqlx::query_as!(
        RollCall,
        r#"
        select rc.id, rc.slug, rc.chamber, rc.held_on, rc.title, s.url as source_url,
               count(*) filter (where v.choice = 'yes') as "yes!",
               count(*) filter (where v.choice = 'no') as "no!",
               count(*) filter (where v.choice = 'abstain') as "abstain!",
               count(*) filter (where v.choice = 'absent') as "absent!"
        from roll_calls rc
        join sources s on s.id = rc.source_id
        left join roll_call_votes v on v.roll_call_id = rc.id
        where rc.country_id = $1 and rc.slug = $2
        group by rc.id, s.url
        "#,
        country_id,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The vote split by party, largest party first, members of no party last.
pub async fn by_party(pool: &Pool, roll_call_id: i64) -> Result<Vec<PartyVotes>> {
    let rows = sqlx::query_as!(
        PartyVotes,
        r#"
        select p.slug as "party_slug?", p.short_name as "short_name?", p.name as "name?",
               p.color as "color?",
               count(*) filter (where v.choice = 'yes') as "yes!",
               count(*) filter (where v.choice = 'no') as "no!",
               count(*) filter (where v.choice = 'abstain') as "abstain!",
               count(*) filter (where v.choice = 'absent') as "absent!"
        from roll_call_votes v
        join roll_calls rc on rc.id = v.roll_call_id
        left join lateral (
            select m.party_id from party_memberships m
            where m.person_id = v.person_id
              and (m.start_date is null or m.start_date <= rc.held_on)
              and (m.end_date is null or m.end_date >= rc.held_on)
            order by m.start_date desc nulls last
            limit 1
        ) m on true
        left join parties p on p.id = m.party_id
        where v.roll_call_id = $1
        group by p.id
        order by p.id is null, count(*) desc, p.slug
        "#,
        roll_call_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every member's vote in a roll call, by name.
pub async fn members(pool: &Pool, roll_call_id: i64) -> Result<Vec<MemberVote>> {
    let rows = sqlx::query_as!(
        MemberVote,
        r#"
        select pe.full_name as person_name, pe.slug as person_slug,
               p.slug as "party_slug?", v.choice
        from roll_call_votes v
        join roll_calls rc on rc.id = v.roll_call_id
        join people pe on pe.id = v.person_id
        left join lateral (
            select m.party_id from party_memberships m
            where m.person_id = v.person_id
              and (m.start_date is null or m.start_date <= rc.held_on)
              and (m.end_date is null or m.end_date >= rc.held_on)
            order by m.start_date desc nulls last
            limit 1
        ) m on true
        left join parties p on p.id = m.party_id
        where v.roll_call_id = $1
        order by pe.full_name collate "name_sort"
        "#,
        roll_call_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A page of a person's votes, newest first, each with their party's majority.
pub async fn for_person(
    pool: &Pool,
    person_id: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<PersonVote>> {
    let rows = sqlx::query_as!(
        PersonVote,
        r#"
        with mine as (
            select rc.id, rc.slug, rc.title, rc.held_on, s.url as source_url, v.choice,
                   (select m.party_id from party_memberships m
                    where m.person_id = v.person_id
                      and (m.start_date is null or m.start_date <= rc.held_on)
                      and (m.end_date is null or m.end_date >= rc.held_on)
                    order by m.start_date desc nulls last
                    limit 1) as party_id
            from roll_call_votes v
            join roll_calls rc on rc.id = v.roll_call_id
            join sources s on s.id = rc.source_id
            where v.person_id = $1
            order by rc.held_on desc, rc.slug desc
            limit $2 offset $3
        )
        select mine.slug, mine.title, mine.held_on, mine.source_url, mine.choice,
               (select o.choice from roll_call_votes o
                join party_memberships om on om.person_id = o.person_id
                where o.roll_call_id = mine.id and o.choice <> 'absent'
                  and om.party_id = mine.party_id
                  and (om.start_date is null or om.start_date <= mine.held_on)
                  and (om.end_date is null or om.end_date >= mine.held_on)
                group by o.choice
                order by count(*) desc, o.choice
                limit 1) as party_majority
        from mine
        order by mine.held_on desc, mine.slug desc
        "#,
        person_id,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How often a person voted, and how often against most of their own party.
pub async fn record(pool: &Pool, person_id: i64) -> Result<PersonRecord> {
    let votes = for_person(pool, person_id, i64::MAX, 0).await?;
    Ok(PersonRecord {
        total: votes.len() as i64,
        absent: votes.iter().filter(|v| v.choice == "absent").count() as i64,
        against_party: votes.iter().filter(|v| differs_from_party(v)).count() as i64,
    })
}

/// Whether a member who voted went another way than most of their party.
pub fn differs_from_party(vote: &PersonVote) -> bool {
    vote.choice != "absent"
        && vote
            .party_majority
            .as_deref()
            .is_some_and(|m| m != vote.choice)
}
