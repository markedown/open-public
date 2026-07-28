//! Proof-of-work captcha (ALTCHA), the open, self-hosted bot deterrent.
//!
//! The browser does a small amount of CPU work to submit a protected action, so
//! minting accounts and casting votes at scale costs real compute. Nothing about
//! it touches a third party: the challenge is signed and verified here with the
//! server secret, the widget is served from our own origin, and no cookie,
//! fingerprint, or behavioural signal is collected. It raises the cost of mass
//! automation; it does not prove one-person-one-vote, which waits for verified
//! uniqueness.
//!
//! This implements the standard ALTCHA SHA-256 scheme, the exact wire format the
//! official self-hosted widget solves: the challenge is `SHA-256(salt + n)` for a
//! secret number `n` the browser recovers by counting up from zero, and the salt
//! carries the expiry. We sign the challenge with an HMAC of the app secret, so a
//! solution can be verified later without the server storing anything but the
//! spent signature (the single-use guard).

use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use chrono::{Duration, Utc};

type HmacSha256 = Hmac<Sha256>;

/// How long a challenge is solvable before it expires.
const TTL_SECONDS: i64 = 600;

/// The upper bound on the secret number. The browser counts from zero to at most
/// this; the average solve is half of it. Small enough to solve in a moment on a
/// phone, large enough that scripted mass submission pays a real cost per action.
const MAX_NUMBER: u64 = 120_000;

/// The challenge JSON the widget fetches (ALTCHA's field names).
#[derive(Serialize)]
struct Challenge {
    algorithm: &'static str,
    challenge: String,
    maxnumber: u64,
    salt: String,
    signature: String,
}

/// What the widget submits back, base64-encoded, in the `altcha` form field.
#[derive(Deserialize)]
struct Solution {
    algorithm: String,
    challenge: String,
    number: u64,
    salt: String,
    signature: String,
}

fn hmac_hex(secret: &[u8], message: &str) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(message.as_bytes());
    Some(hex_encode(&mac.finalize().into_bytes()))
}

fn challenge_hash(salt: &str, number: u64) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(number.to_string().as_bytes());
    hex_encode(&h.finalize())
}

/// A fresh challenge, as the JSON the widget fetches. It is HMAC-signed, so it
/// can be verified later without being stored, and its salt carries the expiry.
pub fn challenge(secret: &[u8]) -> anyhow::Result<String> {
    let mut salt_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let expires = (Utc::now() + Duration::seconds(TTL_SECONDS)).timestamp();
    // The salt carries the expiry as a query param, per ALTCHA, so the deadline
    // is covered by the signature without a separate field.
    let salt = format!("{}?expires={}", hex_encode(&salt_bytes), expires);

    let number = rand::thread_rng().next_u64() % (MAX_NUMBER + 1);
    let challenge = challenge_hash(&salt, number);
    let signature =
        hmac_hex(secret, &challenge).ok_or_else(|| anyhow::anyhow!("captcha hmac key rejected"))?;

    let ch = Challenge {
        algorithm: "SHA-256",
        challenge,
        maxnumber: MAX_NUMBER,
        salt,
        signature,
    };
    Ok(serde_json::to_string(&ch)?)
}

/// The expiry unix timestamp encoded in the salt, if present.
fn salt_expiry(salt: &str) -> Option<i64> {
    salt.split_once("?expires=")
        .and_then(|(_, rest)| rest.split('&').next())
        .and_then(|s| s.parse().ok())
}

/// Whether a submitted solution is valid: cryptographically sound, unexpired,
/// and not a replay. The `payload` is the base64 string the widget puts in the
/// `altcha` form field. A replay (a solution seen before) returns `false`, so
/// the caller checks the database as part of accepting the action.
pub async fn verify(pool: &db::Pool, secret: &[u8], payload: &str) -> anyhow::Result<bool> {
    use base64::Engine;
    let json = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| anyhow::anyhow!("captcha payload is not base64: {e}"))?;
    let sol: Solution = serde_json::from_slice(&json)?;

    if sol.algorithm != "SHA-256" {
        return Ok(false);
    }
    // The signature must be ours: it is the HMAC of the challenge string. Checking
    // this first means a forged challenge never reaches the hash recomputation.
    let expected_sig = match hmac_hex(secret, &sol.challenge) {
        Some(s) => s,
        None => return Ok(false),
    };
    if !constant_time_eq(expected_sig.as_bytes(), sol.signature.as_bytes()) {
        return Ok(false);
    }
    // The number actually solves the challenge.
    if challenge_hash(&sol.salt, sol.number) != sol.challenge {
        return Ok(false);
    }
    // Not past the deadline baked into the salt.
    match salt_expiry(&sol.salt) {
        Some(exp) if Utc::now().timestamp() <= exp => {}
        _ => return Ok(false),
    }

    // Single use: the signature is the replay key. Keep it until just past the
    // challenge's own expiry so a replay within its lifetime is caught.
    let expires_at = Utc::now() + Duration::seconds(TTL_SECONDS);
    db::captcha::mark_used(pool, &sol.signature, expires_at)
        .await
        .map_err(|e| anyhow::anyhow!("captcha replay check failed: {e}"))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Length-independent-ish constant-time compare for the two hex signatures.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// `GET /altcha/challenge`: a fresh challenge for the widget to solve. Must stay
/// reachable while the construction gate is on, since the sign-in page (open
/// while gated) carries the widget.
pub async fn challenge_endpoint(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match challenge(&state.secret) {
        Ok(json) => (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            json,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(?e, "captcha challenge generation failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// Solve a server-issued challenge the way the browser would (count up until
    /// the hash matches) and return the base64 payload the widget would submit.
    fn solve(challenge_json: &str) -> String {
        let v: serde_json::Value = serde_json::from_str(challenge_json).unwrap();
        let salt = v["salt"].as_str().unwrap();
        let target = v["challenge"].as_str().unwrap();
        let maxnumber = v["maxnumber"].as_u64().unwrap();
        let signature = v["signature"].as_str().unwrap();
        let number = (0..=maxnumber)
            .find(|n| challenge_hash(salt, *n) == target)
            .expect("the challenge is solvable within maxnumber");
        let sol = serde_json::json!({
            "algorithm": "SHA-256",
            "challenge": target,
            "number": number,
            "salt": salt,
            "signature": signature,
        });
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&sol).unwrap())
    }

    fn parse(payload: &str) -> Solution {
        let json = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        serde_json::from_slice(&json).unwrap()
    }

    #[test]
    fn a_solved_challenge_carries_the_number_that_hashes_to_the_target() {
        let secret = b"a-server-secret-of-some-length!!";
        let ch = challenge(secret).unwrap();
        let sol = parse(&solve(&ch));
        // The recovered number reproduces the published challenge hash.
        assert_eq!(challenge_hash(&sol.salt, sol.number), sol.challenge);
        // And the challenge is signed with our secret.
        assert_eq!(hmac_hex(secret, &sol.challenge).unwrap(), sol.signature);
    }

    #[test]
    fn a_signature_from_a_different_secret_does_not_match() {
        let ch = challenge(b"secret-one-secret-one-secret-one").unwrap();
        let sol = parse(&solve(&ch));
        // Verified against the wrong secret, the HMAC of the same challenge differs.
        assert_ne!(
            hmac_hex(b"secret-two-secret-two-secret-two", &sol.challenge).unwrap(),
            sol.signature
        );
    }

    #[test]
    fn the_salt_carries_a_future_expiry() {
        let ch = challenge(b"any-secret-any-secret-any-secret").unwrap();
        let v: serde_json::Value = serde_json::from_str(&ch).unwrap();
        let exp = salt_expiry(v["salt"].as_str().unwrap()).unwrap();
        assert!(exp > Utc::now().timestamp());
    }

    #[test]
    fn a_garbage_payload_is_rejected_not_a_panic() {
        // Not base64: the decode fails cleanly rather than panicking.
        assert!(base64::engine::general_purpose::STANDARD
            .decode("not base64!!!")
            .is_err());
    }

    #[test]
    fn constant_time_eq_matches_equal_and_rejects_different() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
