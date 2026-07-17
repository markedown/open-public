//! User-submitted polls and their moderation lifecycle.
//!
//! A submission moves through `pending_ai` (awaiting the automated screen) to
//! either `ai_rejected` (a clear policy violation) or `pending_admin` (awaiting
//! a human), and from there to `approved` (a real poll is created) or
//! `rejected`. A submission marked a violation counts toward a ban: once a
//! submitter reaches [`STRIKE_LIMIT`] violations the account is suspended.

use chrono::{DateTime, Utc};
use domain::slug::slugify;

use crate::{Pool, Result};

/// How many policy violations permanently suspend an account.
pub const STRIKE_LIMIT: i64 = 3;

/// A submission as shown in a review queue or to its author, with the country
/// and the question image resolved for display.
#[derive(Debug, Clone)]
pub struct Submission {
    pub id: i64,
    pub submitter_id: i64,
    pub country_id: i64,
    pub country_slug: String,
    pub country_name: String,
    pub question: String,
    pub kind: String,
    /// Content hash of the question image, if any (forms a `/media/{sha}` URL).
    pub question_sha: Option<String>,
    pub status: String,
    pub ai_decision: Option<String>,
    pub ai_reason: Option<String>,
    pub ai_categories: Option<Vec<String>>,
    pub is_violation: bool,
    pub admin_note: Option<String>,
    pub published_poll_id: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// One option of a submission, with its image resolved for display.
#[derive(Debug, Clone)]
pub struct SubmissionOption {
    pub id: i64,
    pub label: String,
    pub position: i32,
    pub asset_sha: Option<String>,
}

/// Fields for a new submission (the poll itself, minus its options).
pub struct NewSubmission<'a> {
    pub submitter_id: i64,
    pub country_id: i64,
    pub question: &'a str,
    pub kind: &'a str,
    pub question_asset_id: Option<i64>,
}

/// One option to submit: a label and an optional uploaded image.
pub struct NewSubmissionOption {
    pub label: String,
    pub asset_id: Option<i64>,
}

/// Insert a submission and its options in one transaction. Returns the id.
pub async fn create(
    pool: &Pool,
    sub: &NewSubmission<'_>,
    options: &[NewSubmissionOption],
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar!(
        r#"
        insert into poll_submissions (submitter_id, country_id, question, kind, question_asset_id)
        values ($1, $2, $3, $4, $5)
        returning id
        "#,
        sub.submitter_id,
        sub.country_id,
        sub.question,
        sub.kind,
        sub.question_asset_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    for (i, o) in options.iter().enumerate() {
        sqlx::query!(
            "insert into poll_submission_options (submission_id, label, position, asset_id) values ($1, $2, $3, $4)",
            id,
            o.label,
            (i as i32) + 1,
            o.asset_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(id)
}

/// Submissions in a given status, oldest first. `limit` caps a batch.
pub async fn by_status(pool: &Pool, status: &str, limit: i64) -> Result<Vec<Submission>> {
    let rows = sqlx::query_as!(
        Submission,
        r#"
        select s.id, s.submitter_id, s.country_id,
               c.slug as country_slug, c.name as country_name,
               s.question, s.kind, qa.sha256 as "question_sha?",
               s.status, s.ai_decision, s.ai_reason, s.ai_categories,
               s.is_violation, s.admin_note, s.published_poll_id, s.created_at
        from poll_submissions s
        join countries c on c.id = s.country_id
        left join assets qa on qa.id = s.question_asset_id
        where s.status = $1
        order by s.created_at, s.id
        limit $2
        "#,
        status,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The admin review queue: submissions the automated screen has passed.
pub async fn pending_admin(pool: &Pool) -> Result<Vec<Submission>> {
    by_status(pool, "pending_admin", 200).await
}

/// Count of submissions awaiting an admin (for the admin hub badge).
pub async fn pending_admin_count(pool: &Pool) -> Result<i64> {
    let n = sqlx::query_scalar!(
        r#"select count(*) as "n!" from poll_submissions where status = 'pending_admin'"#
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// A submitter's own submissions, newest first, so they can see each one's state.
pub async fn for_submitter(pool: &Pool, user_id: i64) -> Result<Vec<Submission>> {
    let rows = sqlx::query_as!(
        Submission,
        r#"
        select s.id, s.submitter_id, s.country_id,
               c.slug as country_slug, c.name as country_name,
               s.question, s.kind, qa.sha256 as "question_sha?",
               s.status, s.ai_decision, s.ai_reason, s.ai_categories,
               s.is_violation, s.admin_note, s.published_poll_id, s.created_at
        from poll_submissions s
        join countries c on c.id = s.country_id
        left join assets qa on qa.id = s.question_asset_id
        where s.submitter_id = $1
        order by s.created_at desc, s.id desc
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// A single submission by id.
pub async fn get(pool: &Pool, id: i64) -> Result<Option<Submission>> {
    let row = sqlx::query_as!(
        Submission,
        r#"
        select s.id, s.submitter_id, s.country_id,
               c.slug as country_slug, c.name as country_name,
               s.question, s.kind, qa.sha256 as "question_sha?",
               s.status, s.ai_decision, s.ai_reason, s.ai_categories,
               s.is_violation, s.admin_note, s.published_poll_id, s.created_at
        from poll_submissions s
        join countries c on c.id = s.country_id
        left join assets qa on qa.id = s.question_asset_id
        where s.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The options of a submission, ordered, with each image resolved.
pub async fn options(pool: &Pool, submission_id: i64) -> Result<Vec<SubmissionOption>> {
    let rows = sqlx::query_as!(
        SubmissionOption,
        r#"
        select o.id, o.label, o.position, a.sha256 as "asset_sha?"
        from poll_submission_options o
        left join assets a on a.id = o.asset_id
        where o.submission_id = $1
        order by o.position
        "#,
        submission_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The option labels of a submission (the text the automated screen reads).
pub async fn option_labels(pool: &Pool, submission_id: i64) -> Result<Vec<String>> {
    let rows = sqlx::query_scalar!(
        "select label from poll_submission_options where submission_id = $1 order by position",
        submission_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Record that the automated screen allowed a submission: it moves to the admin
/// queue. A no-op if the submission is no longer `pending_ai`.
pub async fn record_ai_allow(
    pool: &Pool,
    id: i64,
    model: &str,
    reason: Option<&str>,
    categories: &[String],
) -> Result<()> {
    sqlx::query!(
        r#"
        update poll_submissions
        set status = 'pending_admin', ai_decision = 'allow', ai_model = $2,
            ai_reason = $3, ai_categories = $4, ai_reviewed_at = now(), updated_at = now()
        where id = $1 and status = 'pending_ai'
        "#,
        id,
        model,
        reason,
        categories,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Record that the automated screen rejected a submission as a clear policy
/// violation. The submission becomes `ai_rejected` and a violation, and if the
/// submitter has now reached [`STRIKE_LIMIT`] the account is suspended. Returns
/// whether the submitter was banned. A no-op if not `pending_ai`.
pub async fn record_ai_reject(
    pool: &Pool,
    id: i64,
    model: &str,
    reason: Option<&str>,
    categories: &[String],
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let submitter = sqlx::query_scalar!(
        r#"
        update poll_submissions
        set status = 'ai_rejected', ai_decision = 'reject', ai_model = $2,
            ai_reason = $3, ai_categories = $4, is_violation = true,
            ai_reviewed_at = now(), updated_at = now()
        where id = $1 and status = 'pending_ai'
        returning submitter_id
        "#,
        id,
        model,
        reason,
        categories,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(submitter_id) = submitter else {
        tx.rollback().await?;
        return Ok(false);
    };
    let banned = enforce_ban(&mut tx, submitter_id).await?;
    tx.commit().await?;
    Ok(banned)
}

/// Note a failed automated-screen attempt and return the new attempt count, so
/// the caller can fall back to human review after repeated failures.
pub async fn bump_ai_attempt(pool: &Pool, id: i64) -> Result<i32> {
    let n = sqlx::query_scalar!(
        r#"
        update poll_submissions set ai_attempts = ai_attempts + 1, updated_at = now()
        where id = $1 returning ai_attempts
        "#,
        id,
    )
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Route a submission to the admin queue without an automated verdict (used when
/// the reviewer is unavailable, so nothing gets stuck). A no-op if not
/// `pending_ai`.
pub async fn defer_to_admin(pool: &Pool, id: i64, reason: &str) -> Result<()> {
    sqlx::query!(
        r#"
        update poll_submissions
        set status = 'pending_admin', ai_reason = $2, ai_reviewed_at = now(), updated_at = now()
        where id = $1 and status = 'pending_ai'
        "#,
        id,
        reason,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Approve a submission: create a real poll from it (its uploaded images become
/// public at this moment), and record the poll's id on the submission. Returns
/// the new poll's slug, or `None` if the submission was not awaiting an admin
/// (already handled), so a double submit cannot create a duplicate poll.
pub async fn approve(pool: &Pool, id: i64, admin_id: i64) -> Result<Option<String>> {
    let mut tx = pool.begin().await?;

    let sub = sqlx::query!(
        r#"
        select submitter_id, country_id, question, kind, question_asset_id
        from poll_submissions where id = $1 and status = 'pending_admin' for update
        "#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(sub) = sub else {
        tx.rollback().await?;
        return Ok(None);
    };

    let question_url = match sub.question_asset_id {
        Some(aid) => sqlx::query_scalar!("select sha256 from assets where id = $1", aid)
            .fetch_optional(&mut *tx)
            .await?
            .map(|sha| format!("/media/{sha}")),
        None => None,
    };

    let opts = sqlx::query!(
        r#"
        select o.label, o.asset_id, a.sha256 as "sha?"
        from poll_submission_options o
        left join assets a on a.id = o.asset_id
        where o.submission_id = $1 order by o.position
        "#,
        id,
    )
    .fetch_all(&mut *tx)
    .await?;

    // Generate a unique poll slug from the question, within the transaction.
    let base: String = slugify(&sub.question).chars().take(60).collect();
    let base = base.trim_matches('-').to_string();
    let base = if base.is_empty() {
        "poll".to_string()
    } else {
        base
    };
    let mut slug = base.clone();
    let mut n = 2;
    loop {
        let taken = sqlx::query_scalar!(
            r#"select exists(select 1 from polls where slug = $1) as "e!""#,
            slug,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !taken {
            break;
        }
        slug = format!("{base}-{n}");
        n += 1;
    }

    // Community-submitted polls carry our-hosted images, so they have no license
    // string; `created_by` marks their origin.
    let poll_id: i64 = sqlx::query_scalar!(
        r#"
        insert into polls (question, slug, kind, media_url, country_id, created_by)
        values ($1, $2, $3, $4, $5, 'community')
        returning id
        "#,
        sub.question,
        slug,
        sub.kind,
        question_url,
        sub.country_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    for (i, o) in opts.iter().enumerate() {
        let url = o.sha.as_ref().map(|s| format!("/media/{s}"));
        sqlx::query!(
            "insert into poll_options (poll_id, label, position, media_url) values ($1, $2, $3, $4)",
            poll_id,
            o.label,
            (i as i32) + 1,
            url,
        )
        .execute(&mut *tx)
        .await?;
        if let Some(aid) = o.asset_id {
            sqlx::query!("update assets set published = true where id = $1", aid)
                .execute(&mut *tx)
                .await?;
        }
    }
    if let Some(aid) = sub.question_asset_id {
        sqlx::query!("update assets set published = true where id = $1", aid)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query!(
        r#"
        update poll_submissions
        set status = 'approved', published_poll_id = $2, reviewed_by = $3,
            reviewed_at = now(), updated_at = now()
        where id = $1
        "#,
        id,
        poll_id,
        admin_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(slug))
}

/// Reject a submission from the admin queue. When `violation` is set it counts
/// toward a ban (and may suspend the account); a plain decline does not. Returns
/// whether the submitter was banned. A no-op if not `pending_admin`.
pub async fn reject(
    pool: &Pool,
    id: i64,
    admin_id: i64,
    note: Option<&str>,
    violation: bool,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let submitter = sqlx::query_scalar!(
        r#"
        update poll_submissions
        set status = 'rejected', is_violation = $2, admin_note = $3,
            reviewed_by = $4, reviewed_at = now(), updated_at = now()
        where id = $1 and status = 'pending_admin'
        returning submitter_id
        "#,
        id,
        violation,
        note,
        admin_id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(submitter_id) = submitter else {
        tx.rollback().await?;
        return Ok(false);
    };
    let banned = if violation {
        enforce_ban(&mut tx, submitter_id).await?
    } else {
        false
    };
    tx.commit().await?;
    Ok(banned)
}

/// Suspend the submitter if they have now reached the violation threshold. Runs
/// inside the caller's transaction so the ban and the violation commit together.
/// Returns whether a ban was applied.
async fn enforce_ban(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    submitter_id: i64,
) -> Result<bool> {
    let violations = sqlx::query_scalar!(
        r#"select count(*) as "n!" from poll_submissions where submitter_id = $1 and is_violation"#,
        submitter_id,
    )
    .fetch_one(&mut **tx)
    .await?;
    if violations < STRIKE_LIMIT {
        return Ok(false);
    }
    let updated = sqlx::query!(
        r#"
        update users
        set banned_at = now(), ban_reason = 'repeated poll-submission policy violations'
        where id = $1 and banned_at is null
        "#,
        submitter_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(updated.rows_affected() > 0)
}
