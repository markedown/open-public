//! Load the published dataset into a database.
//!
//! The dataset in `dataset/` is what the platform knows, exported as JSON. This
//! reads it back. Anyone can therefore reconstruct the platform from the
//! published files alone, which is what makes publishing them a claim that can
//! be checked rather than a claim that has to be believed.
//!
//! Nothing in the files is keyed by a database id: rows name each other by
//! slug, and a source by its url and content hash together. So the import runs
//! in dependency order, keeping a lookup from each natural key to the id the
//! database assigned, and resolves references through it.
//!
//! It is idempotent. Loading the same dataset twice leaves the same rows, so a
//! database can be brought up to date with a newer export by running it again.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sqlx::PgPool;

/// How a row points at the document it was read from.
#[derive(Deserialize)]
struct SourceRef {
    source_url: Option<String>,
    source_hash: Option<String>,
}

/// The counts of what was loaded, so a caller can report or assert on them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Loaded {
    pub sources: u64,
    pub countries: u64,
    pub people: u64,
    pub parties: u64,
    pub memberships: u64,
    pub roles: u64,
    pub elections: u64,
    pub results: u64,
    pub theses: u64,
    pub evidence: u64,
    pub other: u64,
}

impl Loaded {
    pub fn total(&self) -> u64 {
        self.sources
            + self.countries
            + self.people
            + self.parties
            + self.memberships
            + self.roles
            + self.elections
            + self.results
            + self.theses
            + self.evidence
            + self.other
    }
}

/// Read one file of the dataset. A file that is not there is an empty list,
/// because a dataset with nothing of that kind in it simply omits it.
fn read<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> Result<Vec<T>> {
    let path = dir.join(format!("{name}.json"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {name}.json"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {name}.json"))
}

/// The key a source is identified by, everywhere in the dataset.
fn source_key(url: &str, hash: Option<&str>) -> String {
    format!("{url}\u{1f}{}", hash.unwrap_or(""))
}

impl SourceRef {
    fn lookup(&self, sources: &HashMap<String, i64>) -> Option<i64> {
        let url = self.source_url.as_deref()?;
        sources
            .get(&source_key(url, self.source_hash.as_deref()))
            .copied()
    }
}

macro_rules! row {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Deserialize)]
        struct $name {
            $($field: $ty,)*
            #[serde(flatten)]
            src: SourceRef,
        }
    };
}

#[derive(Deserialize)]
struct Source {
    url: String,
    content_hash: Option<String>,
    content_sha256: Option<String>,
    snapshot_url: Option<String>,
    kind: String,
    title: Option<String>,
    outlet: Option<String>,
    fetched_at: Option<DateTime<Utc>>,
    published_at: Option<DateTime<Utc>>,
}

row!(Country {
    slug: String,
    name: String,
    capital: Option<String>,
    government_type: Option<String>,
    legislature_name: Option<String>,
    founded_date: Option<NaiveDate>,
    population: Option<i64>,
    summary: Option<String>,
    flag_url: Option<String>,
});

row!(Person {
    slug: String,
    full_name: String,
    country: Option<String>,
    birth_date: Option<NaiveDate>,
    birth_place: Option<String>,
    photo_url: Option<String>,
    photo_license: Option<String>,
    summary: Option<String>,
    wikidata_id: Option<String>,
});

row!(Party {
    slug: String,
    name: String,
    short_name: Option<String>,
    country: Option<String>,
    founded_date: Option<NaiveDate>,
    dissolved_date: Option<NaiveDate>,
    ideology_tags: Option<Vec<String>>,
    color: Option<String>,
    summary: Option<String>,
    wikidata_id: Option<String>,
});

row!(Membership {
    person: String,
    party: String,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
});

row!(Role {
    person: String,
    role_type: Option<String>,
    title: Option<String>,
    org: Option<String>,
    district: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
});

row!(Election {
    slug: String,
    name: String,
    country: String,
    held_on: Option<NaiveDate>,
    kind: Option<String>,
    description: Option<String>,
    expected_note: Option<String>,
    electorate: Option<i64>,
    votes_cast: Option<i64>,
    valid_votes: Option<i64>,
});

row!(ElectionResult {
    election: String,
    party: Option<String>,
    label: Option<String>,
    seats: Option<i32>,
    votes: Option<i64>,
});

row!(Alliance {
    slug: String,
    name: String,
    country: String,
    founded_date: Option<NaiveDate>,
    dissolved_date: Option<NaiveDate>,
    summary: Option<String>,
});

row!(PartyAlliance {
    party: String,
    alliance: String,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
});

row!(Event {
    country: Option<String>,
    party: Option<String>,
    person: Option<String>,
    kind: Option<String>,
    title: String,
    happened_on: Option<NaiveDate>,
});

#[derive(Deserialize)]
struct Topic {
    slug: String,
    name: String,
}

row!(Statement {
    person: Option<String>,
    party: Option<String>,
    topic: Option<String>,
    text_original: String,
    is_paraphrase: bool,
    stated_at: Option<NaiveDate>,
});

/// An outlet describes an organisation rather than a fact read from a document,
/// so it carries no source of its own; the leaning recorded about it does.
#[derive(Deserialize)]
struct Outlet {
    slug: String,
    name: String,
    country: Option<String>,
    homepage_url: Option<String>,
    logo_url: Option<String>,
    logo_license: Option<String>,
    leaning: Option<String>,
    summary: Option<String>,
    leaning_source_url: Option<String>,
    leaning_source_hash: Option<String>,
}

row!(PersonAttribute {
    person: String,
    kind: String,
    value: String,
    value_wikidata_id: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
});

row!(PersonEducation {
    person: String,
    institution: String,
    institution_wikidata_id: Option<String>,
    degree: Option<String>,
    field: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
});

row!(Thesis {
    country: String,
    text: String,
    position: i32,
    scope: String,
    topic: Option<String>,
});

row!(Evidence {
    country: String,
    thesis: String,
    party: Option<String>,
    person: Option<String>,
    kind: String,
    stance: i16,
    quote: Option<String>,
    locator: Option<String>,
    occurred_on: Option<NaiveDate>,
});

#[derive(Deserialize)]
struct Poll {
    slug: String,
    question: String,
    country: Option<String>,
    party: Option<String>,
    person: Option<String>,
    kind: Option<String>,
    opens_at: Option<DateTime<Utc>>,
    closes_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct PollOption {
    poll: String,
    label: String,
    position: i32,
}

#[derive(Deserialize)]
struct Translation {
    entity: String,
    entity_type: String,
    field: String,
    lang: String,
    text: String,
    origin: Option<String>,
    source_lang: Option<String>,
}

/// Load every file in `dir` into the database, in dependency order.
pub async fn run(pool: &PgPool, dir: &Path) -> Result<Loaded> {
    let mut n = Loaded::default();

    // Sources first: everything else points at one.
    let mut sources: HashMap<String, i64> = HashMap::new();
    for s in read::<Source>(dir, "sources")? {
        // A source is identified by url and content hash together, but the hash
        // can be absent, and Postgres treats two nulls as different values in a
        // unique index. Relying on the conflict clause alone would therefore
        // insert an unhashed source again on every run, so the row is looked up
        // first, treating null as a value like any other.
        let existing = sqlx::query_scalar!(
            "select id from sources
             where url = $1 and content_hash is not distinct from $2",
            s.url,
            s.content_hash,
        )
        .fetch_optional(pool)
        .await?;
        let id = match existing {
            Some(id) => {
                sqlx::query!(
                    "update sources set title = $2, content_sha256 = $3,
                       snapshot_url = $4 where id = $1",
                    id,
                    s.title,
                    s.content_sha256,
                    s.snapshot_url,
                )
                .execute(pool)
                .await?;
                id
            }
            None => {
                sqlx::query_scalar!(
                    "insert into sources
               (kind, url, title, outlet, fetched_at, published_at, content_hash,
                content_sha256, snapshot_url)
             values ($1, $2, $3, $4, coalesce($5, now()), $6, $7, $8, $9)
             returning id",
                    s.kind,
                    s.url,
                    s.title,
                    s.outlet,
                    s.fetched_at,
                    s.published_at,
                    s.content_hash,
                    s.content_sha256,
                    s.snapshot_url,
                )
                .fetch_one(pool)
                .await?
            }
        };
        sources.insert(source_key(&s.url, s.content_hash.as_deref()), id);
        n.sources += 1;
    }

    // A fact with no source in the dataset cannot be loaded: the schema
    // requires one, and inventing a placeholder would defeat the point.
    macro_rules! src {
        ($row:expr, $what:expr) => {
            match $row.src.lookup(&sources) {
                Some(id) => id,
                None => anyhow::bail!("{}: cites a source that is not in the dataset", $what),
            }
        };
    }

    let mut countries: HashMap<String, i64> = HashMap::new();
    for c in read::<Country>(dir, "countries")? {
        let source_id = src!(c, format!("country {}", c.slug));
        let id = sqlx::query_scalar!(
            "insert into countries
               (slug, name, capital, government_type, legislature_name,
                founded_date, population, summary, flag_url, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             on conflict (slug) do update set name = excluded.name,
               capital = excluded.capital, government_type = excluded.government_type,
               legislature_name = excluded.legislature_name,
               founded_date = excluded.founded_date, population = excluded.population,
               summary = excluded.summary, flag_url = excluded.flag_url
             returning id",
            c.slug,
            c.name,
            c.capital,
            c.government_type,
            c.legislature_name,
            c.founded_date,
            c.population,
            c.summary,
            c.flag_url,
            source_id,
        )
        .fetch_one(pool)
        .await?;
        countries.insert(c.slug, id);
        n.countries += 1;
    }

    let mut people: HashMap<String, i64> = HashMap::new();
    for p in read::<Person>(dir, "people")? {
        let source_id = src!(p, format!("person {}", p.slug));
        let country_id = p.country.as_ref().and_then(|c| countries.get(c)).copied();
        let id = sqlx::query_scalar!(
            "insert into people
               (slug, full_name, country_id, birth_date, birth_place, photo_url,
                photo_license, summary, wikidata_id, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             on conflict (slug) do update set full_name = excluded.full_name,
               country_id = excluded.country_id, birth_date = excluded.birth_date,
               birth_place = excluded.birth_place, photo_url = excluded.photo_url,
               photo_license = excluded.photo_license, summary = excluded.summary,
               wikidata_id = excluded.wikidata_id
             returning id",
            p.slug,
            p.full_name,
            country_id,
            p.birth_date,
            p.birth_place,
            p.photo_url,
            p.photo_license,
            p.summary,
            p.wikidata_id,
            source_id,
        )
        .fetch_one(pool)
        .await?;
        people.insert(p.slug, id);
        n.people += 1;
    }

    let mut parties: HashMap<String, i64> = HashMap::new();
    for p in read::<Party>(dir, "parties")? {
        let source_id = src!(p, format!("party {}", p.slug));
        let country_id = p.country.as_ref().and_then(|c| countries.get(c)).copied();
        let tags = p.ideology_tags.unwrap_or_default();
        let id = sqlx::query_scalar!(
            "insert into parties
               (slug, name, short_name, country_id, founded_date, dissolved_date,
                ideology_tags, color, summary, wikidata_id, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             on conflict (slug) do update set name = excluded.name,
               short_name = excluded.short_name, country_id = excluded.country_id,
               founded_date = excluded.founded_date,
               dissolved_date = excluded.dissolved_date,
               ideology_tags = excluded.ideology_tags, color = excluded.color,
               summary = excluded.summary, wikidata_id = excluded.wikidata_id
             returning id",
            p.slug,
            p.name,
            p.short_name,
            country_id,
            p.founded_date,
            p.dissolved_date,
            &tags,
            p.color,
            p.summary,
            p.wikidata_id,
            source_id,
        )
        .fetch_one(pool)
        .await?;
        parties.insert(p.slug, id);
        n.parties += 1;
    }

    for m in read::<Membership>(dir, "party_memberships")? {
        let source_id = src!(m, format!("membership {} in {}", m.person, m.party));
        let (Some(&person_id), Some(&party_id)) = (people.get(&m.person), parties.get(&m.party))
        else {
            anyhow::bail!("membership names a person or party not in the dataset");
        };
        sqlx::query!(
            "insert into party_memberships
               (person_id, party_id, start_date, end_date, source_id)
             values ($1, $2, $3, $4, $5)
             on conflict (person_id, party_id, start_date) do update set
               end_date = excluded.end_date",
            person_id,
            party_id,
            m.start_date,
            m.end_date,
            source_id,
        )
        .execute(pool)
        .await?;
        n.memberships += 1;
    }

    for r in read::<Role>(dir, "roles")? {
        let source_id = src!(r, format!("role of {}", r.person));
        let Some(&person_id) = people.get(&r.person) else {
            anyhow::bail!("role names a person not in the dataset: {}", r.person);
        };
        sqlx::query!(
            "insert into roles
               (person_id, role_type, title, org, district, start_date, end_date, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8)
             on conflict (person_id, role_type, org, start_date) do update set
               title = excluded.title, district = excluded.district,
               end_date = excluded.end_date",
            person_id,
            r.role_type,
            r.title,
            r.org,
            r.district,
            r.start_date,
            r.end_date,
            source_id,
        )
        .execute(pool)
        .await?;
        n.roles += 1;
    }

    let mut elections: HashMap<String, i64> = HashMap::new();
    for e in read::<Election>(dir, "elections")? {
        let source_id = src!(e, format!("election {}", e.slug));
        let Some(&country_id) = countries.get(&e.country) else {
            anyhow::bail!("election {} names a country not in the dataset", e.slug);
        };
        let id = sqlx::query_scalar!(
            "insert into elections
               (slug, name, country_id, held_on, kind, description, expected_note,
                electorate, votes_cast, valid_votes, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             on conflict (slug) do update set name = excluded.name,
               held_on = excluded.held_on, kind = excluded.kind,
               description = excluded.description,
               expected_note = excluded.expected_note,
               electorate = excluded.electorate, votes_cast = excluded.votes_cast,
               valid_votes = excluded.valid_votes
             returning id",
            e.slug,
            e.name,
            country_id,
            e.held_on,
            e.kind,
            e.description,
            e.expected_note,
            e.electorate,
            e.votes_cast,
            e.valid_votes,
            source_id,
        )
        .fetch_one(pool)
        .await?;
        elections.insert(e.slug, id);
        n.elections += 1;
    }

    for r in read::<ElectionResult>(dir, "election_results")? {
        let source_id = src!(r, format!("result in {}", r.election));
        let Some(&election_id) = elections.get(&r.election) else {
            anyhow::bail!("result names an election not in the dataset");
        };
        let party_id = r.party.as_ref().and_then(|p| parties.get(p)).copied();
        sqlx::query!(
            "insert into election_results (election_id, party_id, label, seats, votes, source_id)
             values ($1, $2, $3, $4, $5, $6)
             on conflict do nothing",
            election_id,
            party_id,
            r.label,
            r.seats,
            r.votes,
            source_id,
        )
        .execute(pool)
        .await?;
        n.results += 1;
    }

    let mut alliances: HashMap<String, i64> = HashMap::new();
    for a in read::<Alliance>(dir, "alliances")? {
        let source_id = src!(a, format!("alliance {}", a.slug));
        let Some(&country_id) = countries.get(&a.country) else {
            anyhow::bail!("alliance {} names a country not in the dataset", a.slug);
        };
        let id = sqlx::query_scalar!(
            "insert into alliances
               (slug, name, country_id, founded_date, dissolved_date, summary, source_id)
             values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (slug) do update set name = excluded.name,
               summary = excluded.summary
             returning id",
            a.slug,
            a.name,
            country_id,
            a.founded_date,
            a.dissolved_date,
            a.summary,
            source_id,
        )
        .fetch_one(pool)
        .await?;
        alliances.insert(a.slug, id);
        n.other += 1;
    }

    for x in read::<PartyAlliance>(dir, "party_alliances")? {
        let source_id = src!(x, format!("{} in {}", x.party, x.alliance));
        let (Some(&party_id), Some(&alliance_id)) =
            (parties.get(&x.party), alliances.get(&x.alliance))
        else {
            anyhow::bail!("alliance membership names a party or alliance not in the dataset");
        };
        sqlx::query!(
            "insert into party_alliances (party_id, alliance_id, start_date, end_date, source_id)
             values ($1, $2, $3, $4, $5) on conflict do nothing",
            party_id,
            alliance_id,
            x.start_date,
            x.end_date,
            source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    for e in read::<Event>(dir, "events")? {
        let source_id = src!(e, format!("event {}", e.title));
        sqlx::query!(
            "insert into events (country_id, party_id, person_id, kind, title, happened_on, source_id)
             values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
            e.country.as_ref().and_then(|c| countries.get(c)).copied(),
            e.party.as_ref().and_then(|p| parties.get(p)).copied(),
            e.person.as_ref().and_then(|p| people.get(p)).copied(),
            e.kind,
            e.title,
            e.happened_on,
            source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    let mut topics: HashMap<String, i64> = HashMap::new();
    for t in read::<Topic>(dir, "topics")? {
        let id = sqlx::query_scalar!(
            "insert into topics (slug, name) values ($1, $2)
             on conflict (slug) do update set name = excluded.name returning id",
            t.slug,
            t.name,
        )
        .fetch_one(pool)
        .await?;
        topics.insert(t.slug, id);
        n.other += 1;
    }

    for s in read::<Statement>(dir, "statements")? {
        let source_id = src!(s, "statement");
        sqlx::query!(
            "insert into statements
               (person_id, party_id, topic_id, text_original, is_paraphrase, stated_at, source_id)
             values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
            s.person.as_ref().and_then(|p| people.get(p)).copied(),
            s.party.as_ref().and_then(|p| parties.get(p)).copied(),
            s.topic.as_ref().and_then(|t| topics.get(t)).copied(),
            s.text_original,
            s.is_paraphrase,
            s.stated_at,
            source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    for o in read::<Outlet>(dir, "outlets")? {
        let leaning_source_id = o
            .leaning_source_url
            .as_deref()
            .and_then(|u| sources.get(&source_key(u, o.leaning_source_hash.as_deref())))
            .copied();
        sqlx::query!(
            "insert into outlets
               (slug, name, country_id, homepage_url, logo_url, logo_license,
                leaning, summary, leaning_source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             on conflict (slug) do update set name = excluded.name,
               leaning = excluded.leaning, summary = excluded.summary,
               leaning_source_id = excluded.leaning_source_id",
            o.slug,
            o.name,
            o.country.as_ref().and_then(|c| countries.get(c)).copied(),
            o.homepage_url,
            o.logo_url,
            o.logo_license,
            o.leaning,
            o.summary,
            leaning_source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    for a in read::<PersonAttribute>(dir, "person_attributes")? {
        let source_id = src!(a, format!("attribute of {}", a.person));
        let Some(&person_id) = people.get(&a.person) else {
            anyhow::bail!("attribute names a person not in the dataset: {}", a.person);
        };
        sqlx::query!(
            "insert into person_attributes
               (person_id, kind, value, value_wikidata_id, start_date, end_date, source_id)
             values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
            person_id,
            a.kind,
            a.value,
            a.value_wikidata_id,
            a.start_date,
            a.end_date,
            source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    for e in read::<PersonEducation>(dir, "person_education")? {
        let source_id = src!(e, format!("education of {}", e.person));
        let Some(&person_id) = people.get(&e.person) else {
            anyhow::bail!("education names a person not in the dataset: {}", e.person);
        };
        sqlx::query!(
            "insert into person_education
               (person_id, institution, institution_wikidata_id, degree, field,
                start_date, end_date, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8) on conflict do nothing",
            person_id,
            e.institution,
            e.institution_wikidata_id,
            e.degree,
            e.field,
            e.start_date,
            e.end_date,
            source_id,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    // A thesis is named by its country and its text, which is how the dataset
    // refers to it from every piece of evidence.
    let mut theses: HashMap<(String, String), i64> = HashMap::new();
    for t in read::<Thesis>(dir, "theses")? {
        let source_id = src!(t, "thesis");
        let Some(&country_id) = countries.get(&t.country) else {
            anyhow::bail!("thesis names a country not in the dataset: {}", t.country);
        };
        // A thesis is named by its country and its text; nothing in the schema
        // makes that pair unique, so it is looked up rather than left to a
        // conflict clause that would never fire.
        let existing = sqlx::query_scalar!(
            "select id from theses where country_id = $1 and text = $2",
            country_id,
            t.text,
        )
        .fetch_optional(pool)
        .await?;
        let id = match existing {
            Some(id) => {
                sqlx::query!(
                    "update theses set position = $2, scope = $3 where id = $1",
                    id,
                    t.position,
                    t.scope,
                )
                .execute(pool)
                .await?;
                id
            }
            None => {
                sqlx::query_scalar!(
                    "insert into theses (country_id, text, position, scope, topic_id, source_id)
                     values ($1, $2, $3, $4, $5, $6) returning id",
                    country_id,
                    t.text,
                    t.position,
                    t.scope,
                    t.topic.as_ref().and_then(|x| topics.get(x)).copied(),
                    source_id,
                )
                .fetch_one(pool)
                .await?
            }
        };
        theses.insert((t.country, t.text), id);
        n.theses += 1;
    }

    for e in read::<Evidence>(dir, "position_evidence")? {
        let source_id = src!(e, "evidence");
        let Some(&thesis_id) = theses.get(&(e.country.clone(), e.thesis.clone())) else {
            anyhow::bail!("evidence names a thesis not in the dataset");
        };
        sqlx::query!(
            "insert into position_evidence
               (thesis_id, party_id, person_id, kind, stance, quote, locator,
                occurred_on, source_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             on conflict (thesis_id, party_id, person_id, kind, source_id) do update set
               stance = excluded.stance, quote = excluded.quote,
               locator = excluded.locator, occurred_on = excluded.occurred_on",
            thesis_id,
            e.party.as_ref().and_then(|p| parties.get(p)).copied(),
            e.person.as_ref().and_then(|p| people.get(p)).copied(),
            e.kind,
            e.stance,
            e.quote,
            e.locator,
            e.occurred_on,
            source_id,
        )
        .execute(pool)
        .await?;
        n.evidence += 1;
    }

    let mut polls: HashMap<String, i64> = HashMap::new();
    for p in read::<Poll>(dir, "polls")? {
        let id = sqlx::query_scalar!(
            "insert into polls (slug, question, country_id, party_id, person_id, kind,
                                opens_at, closes_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8)
             on conflict (slug) do update set question = excluded.question
             returning id",
            p.slug,
            p.question,
            p.country.as_ref().and_then(|c| countries.get(c)).copied(),
            p.party.as_ref().and_then(|x| parties.get(x)).copied(),
            p.person.as_ref().and_then(|x| people.get(x)).copied(),
            p.kind,
            p.opens_at,
            p.closes_at,
        )
        .fetch_one(pool)
        .await?;
        polls.insert(p.slug, id);
        n.other += 1;
    }

    for o in read::<PollOption>(dir, "poll_options")? {
        let Some(&poll_id) = polls.get(&o.poll) else {
            anyhow::bail!("poll option names a poll not in the dataset: {}", o.poll);
        };
        sqlx::query!(
            "insert into poll_options (poll_id, label, position)
             values ($1, $2, $3) on conflict do nothing",
            poll_id,
            o.label,
            o.position,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    for t in read::<Translation>(dir, "translations")? {
        // Only entity types the dataset can name by slug are exported, so the
        // id is resolved from whichever map holds that kind of row.
        let entity_id = match t.entity_type.as_str() {
            "country" => countries.get(&t.entity).copied(),
            "person" => people.get(&t.entity).copied(),
            "party" => parties.get(&t.entity).copied(),
            "election" => elections.get(&t.entity).copied(),
            "alliance" => alliances.get(&t.entity).copied(),
            "poll" => polls.get(&t.entity).copied(),
            "topic" => topics.get(&t.entity).copied(),
            _ => None,
        };
        let Some(entity_id) = entity_id else {
            anyhow::bail!(
                "translation names a {} not in the dataset: {}",
                t.entity_type,
                t.entity
            );
        };
        sqlx::query!(
            "insert into translations
               (entity_type, entity_id, field, lang, text, origin, status, source_lang)
             values ($1, $2, $3, $4, $5, coalesce($6, 'machine'), 'published', $7)
             on conflict do nothing",
            t.entity_type,
            entity_id,
            t.field,
            t.lang,
            t.text,
            t.origin,
            t.source_lang,
        )
        .execute(pool)
        .await?;
        n.other += 1;
    }

    Ok(n)
}
