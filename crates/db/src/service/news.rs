//! Creating news items: validation, entity resolution, and persistence.

use chrono::NaiveDate;

use super::{trimmed, Error, Result};
use crate::Pool;

/// Input for creating a news item. Callers pass entity slugs (both the web form
/// and the API speak slugs); trimming and validation happen inside [`create`].
#[derive(Debug, Default)]
pub struct CreateNews {
    pub person_slug: Option<String>,
    pub party_slug: Option<String>,
    pub headline: String,
    pub url: String,
    pub outlet: Option<String>,
    pub published_on: Option<NaiveDate>,
    pub our_summary: Option<String>,
}

/// Validate the input, resolve the linked entities, and persist the news item.
/// Requires a headline, a source URL, and at least one linked person or party.
/// Returns the new news id.
pub async fn create(pool: &Pool, input: CreateNews) -> Result<i64> {
    let headline = input.headline.trim();
    let url = input.url.trim();
    if headline.is_empty() {
        return Err(Error::validation("a headline is required"));
    }
    if url.is_empty() {
        return Err(Error::validation("a source url is required"));
    }

    let mut person_ids = Vec::new();
    let mut party_ids = Vec::new();

    if let Some(slug) = trimmed(&input.person_slug) {
        let person = crate::people::get_by_slug(pool, slug)
            .await?
            .ok_or_else(|| Error::not_found("person", slug))?;
        person_ids.push(person.id);
    }
    if let Some(slug) = trimmed(&input.party_slug) {
        let party = crate::parties::get_by_slug(pool, slug)
            .await?
            .ok_or_else(|| Error::not_found("party", slug))?;
        party_ids.push(party.id);
    }
    if person_ids.is_empty() && party_ids.is_empty() {
        return Err(Error::validation(
            "link the news to at least one person or party",
        ));
    }

    let published_at = input
        .published_on
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc());

    let id = crate::news::create(
        pool,
        &crate::news::NewNews {
            url,
            outlet: trimmed(&input.outlet),
            published_at,
            headline,
            our_summary: trimmed(&input.our_summary),
            person_ids: &person_ids,
            party_ids: &party_ids,
        },
    )
    .await?;
    Ok(id)
}
