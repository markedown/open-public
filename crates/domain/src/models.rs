//! Core domain types shared across the workspace.
//!
//! These are plain data types with no database or web dependencies. The `db`
//! crate maps query rows into them; the web layer renders them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// A person as stored and displayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    pub wikidata_id: Option<String>,
    pub full_name: String,
    pub slug: String,
    pub birth_date: Option<NaiveDate>,
    pub birth_place: Option<String>,
    pub photo_url: Option<String>,
    pub photo_license: Option<String>,
    pub summary: Option<String>,
}

/// Fields needed to insert or update a person (keyed on `wikidata_id`).
#[derive(Debug, Clone)]
pub struct NewPerson {
    pub wikidata_id: Option<String>,
    pub full_name: String,
    pub slug: String,
    pub birth_date: Option<NaiveDate>,
    pub birth_place: Option<String>,
    pub photo_url: Option<String>,
    pub photo_license: Option<String>,
    pub summary: Option<String>,
    pub source_id: i64,
    /// The country this person belongs to. `None` only in tests that do not
    /// exercise per-country listing.
    pub country_id: Option<i64>,
}

/// A political party as stored and displayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub id: i64,
    pub wikidata_id: Option<String>,
    pub name: String,
    pub short_name: Option<String>,
    pub slug: String,
    pub founded_date: Option<NaiveDate>,
    pub dissolved_date: Option<NaiveDate>,
    pub ideology_tags: Vec<String>,
    pub summary: Option<String>,
    /// Hex color for the party's badge and page accent (organization color).
    pub color: Option<String>,
}

/// Fields needed to insert or update a party (keyed on `wikidata_id`).
#[derive(Debug, Clone)]
pub struct NewParty {
    pub wikidata_id: Option<String>,
    pub name: String,
    pub short_name: Option<String>,
    pub slug: String,
    pub founded_date: Option<NaiveDate>,
    pub dissolved_date: Option<NaiveDate>,
    pub ideology_tags: Vec<String>,
    pub summary: Option<String>,
    pub source_id: i64,
    /// The country this party belongs to.
    pub country_id: Option<i64>,
}

/// One party membership on a person's timeline, with its source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub party_name: String,
    pub party_short_name: Option<String>,
    pub party_slug: String,
    pub party_color: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub source_id: i64,
    pub source_url: String,
}

/// One role on a person's timeline, with its source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub role_type: String,
    pub title: Option<String>,
    pub org: Option<String>,
    pub district: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub source_id: i64,
    pub source_url: String,
}

/// One entry in a person's education, with its source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Education {
    pub id: i64,
    pub institution: String,
    pub degree: Option<String>,
    pub field: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub source_id: i64,
    pub source_url: String,
}

/// One sourced attribute of a person (an occupation, an ideology, a religion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonAttribute {
    pub id: i64,
    pub kind: String,
    pub value: String,
    pub source_id: i64,
    pub source_url: String,
}

/// A search hit: a person or a party.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub name: String,
    pub slug: String,
    /// The country this row belongs to, since every page that shows it lives
    /// under one. `None` where the row has no country: the schema allows it,
    /// and such a row has no page to link to rather than a page under some
    /// other country.
    pub country: Option<SearchCountry>,
}

/// Just enough of a country to link to a result and to tell two results apart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCountry {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchKind {
    Person,
    Party,
}

/// A poll with its options and current tallies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Poll {
    pub id: i64,
    pub question: String,
    pub slug: String,
    /// How the widget renders and votes: "single", "multi", "yesno", "scale".
    pub kind: String,
    /// An optional freely-licensed image for the question, with its license.
    pub media_url: Option<String>,
    pub media_license: Option<String>,
    pub options: Vec<PollOption>,
}

/// One poll option and how many votes it has received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    pub id: i64,
    pub label: String,
    pub position: i32,
    /// An optional freely-licensed image for this option.
    pub media_url: Option<String>,
    pub votes: i64,
}

/// A news item shown on a person or party page: our short summary in our own
/// words plus the underlying source (the headline links out to it). The full
/// article body is never stored or displayed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: i64,
    pub headline: String,
    pub our_summary: Option<String>,
    pub url: String,
    pub outlet: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
}

/// A sourced statement attributed to a person or party: a short excerpt or a
/// paraphrase in our own words, with the date and the source it came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    pub id: i64,
    pub text_original: String,
    pub is_paraphrase: bool,
    pub stated_at: Option<NaiveDate>,
    pub url: String,
    pub outlet: Option<String>,
}

/// A country and its political facts. Country-agnostic; the platform can hold
/// several, though the first dataset is a single country.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub capital: Option<String>,
    pub government_type: Option<String>,
    pub founded_date: Option<NaiveDate>,
    pub population: Option<i64>,
    pub summary: Option<String>,
    /// A freely-licensed flag image URL.
    pub flag_url: Option<String>,
    /// The country's own name for its legislature (a Congress, an Assembly), in
    /// the source language. `None` falls back to the generic "Parliament".
    pub legislature_name: Option<String>,
}
