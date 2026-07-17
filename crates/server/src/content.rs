//! Serving content in the reader's language.
//!
//! Content (summaries, headlines, poll questions) is stored in the language it
//! was authored in. [`Localized`] resolves such a field to its published
//! translation in the active language, falling back to the stored original, so
//! the sourced original stays canonical and a missing translation degrades to
//! the original rather than to nothing.

use std::collections::HashMap;

use domain::models::{Education, PersonAttribute, Poll, Statement};

use crate::i18n;

/// An entity's published translations in the active language, ready to overlay
/// on the base-table originals.
pub struct Localized {
    map: HashMap<String, String>,
}

impl Localized {
    /// Load an entity's published translations for the active language. When the
    /// active language is the platform's source language there is nothing to
    /// overlay, so the lookup is skipped.
    pub async fn load(pool: &db::Pool, entity_type: &str, entity_id: i64) -> db::Result<Self> {
        let map =
            db::translations::published_for_entity(pool, entity_type, entity_id, i18n::lang_code())
                .await?;
        Ok(Self { map })
    }

    /// An empty overlay, for entities with no translations to load.
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// The field in the active language: the published translation if there is
    /// one, else the given original.
    pub fn get<'a>(&'a self, field: &str, original: Option<&'a str>) -> Option<&'a str> {
        self.map.get(field).map(String::as_str).or(original)
    }

    /// Whether `field` is being shown as a translation rather than the original.
    pub fn is_translated(&self, field: &str) -> bool {
        self.map.contains_key(field)
    }
}

/// Overlay published translations for the active language onto a list of
/// statements, replacing each quote with its translation where one exists. The
/// original stays reachable through the statement's own source link, so no
/// separate disclosure is shown. One batched query for the whole list.
pub async fn localize_statements(pool: &db::Pool, statements: &mut [Statement]) -> db::Result<()> {
    if statements.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = statements.iter().map(|s| s.id).collect();
    let tr = db::translations::published_for_entities(pool, "statement", &ids, i18n::lang_code())
        .await?;
    for s in statements.iter_mut() {
        if let Some(t) = tr.get(&(s.id, "text_original".to_string())) {
            s.text_original.clone_from(t);
        }
    }
    Ok(())
}

/// Overlay published translations for the active language onto a poll: its
/// question and each option's label. The original question and labels are not
/// disclosed here, since a poll is a call to participate rather than a sourced
/// quote; showing it in the reader's language is the point.
pub async fn localize_poll(pool: &db::Pool, poll: &mut Poll) -> db::Result<()> {
    let lang = i18n::lang_code();
    let q = db::translations::published_for_entity(pool, "poll", poll.id, lang).await?;
    if let Some(t) = q.get("question") {
        poll.question.clone_from(t);
    }
    let ids: Vec<i64> = poll.options.iter().map(|o| o.id).collect();
    if !ids.is_empty() {
        let opt = db::translations::published_for_entities(pool, "poll_option", &ids, lang).await?;
        for o in poll.options.iter_mut() {
            if let Some(t) = opt.get(&(o.id, "label".to_string())) {
                o.label.clone_from(t);
            }
        }
    }
    Ok(())
}

/// Localize each poll in a list (for the poll previews on a country, person, or
/// party page).
pub async fn localize_polls(pool: &db::Pool, polls: &mut [Poll]) -> db::Result<()> {
    for p in polls.iter_mut() {
        localize_poll(pool, p).await?;
    }
    Ok(())
}

/// Overlay published translations for the active language onto a person's
/// attributes (the value of each occupation, ideology or religion). Short terms,
/// so no original is disclosed inline; the source link carries provenance.
pub async fn localize_attributes(
    pool: &db::Pool,
    attributes: &mut [PersonAttribute],
) -> db::Result<()> {
    if attributes.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = attributes.iter().map(|a| a.id).collect();
    let tr =
        db::translations::published_for_entities(pool, "person_attribute", &ids, i18n::lang_code())
            .await?;
    for a in attributes.iter_mut() {
        if let Some(t) = tr.get(&(a.id, "value".to_string())) {
            a.value.clone_from(t);
        }
    }
    Ok(())
}

/// Overlay published translations onto a person's education (institution, degree
/// and field), for the active language.
pub async fn localize_education(pool: &db::Pool, education: &mut [Education]) -> db::Result<()> {
    if education.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = education.iter().map(|e| e.id).collect();
    let tr =
        db::translations::published_for_entities(pool, "person_education", &ids, i18n::lang_code())
            .await?;
    for e in education.iter_mut() {
        if let Some(t) = tr.get(&(e.id, "institution".to_string())) {
            e.institution.clone_from(t);
        }
        if let (Some(t), Some(d)) = (tr.get(&(e.id, "degree".to_string())), e.degree.as_mut()) {
            d.clone_from(t);
        }
        if let (Some(t), Some(f)) = (tr.get(&(e.id, "field".to_string())), e.field.as_mut()) {
            f.clone_from(t);
        }
    }
    Ok(())
}
