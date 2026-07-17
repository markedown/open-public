use domain::models::Country;

use crate::{Pool, Result};

/// A member of the executive (president, vice-president, or minister).
#[derive(Debug, Clone)]
pub struct GovMember {
    pub person_name: String,
    pub person_slug: String,
    pub title: Option<String>,
    pub role_type: String,
}

/// One party's current seat count in parliament.
#[derive(Debug, Clone)]
pub struct SeatCount {
    pub name: String,
    pub short_name: Option<String>,
    pub slug: String,
    pub color: Option<String>,
    pub seats: i64,
}

/// A party's membership in an alliance, flattened for grouping in the handler.
#[derive(Debug, Clone)]
pub struct AllianceParty {
    pub alliance_name: String,
    pub alliance_slug: String,
    pub short_name: Option<String>,
    pub party_slug: String,
    pub color: Option<String>,
}

/// A country for the dashboard feature card.
#[derive(Debug, Clone)]
pub struct CountryCard {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub capital: Option<String>,
    pub government_type: Option<String>,
    pub flag_url: Option<String>,
}

/// All countries, ordered by name. Empty when no country has been seeded, so the
/// dashboard simply omits the feature.
pub async fn list(pool: &Pool) -> Result<Vec<CountryCard>> {
    let rows = sqlx::query_as!(
        CountryCard,
        r#"
        select c.id, c.name, c.slug, c.capital, c.government_type, c.flag_url
        from countries c
        order by c.name collate "name_sort"
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How much content a country has in each section, so the country page can hide
/// the entry points that would lead to an empty list.
#[derive(Debug, Clone)]
pub struct SectionCounts {
    pub people: i64,
    pub parties: i64,
    pub alliances: i64,
    pub elections: i64,
    pub news: i64,
    pub polls: i64,
    pub events: i64,
}

/// Count the content a country holds in each section. News is counted through
/// the people and parties an item links (news items carry no country of their
/// own); polls through their direct, party, or person attachment.
pub async fn section_counts(pool: &Pool, country_id: i64) -> Result<SectionCounts> {
    let r = sqlx::query!(
        r#"
        select
          (select count(*) from people where country_id = $1) as "people!",
          (select count(*) from parties where country_id = $1) as "parties!",
          (select count(*) from alliances where country_id = $1) as "alliances!",
          (select count(*) from elections where country_id = $1) as "elections!",
          (select count(*) from news_items n where
             exists (select 1 from news_item_people x join people pe on pe.id = x.person_id
                     where x.news_item_id = n.id and pe.country_id = $1)
             or exists (select 1 from news_item_parties y join parties pa on pa.id = y.party_id
                        where y.news_item_id = n.id and pa.country_id = $1)) as "news!",
          (select count(*) from polls p where p.country_id = $1
             or exists (select 1 from parties pt where pt.id = p.party_id and pt.country_id = $1)
             or exists (select 1 from people pe where pe.id = p.person_id and pe.country_id = $1)) as "polls!",
          (select count(*) from events where country_id = $1) as "events!"
        "#,
        country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(SectionCounts {
        people: r.people,
        parties: r.parties,
        alliances: r.alliances,
        elections: r.elections,
        news: r.news,
        polls: r.polls,
        events: r.events,
    })
}

pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Country>> {
    let country = sqlx::query_as!(
        Country,
        r#"
        select id, name, slug, capital, government_type, founded_date, population, summary,
               flag_url, legislature_name
        from countries
        where slug = $1
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(country)
}

/// The current executive, ordered president, vice-president, then ministers.
pub async fn government(pool: &Pool, country_id: i64) -> Result<Vec<GovMember>> {
    let rows = sqlx::query_as!(
        GovMember,
        r#"
        select p.full_name as person_name, p.slug as person_slug, r.title, r.role_type
        from roles r
        join people p on p.id = r.person_id
        where r.role_type in
            ('president', 'chancellor', 'prime_minister', 'vice_president', 'minister')
          and r.end_date is null
          and p.country_id = $1
        order by case r.role_type
                   when 'president' then 0
                   when 'chancellor' then 1
                   when 'prime_minister' then 1
                   when 'vice_president' then 2
                   else 3
                 end,
                 r.title
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Seats per party in a country: current party members who currently hold a
/// legislative role (a member of parliament, a senator, a representative: the
/// role types that count as a sitting legislator across countries).
pub async fn seat_distribution(pool: &Pool, country_id: i64) -> Result<Vec<SeatCount>> {
    let rows = sqlx::query_as!(
        SeatCount,
        r#"
        select p.name, p.short_name, p.slug, p.color, count(*) as "seats!"
        from party_memberships m
        join parties p on p.id = m.party_id
        where m.end_date is null and p.country_id = $1
          and exists (
            select 1 from roles r
            where r.person_id = m.person_id
              and r.role_type in ('mp', 'senator', 'representative')
              and r.end_date is null
          )
        group by p.id
        order by count(*) desc, p.name
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// One chamber's composition: its name, the party seat counts, and the
/// independents sitting in it.
#[derive(Debug, Clone)]
pub struct ChamberComposition {
    /// The chamber's name (the org the legislative role names, e.g. a senate or
    /// a house of representatives).
    pub chamber: String,
    pub role_type: String,
    pub parties: Vec<SeatCount>,
    pub independents: i64,
    pub total: i64,
}

/// Seats grouped by chamber, for a legislature that has more than one (a senate
/// and a house). Each member sits in exactly one chamber, named by their
/// legislative role's org. A unicameral legislature returns a single group; the
/// country page uses the combined [`seat_distribution`] bar for that case and
/// this per-chamber view when there is more than one chamber. Chambers are
/// ordered upper (senate) first.
pub async fn chambers(pool: &Pool, country_id: i64) -> Result<Vec<ChamberComposition>> {
    let party_rows = sqlx::query!(
        r#"
        select r.org as "chamber!", r.role_type as "role_type!",
               p.name, p.short_name, p.slug, p.color, count(*) as "seats!"
        from party_memberships m
        join parties p on p.id = m.party_id
        join roles r on r.person_id = m.person_id
          and r.role_type in ('mp', 'senator', 'representative') and r.end_date is null
        where m.end_date is null and p.country_id = $1
        group by r.org, r.role_type, p.id
        order by count(*) desc, p.name
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;

    let indep_rows = sqlx::query!(
        r#"
        select r.org as "chamber!", count(*) as "seats!"
        from people pe
        join roles r on r.person_id = pe.id
          and r.role_type in ('mp', 'senator', 'representative') and r.end_date is null
        where pe.country_id = $1
          and not exists (
            select 1 from party_memberships m where m.person_id = pe.id and m.end_date is null
          )
        group by r.org
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;

    let mut groups: Vec<ChamberComposition> = Vec::new();
    for r in party_rows {
        let seat = SeatCount {
            name: r.name,
            short_name: r.short_name,
            slug: r.slug,
            color: r.color,
            seats: r.seats,
        };
        match groups.iter_mut().find(|g| g.chamber == r.chamber) {
            Some(g) => g.parties.push(seat),
            None => groups.push(ChamberComposition {
                chamber: r.chamber,
                role_type: r.role_type,
                parties: vec![seat],
                independents: 0,
                total: 0,
            }),
        }
    }
    for r in indep_rows {
        if let Some(g) = groups.iter_mut().find(|g| g.chamber == r.chamber) {
            g.independents = r.seats;
        }
    }
    for g in &mut groups {
        g.total = g.parties.iter().map(|p| p.seats).sum::<i64>() + g.independents;
    }
    // Upper chamber (a senate) before a lower one; a lone chamber is unaffected.
    groups.sort_by_key(|g| match g.role_type.as_str() {
        "senator" => 0,
        "mp" => 1,
        _ => 2,
    });
    Ok(groups)
}

/// Sitting legislators in a country who hold no current party membership: the
/// independents. Shown as a neutral segment so the composition reflects every
/// seat we hold, not only the party-affiliated ones.
pub async fn unaffiliated_mp_count(pool: &Pool, country_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "count!"
        from people pe
        where pe.country_id = $1
          and exists (
            select 1 from roles r
            where r.person_id = pe.id
              and r.role_type in ('mp', 'senator', 'representative')
              and r.end_date is null
          )
          and not exists (
            select 1 from party_memberships m
            where m.person_id = pe.id and m.end_date is null
          )
        "#,
        country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// The full size of the chamber: the total seats allocated at the most recent
/// parliamentary election for the country. Fewer members may sit at any moment,
/// since seats fall vacant between elections (resignations, deaths, ministerial
/// appointments), so this is the denominator for a "filled of total" reading.
/// `None` when no parliamentary election records seats.
pub async fn chamber_size(pool: &Pool, country_id: i64) -> Result<Option<i64>> {
    let total = sqlx::query_scalar!(
        r#"
        select sum(er.seats)::bigint as "total"
        from elections e
        join election_results er on er.election_id = e.id
        where e.country_id = $1 and e.kind = 'parliamentary'
        group by e.id, e.held_on
        order by e.held_on desc nulls last
        limit 1
        "#,
        country_id,
    )
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(total)
}

/// Alliances and their member parties in a country, one row per party.
pub async fn alliance_parties(pool: &Pool, country_id: i64) -> Result<Vec<AllianceParty>> {
    let rows = sqlx::query_as!(
        AllianceParty,
        r#"
        select a.name as alliance_name, a.slug as alliance_slug,
               p.short_name, p.slug as party_slug, p.color
        from alliances a
        join party_alliances pa on pa.alliance_id = a.id
        join parties p on p.id = pa.party_id
        where a.country_id = $1
        order by a.name collate "name_sort", p.short_name collate "name_sort"
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
