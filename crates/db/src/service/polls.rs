//! Creating polls: validation, entity resolution, slug generation.

use domain::slug::slugify;

use super::{trimmed, Error, Result};
use crate::Pool;

/// Input for creating a poll. Options may contain blanks (empty form rows);
/// they are trimmed and dropped. At least two must remain.
#[derive(Debug, Default)]
pub struct CreatePoll {
    /// Attach the poll directly to a country (a country-level question). Only
    /// used when neither a person nor a party is given.
    pub country_slug: Option<String>,
    pub person_slug: Option<String>,
    pub party_slug: Option<String>,
    pub question: String,
    /// Render/vote kind. Empty falls back to "single". Unknown kinds are
    /// rejected.
    pub kind: String,
    /// An optional freely-licensed image for the question, and the license that
    /// covers every image on the poll. If any image is present the license is
    /// required.
    pub media_url: Option<String>,
    pub media_license: Option<String>,
    /// Option labels and, parallel to them, an optional image URL per option
    /// (blank = none). An option is kept when it has a label or an image.
    pub options: Vec<String>,
    pub option_media: Vec<String>,
}

/// The poll kinds this service accepts.
const POLL_KINDS: [&str; 4] = ["single", "yesno", "scale", "multi"];

/// Validate, resolve the optional linked entity, generate a unique slug, and
/// persist the poll with its options. Returns the new poll's slug.
pub async fn create(pool: &Pool, input: CreatePoll) -> Result<String> {
    let question = input.question.trim();
    if question.is_empty() {
        return Err(Error::validation("a question is required"));
    }

    let kind = match input.kind.trim() {
        "" => "single",
        k if POLL_KINDS.contains(&k) => k,
        _ => return Err(Error::validation("unknown poll kind")),
    };

    // Pair each label with its optional image, keeping options that have either.
    let options: Vec<(String, Option<String>)> = input
        .options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let media = input
                .option_media
                .get(i)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            (label.trim().to_string(), media)
        })
        .filter(|(label, media)| !label.is_empty() || media.is_some())
        .collect();
    if options.len() < 2 {
        return Err(Error::validation("a poll needs at least two options"));
    }

    let question_media = trimmed(&input.media_url).map(str::to_string);
    let media_license = trimmed(&input.media_license).map(str::to_string);
    // A freely-licensed image must carry its license.
    let has_image = question_media.is_some() || options.iter().any(|(_, m)| m.is_some());
    if has_image && media_license.is_none() {
        return Err(Error::validation("an image requires a license"));
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

    // A country attachment applies only to polls not tied to a person or party,
    // so a country-level poll surfaces on the country page rather than being
    // double-counted under an entity.
    let mut country_id = None;
    if person_id.is_none() && party_id.is_none() {
        if let Some(slug) = trimmed(&input.country_slug) {
            let country = crate::country::get_by_slug(pool, slug)
                .await?
                .ok_or_else(|| Error::not_found("country", slug))?;
            country_id = Some(country.id);
        }
    }

    let slug = unique_slug(pool, question).await?;

    let new_options: Vec<crate::polls::NewOption> = options
        .iter()
        .map(|(label, media)| crate::polls::NewOption {
            label,
            media_url: media.as_deref(),
        })
        .collect();

    crate::polls::create(
        pool,
        &crate::polls::NewPoll {
            question,
            slug: &slug,
            kind,
            media_url: question_media.as_deref(),
            media_license: media_license.as_deref(),
            country_id,
            person_id,
            party_id,
            options: &new_options,
        },
    )
    .await?;
    Ok(slug)
}

/// A slug derived from the question, capped in length and made unique by
/// appending a counter on collision.
async fn unique_slug(pool: &Pool, question: &str) -> Result<String> {
    let base: String = slugify(question).chars().take(60).collect();
    let base = base.trim_matches('-').to_string();
    let base = if base.is_empty() {
        "poll".to_string()
    } else {
        base
    };

    if !crate::polls::slug_exists(pool, &base).await? {
        return Ok(base);
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !crate::polls::slug_exists(pool, &candidate).await? {
            return Ok(candidate);
        }
        n += 1;
    }
}
