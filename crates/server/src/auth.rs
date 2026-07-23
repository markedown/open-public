use std::sync::OnceLock;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRef, FromRequestParts, OptionalFromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use cookie::time::Duration;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use db::Pool;

type HmacSha256 = Hmac<Sha256>;

const SESSION_COOKIE: &str = "op_session";
const SESSION_TTL_HOURS: i64 = 72;

/// HMAC-hash an email so we can key users on it without storing the plaintext.
/// Returns `None` only if the HMAC cannot accept the key, which does not happen
/// for HMAC-SHA256 (any key length is valid); handlers treat `None` as an error.
pub fn hash_email(email: &str, secret: &[u8]) -> Option<String> {
    let normalized = email.trim().to_lowercase();
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(normalized.as_bytes());
    Some(hex_encode(mac.finalize().into_bytes()))
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Verify a password, always doing the argon2 work even when there is no stored
/// hash. This keeps login timing the same whether or not the account exists, so
/// response time cannot be used to enumerate registered emails.
pub fn verify_password_or_dummy(password: &str, stored: Option<&str>) -> bool {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let dummy = DUMMY.get_or_init(|| hash_password("timing-equalizer").unwrap_or_default());
    let hash = stored.unwrap_or(dummy.as_str());
    verify_password(password, hash).unwrap_or(false)
}

pub fn generate_session_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64_url(&bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex_encode(digest)
}

/// The session cookie set after login. `secure` is true when the site is served
/// over https, so the cookie is never sent in the clear.
pub fn session_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .max_age(Duration::hours(SESSION_TTL_HOURS))
        .build()
}

/// A removal cookie that clears the session on logout.
pub fn clear_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::ZERO)
        .build()
}

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub user_id: i64,
    pub is_admin: bool,
    pub session_id: i64,
}

impl AuthSession {
    /// Resolve the session from the request cookie. `Ok(None)` means no valid
    /// session (missing cookie or unknown/expired token); `Err` is a server
    /// error looking it up.
    async fn load<S>(parts: &mut Parts, state: &S) -> Result<Option<Self>, Response>
    where
        Pool: FromRef<S>,
        S: Send + Sync,
    {
        let pool = Pool::from_ref(state);
        let jar = CookieJar::from_headers(&parts.headers);
        let Some(cookie) = jar.get(SESSION_COOKIE) else {
            return Ok(None);
        };

        let token_hash = hash_token(cookie.value());
        let session = db::sessions::get_by_token_hash(&pool, &token_hash)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;

        Ok(session.map(|s| AuthSession {
            user_id: s.user_id,
            is_admin: s.is_admin,
            session_id: s.id,
        }))
    }
}

/// Required session: redirects to `/login` when absent. Use on routes that only
/// make sense when signed in.
impl<S> FromRequestParts<S> for AuthSession
where
    Pool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Self::load(parts, state).await? {
            Some(session) => Ok(session),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}

/// Optional session: `Option<AuthSession>` yields `None` when signed out (or on
/// a lookup error) instead of redirecting. Use on pages that render for both
/// signed-in and anonymous visitors.
impl<S> OptionalFromRequestParts<S> for AuthSession
where
    Pool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        // An optional session degrades to "signed out" rather than failing the
        // page, which is right: a reader should still see public content. It is
        // recorded though, because a session silently disappearing is otherwise
        // indistinguishable from a visitor who was never signed in.
        Ok(match Self::load(parts, state).await {
            Ok(session) => session,
            Err(_) => {
                tracing::warn!("resolving the session failed; treating the request as signed out");
                None
            }
        })
    }
}

pub async fn start_session(pool: &Pool, user_id: i64) -> Result<String, db::Error> {
    let token = generate_session_token();
    let token_hash = hash_token(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);
    db::sessions::create(pool, user_id, &token_hash, expires_at).await?;
    Ok(token)
}

pub async fn end_session(pool: &Pool, session_id: i64) -> Result<(), db::Error> {
    db::sessions::delete(pool, session_id).await
}

fn base64_url(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[(n & 0x3F) as usize] as char);
        }
    }
    out
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_hash_is_stable_and_secret_dependent() {
        // Trims and lowercases, so the same address hashes the same way.
        let a = hash_email("User@Example.com", b"secret").unwrap();
        let b = hash_email("  user@example.com ", b"secret").unwrap();
        assert_eq!(a, b);
        // A different secret yields a different hash.
        assert_ne!(a, hash_email("user@example.com", b"other").unwrap());
    }

    #[test]
    fn password_roundtrips() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn dummy_verify_is_false_without_a_hash() {
        assert!(!verify_password_or_dummy("anything", None));
    }

    #[test]
    fn dummy_verify_checks_a_real_hash() {
        let hash = hash_password("pw").unwrap();
        assert!(verify_password_or_dummy("pw", Some(&hash)));
        assert!(!verify_password_or_dummy("nope", Some(&hash)));
    }

    #[test]
    fn token_hash_is_deterministic() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
    }

    #[test]
    fn session_tokens_are_unique_and_url_safe() {
        let a = generate_session_token();
        assert_ne!(a, generate_session_token());
        assert_eq!(a.len(), 64); // 48 random bytes -> 64 base64url chars
        assert!(a
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn base64_url_encodes_known_values() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(&[0]), "AA");
        assert_eq!(base64_url(b"foo"), "Zm9v");
    }

    #[test]
    fn session_cookie_is_hardened() {
        let secure = session_cookie("tok".into(), true).encoded().to_string();
        assert!(secure.contains("HttpOnly"));
        assert!(secure.contains("SameSite=Lax"));
        assert!(secure.contains("Secure"));
        // Over plain http the Secure attribute is dropped.
        let insecure = session_cookie("tok".into(), false).encoded().to_string();
        assert!(!insecure.contains("Secure"));
    }
}
