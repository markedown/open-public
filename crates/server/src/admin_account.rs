//! Creating the first administrator.
//!
//! An instance starts with no accounts at all, and every route that could make
//! one is either behind sign-in or behind the construction gate. Without a way
//! in from outside the request path, a fresh deployment has no administrator
//! and no way to appoint one, which is a locked door with the key inside.
//!
//! The address is never stored: it is hashed with the instance secret exactly
//! as registration does, used to find or create the account, and discarded. An
//! account made this way is already verified, because the person running a
//! command on the server has demonstrated more than an email round trip would.

use db::Pool;

use crate::auth;

/// What happened, so the caller can report it without inspecting the database.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The account did not exist and was created as a verified admin.
    Created,
    /// The account existed; it is now a verified admin, and its password was
    /// left alone.
    Promoted,
}

/// Make `email` a verified administrator, creating the account if needed.
///
/// Idempotent: running it twice is the same as running it once. An existing
/// account keeps its password, so this cannot be used to take one over without
/// already having database access, in which case the password was never the
/// protection.
pub async fn ensure_admin(
    pool: &Pool,
    secret: &[u8],
    email: &str,
    password: &str,
) -> anyhow::Result<Outcome> {
    // A typo guard, not validation: the only real test of an address is whether
    // mail reaches it. But an account keyed to a mistyped address can never be
    // signed into, and nothing would say why, so an obvious mistake is caught
    // here rather than becoming a mystery later.
    let trimmed = email.trim();
    let plausible = matches!(trimmed.split('@').collect::<Vec<_>>()[..],
        [local, domain] if !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
            && !domain.ends_with('.'));
    if !plausible {
        anyhow::bail!("that does not look like an address");
    }
    if password.chars().count() < 8 {
        anyhow::bail!("the password must be at least 8 characters");
    }
    let email_hash =
        auth::hash_email(trimmed, secret).ok_or_else(|| anyhow::anyhow!("hashing the address"))?;

    if let Some(user) = db::users::get_by_email_hash(pool, &email_hash).await? {
        db::users::mark_verified(pool, user.id).await?;
        db::users::set_admin(pool, user.id, true).await?;
        return Ok(Outcome::Promoted);
    }

    let password_hash =
        auth::hash_password(password).map_err(|_| anyhow::anyhow!("hashing the password"))?;
    let id = db::users::insert(pool, &email_hash, &password_hash).await?;
    db::users::mark_verified(pool, id).await?;
    db::users::set_admin(pool, id, true).await?;
    Ok(Outcome::Created)
}
