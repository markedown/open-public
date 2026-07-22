use chrono::NaiveDate;
use domain::models::{Education, Membership, NewPerson, Person, PersonAttribute, Role};

use crate::{Pool, Result};

/// The attribute kinds a person may carry, in display order. The application
/// reads and writes only these, so a mistyped kind never reaches the database.
pub const ATTRIBUTE_KINDS: [&str; 3] = ["occupation", "ideology", "religion"];

/// Insert or update a person, keyed on `wikidata_id`, and return its id.
pub async fn upsert_person(pool: &Pool, p: &NewPerson) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into people
            (wikidata_id, full_name, slug, birth_date, birth_place,
             photo_url, photo_license, summary, source_id, country_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (wikidata_id) do update set
            full_name     = excluded.full_name,
            slug          = excluded.slug,
            birth_date    = excluded.birth_date,
            birth_place   = excluded.birth_place,
            photo_url     = excluded.photo_url,
            photo_license = excluded.photo_license,
            summary       = excluded.summary,
            country_id    = coalesce(excluded.country_id, people.country_id),
            updated_at    = now()
        returning id
        "#,
        p.wikidata_id,
        p.full_name,
        p.slug,
        p.birth_date,
        p.birth_place,
        p.photo_url,
        p.photo_license,
        p.summary,
        p.source_id,
        p.country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Fetch a person by slug.
pub async fn get_by_slug(pool: &Pool, slug: &str) -> Result<Option<Person>> {
    let person = sqlx::query_as!(
        Person,
        r#"
        select id, wikidata_id, full_name, slug, birth_date, birth_place,
               photo_url, photo_license, summary
        from people
        where slug = $1
        "#,
        slug,
    )
    .fetch_optional(pool)
    .await?;
    Ok(person)
}

/// A person by slug, but only if they belong to `country_id`. The per-country
/// detail page uses this so a slug from another country reads as not-found
/// rather than rendering the wrong country's person under this country.
pub async fn get_by_slug_in_country(
    pool: &Pool,
    slug: &str,
    country_id: i64,
) -> Result<Option<Person>> {
    let person = sqlx::query_as!(
        Person,
        r#"
        select id, wikidata_id, full_name, slug, birth_date, birth_place,
               photo_url, photo_license, summary
        from people
        where slug = $1 and country_id = $2
        "#,
        slug,
        country_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(person)
}

/// List a country's people alphabetically, paginated.
pub async fn list(pool: &Pool, country_id: i64, limit: i64, offset: i64) -> Result<Vec<Person>> {
    let people = sqlx::query_as!(
        Person,
        r#"
        select id, wikidata_id, full_name, slug, birth_date, birth_place,
               photo_url, photo_license, summary
        from people
        where country_id = $1
        order by full_name collate "name_sort"
        limit $2 offset $3
        "#,
        country_id,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(people)
}

/// Number of people in a country.
pub async fn count(pool: &Pool, country_id: i64) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from people where country_id = $1"#,
        country_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// One page of a country's people whose name contains the query, matched
/// diacritic- and case-insensitively (so "ayse" finds "Ayşe"), or all of the
/// country's people when the query is blank. Used by the searchable list page.
pub async fn list_filtered(
    pool: &Pool,
    country_id: i64,
    query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Person>> {
    let q = query.trim();
    if q.is_empty() {
        return list(pool, country_id, limit, offset).await;
    }
    let pattern = format!("%{q}%");
    let people = sqlx::query_as!(
        Person,
        r#"
        select id, wikidata_id, full_name, slug, birth_date, birth_place,
               photo_url, photo_license, summary
        from people
        where country_id = $1 and unaccent(full_name) ilike unaccent($2)
        order by full_name collate "name_sort"
        limit $3 offset $4
        "#,
        country_id,
        pattern,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    Ok(people)
}

/// How many of a country's people match the query (blank query counts all).
pub async fn count_filtered(pool: &Pool, country_id: i64, query: &str) -> Result<i64> {
    let q = query.trim();
    if q.is_empty() {
        return count(pool, country_id).await;
    }
    let pattern = format!("%{q}%");
    let n = sqlx::query_scalar!(
        r#"
        select count(*) as "count!" from people
        where country_id = $1 and unaccent(full_name) ilike unaccent($2)
        "#,
        country_id,
        pattern,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A person's party memberships, newest first, each with its source.
pub async fn memberships(pool: &Pool, person_id: i64) -> Result<Vec<Membership>> {
    let rows = sqlx::query_as!(
        Membership,
        r#"
        select p.name as "party_name!", p.short_name as "party_short_name",
               p.slug as "party_slug!", p.color as "party_color",
               m.start_date, m.end_date,
               s.id as "source_id!", s.url as "source_url!"
        from party_memberships m
        join parties p on p.id = m.party_id
        join sources s on s.id = m.source_id
        where m.person_id = $1
        order by m.start_date desc nulls last
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A person's roles, newest first, each with its source.
pub async fn roles(pool: &Pool, person_id: i64) -> Result<Vec<Role>> {
    let rows = sqlx::query_as!(
        Role,
        r#"
        select r.role_type, r.title, r.org, r.district,
               r.start_date, r.end_date,
               s.id as "source_id!", s.url as "source_url!"
        from roles r
        join sources s on s.id = r.source_id
        where r.person_id = $1
        order by r.start_date desc nulls last
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A person's education, most recent first, each with its source.
pub async fn education(pool: &Pool, person_id: i64) -> Result<Vec<Education>> {
    let rows = sqlx::query_as!(
        Education,
        r#"
        select e.id, e.institution, e.degree, e.field, e.start_date, e.end_date,
               e.source_id as "source_id!", s.url as "source_url!"
        from person_education e
        join sources s on s.id = e.source_id
        where e.person_id = $1
        order by e.start_date desc nulls last, e.institution
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A person's sourced attributes (occupations, ideology, religion), grouped by
/// kind in the display order of [`ATTRIBUTE_KINDS`].
pub async fn attributes(pool: &Pool, person_id: i64) -> Result<Vec<PersonAttribute>> {
    let rows = sqlx::query_as!(
        PersonAttribute,
        r#"
        select a.id, a.kind, a.value, a.source_id as "source_id!", s.url as "source_url!"
        from person_attributes a
        join sources s on s.id = a.source_id
        where a.person_id = $1
        order by array_position(array['occupation','ideology','religion'], a.kind), a.value
        "#,
        person_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert or update an education entry, keyed on
/// `(person, institution, degree, start_date)`. Returns the id.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_education(
    pool: &Pool,
    person_id: i64,
    institution: &str,
    institution_wikidata_id: Option<&str>,
    degree: Option<&str>,
    field: Option<&str>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    source_id: i64,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into person_education
            (person_id, institution, institution_wikidata_id, degree, field, start_date, end_date, source_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        on conflict (person_id, institution, degree, start_date) do update set
            institution_wikidata_id = excluded.institution_wikidata_id,
            field = excluded.field,
            end_date = excluded.end_date,
            source_id = excluded.source_id
        returning id
        "#,
        person_id,
        institution,
        institution_wikidata_id,
        degree,
        field,
        start_date,
        end_date,
        source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Insert or update an attribute, keyed on `(person, kind, value)`. Returns the
/// id. `kind` must be one of [`ATTRIBUTE_KINDS`].
pub async fn upsert_attribute(
    pool: &Pool,
    person_id: i64,
    kind: &str,
    value: &str,
    value_wikidata_id: Option<&str>,
    source_id: i64,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into person_attributes (person_id, kind, value, value_wikidata_id, source_id)
        values ($1, $2, $3, $4, $5)
        on conflict (person_id, kind, value) do update set
            value_wikidata_id = excluded.value_wikidata_id,
            source_id = excluded.source_id
        returning id
        "#,
        person_id,
        kind,
        value,
        value_wikidata_id,
        source_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Delete an education entry (admin correction).
pub async fn delete_education(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!(r#"delete from person_education where id = $1"#, id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete an attribute (admin correction).
pub async fn delete_attribute(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!(r#"delete from person_attributes where id = $1"#, id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert or update a party membership, keyed on `(person, party, start_date)`.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_membership(
    pool: &Pool,
    person_id: i64,
    party_id: i64,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"
        insert into party_memberships (person_id, party_id, start_date, end_date, source_id)
        values ($1, $2, $3, $4, $5)
        on conflict (person_id, party_id, start_date) do update set
            end_date  = excluded.end_date,
            source_id = excluded.source_id
        "#,
        person_id,
        party_id,
        start_date,
        end_date,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert or update a role, keyed on `(person, role_type, org, start_date)`.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_role(
    pool: &Pool,
    person_id: i64,
    role_type: &str,
    title: Option<&str>,
    org: Option<&str>,
    district: Option<&str>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    source_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"
        insert into roles (person_id, role_type, title, org, district, start_date, end_date, source_id)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        on conflict (person_id, role_type, org, start_date) do update set
            title     = excluded.title,
            district  = excluded.district,
            end_date  = excluded.end_date,
            source_id = excluded.source_id
        "#,
        person_id,
        role_type,
        title,
        org,
        district,
        start_date,
        end_date,
        source_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The party each person currently sits in, for the people index, so a row can
/// show party affiliation the way a party page shows its members. Returned as
/// `(person_id, party_slug, short_name, color)`; a person with no current
/// membership is absent. When someone sits in more than one party (rare, and a
/// data artefact), the alphabetically first is taken so the result is stable.
pub struct PersonParty {
    pub person_id: i64,
    pub party_slug: String,
    pub short_name: Option<String>,
    pub color: Option<String>,
}

pub async fn current_parties(pool: &Pool, country_id: i64) -> Result<Vec<PersonParty>> {
    let rows = sqlx::query_as!(
        PersonParty,
        r#"
        select distinct on (m.person_id)
               m.person_id, pa.slug as party_slug, pa.short_name, pa.color
        from party_memberships m
        join parties pa on pa.id = m.party_id
        join people pe on pe.id = m.person_id
        where pe.country_id = $1 and m.end_date is null
        order by m.person_id, pa.name collate "name_sort"
        "#,
        country_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
