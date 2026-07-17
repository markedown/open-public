//! Data conflicts: when a source disagrees with a value we already hold, the
//! discrepancy is recorded here instead of overwriting a trusted value. An
//! editor reviews each one and marks it resolved. This is the audit trail
//! behind the rule that ingestion never silently overwrites curated data.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// Fields for recording a conflict between two sources over one field of one
/// entity. Either source id may be absent (an incoming value with no stored
/// counterpart, or the reverse).
pub struct NewConflict<'a> {
    pub entity_type: &'a str,
    pub entity_id: Option<i64>,
    pub field: &'a str,
    pub existing_value: Option<&'a str>,
    pub incoming_value: Option<&'a str>,
    pub existing_source_id: Option<i64>,
    pub incoming_source_id: Option<i64>,
}

/// An open conflict, joined to a human label for its entity and to the two
/// source URLs, ready to render on the review page.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub id: i64,
    pub entity_type: String,
    pub entity_label: Option<String>,
    pub field: String,
    pub existing_value: Option<String>,
    pub incoming_value: Option<String>,
    pub existing_source_url: Option<String>,
    pub incoming_source_url: Option<String>,
    pub detected_at: DateTime<Utc>,
}

/// Record a conflict, idempotently: an identical unresolved conflict (same
/// entity, field and incoming value) is not stored twice, so a cross-check can
/// be re-run without piling up duplicates. Returns the row's id.
pub async fn record(pool: &Pool, c: &NewConflict<'_>) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let existing: Option<i64> = sqlx::query_scalar!(
        r#"
        select id from data_conflicts
        where entity_type = $1
          and entity_id is not distinct from $2
          and field = $3
          and incoming_value is not distinct from $4
          and resolved_at is null
        limit 1
        "#,
        c.entity_type,
        c.entity_id,
        c.field,
        c.incoming_value,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let id = if let Some(id) = existing {
        id
    } else {
        sqlx::query_scalar!(
            r#"
            insert into data_conflicts
                (entity_type, entity_id, field, existing_value, incoming_value,
                 existing_source_id, incoming_source_id)
            values ($1, $2, $3, $4, $5, $6, $7)
            returning id
            "#,
            c.entity_type,
            c.entity_id,
            c.field,
            c.existing_value,
            c.incoming_value,
            c.existing_source_id,
            c.incoming_source_id,
        )
        .fetch_one(&mut *tx)
        .await?
    };

    tx.commit().await?;
    Ok(id)
}

/// Open (unresolved) conflicts, newest first, each with a label for its entity
/// and the two source URLs.
pub async fn list_open(pool: &Pool) -> Result<Vec<Conflict>> {
    let rows = sqlx::query_as!(
        Conflict,
        r#"
        select c.id,
               c.entity_type,
               case c.entity_type
                   when 'person' then (select full_name from people where id = c.entity_id)
                   when 'party'  then (select name from parties where id = c.entity_id)
                   else null
               end as "entity_label",
               c.field,
               c.existing_value,
               c.incoming_value,
               es.url as "existing_source_url",
               is2.url as "incoming_source_url",
               c.detected_at
        from data_conflicts c
        left join sources es on es.id = c.existing_source_id
        left join sources is2 on is2.id = c.incoming_source_id
        where c.resolved_at is null
        order by c.detected_at desc, c.id desc
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many conflicts are open, for a review-queue badge.
pub async fn count_open(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "count!" from data_conflicts where resolved_at is null"#,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Mark a conflict resolved (reviewed). Idempotent: resolving an already
/// resolved or unknown id is a no-op. Returns whether a row was affected.
pub async fn resolve(pool: &Pool, id: i64) -> Result<bool> {
    let affected = sqlx::query!(
        "update data_conflicts set resolved_at = now() where id = $1 and resolved_at is null",
        id,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}
