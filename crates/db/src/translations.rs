//! Translations of content into other languages (see migration 0024).
//!
//! The gettext catalogs cover the fixed UI strings; this covers content that
//! lives in the database in the language it was authored in. Readers see a
//! translation only when it is `published`; otherwise the page falls back to the
//! canonical original on the base table.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// The content fields that may be translated, as `(entity_type, field)`. The
/// application reads and writes translations only for these pairs, so a mistyped
/// handler cannot create a translation for a field that is not content. Short
/// controlled-vocabulary values (a country's government type) are translated
/// through the message catalog instead and are deliberately absent here.
pub const REGISTRY: &[(&str, &str)] = &[
    ("person", "summary"),
    ("party", "summary"),
    ("country", "summary"),
    ("statement", "text_original"),
    ("news_item", "headline"),
    ("news_item", "our_summary"),
    ("poll", "question"),
    ("poll_option", "label"),
    ("topic", "name"),
    ("election", "name"),
    ("election", "description"),
    ("alliance", "name"),
    ("alliance", "summary"),
    ("outlet", "summary"),
    ("person_attribute", "value"),
    ("person_education", "institution"),
    ("person_education", "degree"),
    ("person_education", "field"),
];

/// Whether `(entity_type, field)` is a known translatable field.
pub fn is_registered(entity_type: &str, field: &str) -> bool {
    REGISTRY
        .iter()
        .any(|(t, f)| *t == entity_type && *f == field)
}

/// Fields for creating or updating a translation.
pub struct NewTranslation<'a> {
    pub entity_type: &'a str,
    pub entity_id: i64,
    pub field: &'a str,
    pub lang: &'a str,
    pub text: &'a str,
    /// `'human'` or `'machine'`.
    pub origin: &'a str,
    /// `'draft'` or `'published'`.
    pub status: &'a str,
    /// The language the text was translated from, when known.
    pub source_lang: Option<&'a str>,
}

/// A draft translation awaiting review, with the context an admin needs to
/// judge it. The original text is not joined here (the entity is polymorphic);
/// the admin page resolves it from the named entity.
#[derive(Debug, Clone)]
pub struct Draft {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub field: String,
    pub lang: String,
    pub text: String,
    pub origin: String,
    pub source_lang: Option<String>,
    pub translated_at: DateTime<Utc>,
}

/// Insert or update a translation, keyed on `(entity_type, entity_id, field,
/// lang)`. Re-running with the same key replaces the text and resets its review
/// state to whatever `status` is passed, so a regenerated machine draft returns
/// to the queue. Returns the row id.
pub async fn upsert(pool: &Pool, t: &NewTranslation<'_>) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into translations
            (entity_type, entity_id, field, lang, text, origin, status, source_lang, translated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, now())
        on conflict (entity_type, entity_id, field, lang) do update set
            text          = excluded.text,
            origin        = excluded.origin,
            status        = excluded.status,
            source_lang   = excluded.source_lang,
            translated_at = now(),
            reviewed_by   = null,
            reviewed_at   = null
        returning id
        "#,
        t.entity_type,
        t.entity_id,
        t.field,
        t.lang,
        t.text,
        t.origin,
        t.status,
        t.source_lang,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Every published translation an entity has in one language, keyed by field.
/// Callers overlay these onto the base-table values, falling back to the
/// original where a field is absent.
pub async fn published_for_entity(
    pool: &Pool,
    entity_type: &str,
    entity_id: i64,
    lang: &str,
) -> Result<HashMap<String, String>> {
    let rows = sqlx::query!(
        r#"
        select field, text from translations
        where entity_type = $1 and entity_id = $2 and lang = $3 and status = 'published'
        "#,
        entity_type,
        entity_id,
        lang,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.field, r.text)).collect())
}

/// Published translations for many entities of one type in one language, keyed
/// `(entity_id, field)`. One query for a whole list page, so rendering stays
/// free of N+1 lookups.
pub async fn published_for_entities(
    pool: &Pool,
    entity_type: &str,
    ids: &[i64],
    lang: &str,
) -> Result<HashMap<(i64, String), String>> {
    let rows = sqlx::query!(
        r#"
        select entity_id, field, text from translations
        where entity_type = $1 and entity_id = any($2) and lang = $3 and status = 'published'
        "#,
        entity_type,
        ids,
        lang,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ((r.entity_id, r.field), r.text))
        .collect())
}

/// Drafts awaiting review, oldest first, for the admin queue. `limit` caps it.
pub async fn pending(pool: &Pool, limit: i64) -> Result<Vec<Draft>> {
    let rows = sqlx::query_as!(
        Draft,
        r#"
        select id, entity_type, entity_id, field, lang, text, origin, source_lang, translated_at
        from translations
        where status = 'draft'
        order by translated_at, id
        limit $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many drafts await review, for the admin badge.
pub async fn pending_count(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from translations where status = 'draft'"#
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Fetch one translation row by id (the admin editor loads it to edit).
pub async fn get(pool: &Pool, id: i64) -> Result<Option<Draft>> {
    let row = sqlx::query_as!(
        Draft,
        r#"
        select id, entity_type, entity_id, field, lang, text, origin, source_lang, translated_at
        from translations where id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Replace a translation's text (an admin edit), keeping it in review.
pub async fn set_text(pool: &Pool, id: i64, text: &str) -> Result<()> {
    sqlx::query!(
        r#"update translations set text = $2, translated_at = now() where id = $1"#,
        id,
        text,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Publish a translation: readers now see it. Records who approved it and when.
pub async fn publish(pool: &Pool, id: i64, reviewer_id: i64) -> Result<()> {
    sqlx::query!(
        r#"
        update translations
        set status = 'published', reviewed_by = $2, reviewed_at = now()
        where id = $1
        "#,
        id,
        reviewer_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Discard a draft translation.
pub async fn discard(pool: &Pool, id: i64) -> Result<()> {
    sqlx::query!(r#"delete from translations where id = $1"#, id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The original (source-language) value of a translatable field, so a reviewer
/// can judge a draft against it. Returns `None` for an unregistered field or a
/// missing row. Uses a runtime query keyed by the registry, so the table and
/// column are chosen from a fixed set, never from caller input directly.
pub async fn original(
    pool: &Pool,
    entity_type: &str,
    entity_id: i64,
    field: &str,
) -> Result<Option<String>> {
    let sql = match (entity_type, field) {
        ("person", "summary") => "select summary from people where id = $1",
        ("party", "summary") => "select summary from parties where id = $1",
        ("country", "summary") => "select summary from countries where id = $1",
        ("statement", "text_original") => "select text_original from statements where id = $1",
        ("news_item", "headline") => "select headline from news_items where id = $1",
        ("news_item", "our_summary") => "select our_summary from news_items where id = $1",
        ("poll", "question") => "select question from polls where id = $1",
        ("poll_option", "label") => "select label from poll_options where id = $1",
        ("topic", "name") => "select name from topics where id = $1",
        ("election", "name") => "select name from elections where id = $1",
        ("election", "description") => "select description from elections where id = $1",
        ("alliance", "name") => "select name from alliances where id = $1",
        ("alliance", "summary") => "select summary from alliances where id = $1",
        ("outlet", "summary") => "select summary from outlets where id = $1",
        ("person_attribute", "value") => "select value from person_attributes where id = $1",
        ("person_education", "institution") => {
            "select institution from person_education where id = $1"
        }
        ("person_education", "degree") => "select degree from person_education where id = $1",
        ("person_education", "field") => "select field from person_education where id = $1",
        _ => return Ok(None),
    };
    let value: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(sql)
        .bind(entity_id)
        .fetch_optional(pool)
        .await?;
    Ok(value.flatten())
}
