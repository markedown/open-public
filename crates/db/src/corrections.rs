//! Visitor-reported errors and their closure by an editor.
//!
//! A report is not a fact row: it names a page and says what reads wrong there.
//! The correction it asks for is made through the normal editing flow, where it
//! carries a source; the report only records that someone asked and what the
//! editor decided. A contact address is personal data held for one conversation,
//! so closing a report deletes it in the same statement that decides it.

use chrono::{DateTime, Utc};

use crate::{Pool, Result};

/// A report as shown in the review queue.
#[derive(Debug, Clone)]
pub struct Correction {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub page: String,
    pub description: String,
    pub source_url: Option<String>,
    /// Held only while the report is open; closing nulls it.
    pub contact_email: Option<String>,
    pub status: String,
    pub resolution_note: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// A validated report from the public form.
pub struct NewCorrection<'a> {
    pub page: &'a str,
    pub description: &'a str,
    pub source_url: Option<&'a str>,
    pub contact_email: Option<&'a str>,
}

/// Insert a report as `open`. Returns its id.
pub async fn create(pool: &Pool, c: &NewCorrection<'_>) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        insert into corrections (page, description, source_url, contact_email)
        values ($1, $2, $3, $4)
        returning id
        "#,
        c.page,
        c.description,
        c.source_url,
        c.contact_email,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Open reports, oldest first, capped so one queue page stays a page.
pub async fn list_open(pool: &Pool) -> Result<Vec<Correction>> {
    let rows = sqlx::query_as!(
        Correction,
        r#"
        select id, created_at, page, description, source_url,
               contact_email, status, resolution_note, resolved_at
        from corrections
        where status = 'open'
        order by created_at, id
        limit 200
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Count of open reports (the admin hub badge).
pub async fn open_count(pool: &Pool) -> Result<i64> {
    let n =
        sqlx::query_scalar!(r#"select count(*) as "n!" from corrections where status = 'open'"#)
            .fetch_one(pool)
            .await?;
    Ok(n)
}

/// Close a report as `resolved` or `rejected`, with an optional note for the
/// record. The contact address is nulled in the same statement, because closing
/// is the moment it stops being needed. A no-op once the report is closed, so a
/// stale queue page cannot overwrite a decision already made.
pub async fn close(pool: &Pool, id: i64, status: &str, note: Option<&str>) -> Result<()> {
    sqlx::query!(
        r#"
        update corrections
        set status = $2, resolution_note = $3, resolved_at = now(), contact_email = null
        where id = $1 and status = 'open'
        "#,
        id,
        status,
        note,
    )
    .execute(pool)
    .await?;
    Ok(())
}
