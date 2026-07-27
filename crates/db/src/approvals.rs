//! Monthly approval of an entity (a person, a party, or a coalition).
//!
//! An approval is a poll attached to an entity for one calendar month, so it
//! reuses the machinery that already makes a vote trustworthy: the append-only
//! hash chain, the one-vote-per-poll trigger (which here enforces one approval
//! per voter per entity per month), the verified-voter gate, and the published
//! recomputable dump. The poll is created lazily on the first vote of a month;
//! until then the tally is simply zero. Anyone can read a tally; only a
//! verified user can cast.

use chrono::{Datelike, NaiveDate};

use crate::{Pool, Result};

/// The three approval choices, stored as option positions 1..3.
pub const APPROVE: i32 = 1;
pub const DISAPPROVE: i32 = 2;
pub const NO_OPINION: i32 = 3;

/// The entity an approval is about. Exactly one id is set on the poll; the
/// others stay null, and the queries match with `is not distinct from` so one
/// compile-checked statement serves all three kinds.
#[derive(Debug, Clone, Copy)]
pub enum Entity {
    Person(i64),
    Party(i64),
    Coalition(i64),
}

impl Entity {
    /// (person_id, party_id, alliance_id), exactly one of them `Some`.
    fn ids(self) -> (Option<i64>, Option<i64>, Option<i64>) {
        match self {
            Entity::Person(id) => (Some(id), None, None),
            Entity::Party(id) => (None, Some(id), None),
            Entity::Coalition(id) => (None, None, Some(id)),
        }
    }
}

/// The first day of the current month (UTC): the period an approval covers now.
pub fn current_period() -> NaiveDate {
    chrono::Utc::now()
        .date_naive()
        .with_day(1)
        .expect("the first of the month is always a valid date")
}

/// Whether the entity an approval would target actually exists, so a cast for a
/// made-up id is refused rather than creating a poll pointing at nothing.
pub async fn exists(pool: &Pool, entity: Entity) -> Result<bool> {
    let (person, party, alliance) = entity.ids();
    let ok = sqlx::query_scalar!(
        r#"
        select coalesce(
          (select true from people where id = $1),
          (select true from parties where id = $2),
          (select true from alliances where id = $3),
          false) as "exists!"
        "#,
        person,
        party,
        alliance,
    )
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// The approve / disapprove / no-opinion counts for an entity in a month.
#[derive(Debug, Clone)]
pub struct Tally {
    pub approve: i64,
    pub disapprove: i64,
    pub no_opinion: i64,
}

impl Tally {
    /// Everyone who took part, of any opinion.
    pub fn total(&self) -> i64 {
        self.approve + self.disapprove + self.no_opinion
    }
}

/// Count an entity's approval for a period. Returns zeros when no one has voted
/// yet (the poll for that month does not exist until the first vote).
pub async fn tally(pool: &Pool, entity: Entity, period: NaiveDate) -> Result<Tally> {
    let (person, party, alliance) = entity.ids();
    let row = sqlx::query!(
        r#"
        select
          coalesce(count(v.id) filter (where o.position = 1), 0) as "approve!",
          coalesce(count(v.id) filter (where o.position = 2), 0) as "disapprove!",
          coalesce(count(v.id) filter (where o.position = 3), 0) as "no_opinion!"
        from polls p
        join poll_options o on o.poll_id = p.id
        left join poll_votes v on v.option_id = o.id
        where p.kind = 'approval'
          and p.person_id is not distinct from $1
          and p.party_id is not distinct from $2
          and p.alliance_id is not distinct from $3
          and p.approval_period = $4
        "#,
        person,
        party,
        alliance,
        period,
    )
    .fetch_one(pool)
    .await?;
    Ok(Tally {
        approve: row.approve,
        disapprove: row.disapprove,
        no_opinion: row.no_opinion,
    })
}

/// One month's approval counts, for the over-time chart.
#[derive(Debug, Clone)]
pub struct MonthTally {
    pub period: NaiveDate,
    pub approve: i64,
    pub disapprove: i64,
    pub no_opinion: i64,
}

impl MonthTally {
    pub fn total(&self) -> i64 {
        self.approve + self.disapprove + self.no_opinion
    }
}

/// An entity's approval month by month, most recent first, up to `limit`
/// months. Only months that have an approval poll appear (a poll exists only
/// once someone has voted), so a month with no participation is simply absent.
pub async fn history(pool: &Pool, entity: Entity, limit: i64) -> Result<Vec<MonthTally>> {
    let (person, party, alliance) = entity.ids();
    let rows = sqlx::query!(
        r#"
        select p.approval_period as "period!",
          coalesce(count(v.id) filter (where o.position = 1), 0) as "approve!",
          coalesce(count(v.id) filter (where o.position = 2), 0) as "disapprove!",
          coalesce(count(v.id) filter (where o.position = 3), 0) as "no_opinion!"
        from polls p
        join poll_options o on o.poll_id = p.id
        left join poll_votes v on v.option_id = o.id
        where p.kind = 'approval'
          and p.person_id is not distinct from $1
          and p.party_id is not distinct from $2
          and p.alliance_id is not distinct from $3
        group by p.approval_period
        order by p.approval_period desc
        limit $4
        "#,
        person,
        party,
        alliance,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MonthTally {
            period: r.period,
            approve: r.approve,
            disapprove: r.disapprove,
            no_opinion: r.no_opinion,
        })
        .collect())
}

/// Which choice a user made for an entity this month, if any (so the panel can
/// mark their pick). One of `APPROVE` / `DISAPPROVE` / `NO_OPINION`.
pub async fn my_choice(
    pool: &Pool,
    entity: Entity,
    period: NaiveDate,
    user_id: i64,
) -> Result<Option<i32>> {
    let (person, party, alliance) = entity.ids();
    let pos = sqlx::query_scalar!(
        r#"
        select o.position
        from poll_votes v
        join poll_options o on o.id = v.option_id
        join polls p on p.id = v.poll_id
        where p.kind = 'approval'
          and p.person_id is not distinct from $1
          and p.party_id is not distinct from $2
          and p.alliance_id is not distinct from $3
          and p.approval_period = $4
          and v.user_id = $5
        limit 1
        "#,
        person,
        party,
        alliance,
        period,
        user_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(pos)
}

/// Record a user's approval choice for an entity this month. The month's poll
/// is created on demand (idempotently, safe under a race), then the vote is
/// inserted. The vote-chain trigger enforces one approval per user per month,
/// so a second attempt records nothing and is never a replacement. Returns
/// whether a vote was recorded.
pub async fn cast(
    pool: &Pool,
    entity: Entity,
    period: NaiveDate,
    user_id: i64,
    position: i32,
) -> Result<bool> {
    if !(APPROVE..=NO_OPINION).contains(&position) {
        return Ok(false);
    }
    let (person, party, alliance) = entity.ids();
    let mut tx = pool.begin().await?;

    // Create this month's approval poll for the entity if it does not exist.
    // The slug ties the poll to the entity's published slug and the month, so
    // the participation dump is self-describing; the question is a constant
    // because the panel renders a localized label, not this text.
    sqlx::query!(
        r#"
        insert into polls
          (question, slug, kind, approval_period, country_id, person_id, party_id, alliance_id)
        select
          'Monthly approval',
          'approval-'
            || case
                 when $1::bigint is not null then 'person-'   || (select slug from people where id = $1)
                 when $2::bigint is not null then 'party-'    || (select slug from parties where id = $2)
                 else                             'coalition-' || (select slug from alliances where id = $3)
               end
            || '-' || to_char($4::date, 'YYYYMM'),
          'approval',
          $4::date,
          coalesce((select country_id from people where id = $1),
                   (select country_id from parties where id = $2),
                   (select country_id from alliances where id = $3)),
          $1, $2, $3
        on conflict (person_id, party_id, alliance_id, approval_period) where kind = 'approval'
        do nothing
        "#,
        person,
        party,
        alliance,
        period,
    )
    .execute(&mut *tx)
    .await?;

    let poll_id = sqlx::query_scalar!(
        r#"
        select id from polls
        where kind = 'approval'
          and person_id is not distinct from $1
          and party_id is not distinct from $2
          and alliance_id is not distinct from $3
          and approval_period = $4
        "#,
        person,
        party,
        alliance,
        period,
    )
    .fetch_one(&mut *tx)
    .await?;

    // The three fixed options, created idempotently.
    sqlx::query!(
        r#"
        insert into poll_options (poll_id, label, position)
        values ($1, 'Approve', 1), ($1, 'Disapprove', 2), ($1, 'No opinion', 3)
        on conflict (poll_id, position) do nothing
        "#,
        poll_id,
    )
    .execute(&mut *tx)
    .await?;

    let option_id = sqlx::query_scalar!(
        "select id from poll_options where poll_id = $1 and position = $2",
        poll_id,
        position,
    )
    .fetch_one(&mut *tx)
    .await?;

    let recorded = sqlx::query!(
        r#"
        insert into poll_votes (poll_id, option_id, user_id)
        select $1, $2, $3
        where exists (select 1 from poll_options where id = $2 and poll_id = $1)
        on conflict (poll_id, user_id, option_id) do nothing
        "#,
        poll_id,
        option_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(recorded == 1)
}
