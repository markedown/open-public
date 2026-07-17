//! Creating statements: validation, entity resolution, persistence.

use chrono::NaiveDate;

use super::{trimmed, Error, Result};
use crate::Pool;

/// Input for creating a statement. It must be attributed to exactly one of a
/// person or a party.
#[derive(Debug, Default)]
pub struct CreateStatement {
    pub person_slug: Option<String>,
    pub party_slug: Option<String>,
    pub text: String,
    pub is_paraphrase: bool,
    pub stated_on: Option<NaiveDate>,
    pub url: String,
    pub outlet: Option<String>,
}

/// Validate, resolve the attributed entity, and persist the statement with its
/// source. Returns the new statement id.
pub async fn create(pool: &Pool, input: CreateStatement) -> Result<i64> {
    let text = input.text.trim();
    let url = input.url.trim();
    if text.is_empty() {
        return Err(Error::validation("statement text is required"));
    }
    if url.is_empty() {
        return Err(Error::validation("a source url is required"));
    }

    let mut person_id = None;
    let mut party_id = None;
    if let Some(slug) = trimmed(&input.person_slug) {
        let person = crate::people::get_by_slug(pool, slug)
            .await?
            .ok_or_else(|| Error::not_found("person", slug))?;
        person_id = Some(person.id);
    }
    if let Some(slug) = trimmed(&input.party_slug) {
        let party = crate::parties::get_by_slug(pool, slug)
            .await?
            .ok_or_else(|| Error::not_found("party", slug))?;
        party_id = Some(party.id);
    }
    // A statement belongs to exactly one entity.
    match (person_id, party_id) {
        (None, None) => {
            return Err(Error::validation(
                "attribute the statement to a person or party",
            ))
        }
        (Some(_), Some(_)) => {
            return Err(Error::validation(
                "attribute the statement to only one of a person or party",
            ))
        }
        _ => {}
    }

    let id = crate::statements::create(
        pool,
        &crate::statements::NewStatement {
            person_id,
            party_id,
            text_original: text,
            is_paraphrase: input.is_paraphrase,
            stated_at: input.stated_on,
            url,
            outlet: trimmed(&input.outlet),
        },
    )
    .await?;
    Ok(id)
}
