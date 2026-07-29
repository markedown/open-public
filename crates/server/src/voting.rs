//! Blind-signature crypto for anonymous voting (RFC 9474
//! RSABSSA-SHA384-PSS-Randomized). See docs/anonymous-voting.md.
//!
//! Each poll has its own issuer keypair. The server blind-signs a client's token
//! request without seeing the token, and later verifies the unblinded signature
//! over the revealed token when it is spent. Because a token signed by poll P's
//! key verifies only under P's key, a token cannot be moved between polls. Keys
//! are stored DER-encoded by `db::voting`; this module holds the crypto.

use blind_rsa_signatures::{
    DefaultRng, KeyPair, MessageRandomizer, PublicKey, Randomized, SecretKey, Sha384, Signature,
    PSS,
};

/// The RFC 9474 variant we use everywhere: SHA-384, PSS, randomized message.
type IssuerPublicKey = PublicKey<Sha384, PSS, Randomized>;
type IssuerSecretKey = SecretKey<Sha384, PSS, Randomized>;

/// RSA-2048 is the standard blind-signature modulus (as in Privacy Pass / PAT):
/// small signatures, fast verify, ample security for a per-poll, per-close key.
const MODULUS_BITS: usize = 2048;

/// Generate a fresh per-poll issuer keypair, returned DER-encoded as
/// `(public, private)`. Keygen is ~85 ms of CPU, so callers run it off the async
/// runtime (see `ensure_issuer_key`).
pub fn generate_issuer_keypair() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let kp = KeyPair::<Sha384, PSS, Randomized>::generate(&mut DefaultRng, MODULUS_BITS)
        .map_err(|e| anyhow::anyhow!("issuer keygen failed: {e}"))?;
    let public = kp
        .pk
        .to_der()
        .map_err(|e| anyhow::anyhow!("public key encode failed: {e}"))?;
    let private = kp
        .sk
        .to_der()
        .map_err(|e| anyhow::anyhow!("private key encode failed: {e}"))?;
    Ok((public, private))
}

/// Blind-sign a client's blinded token request. The server never sees the token
/// inside `blind_msg`; that is what makes the eventual vote unlinkable.
pub fn blind_sign(private_key_der: &[u8], blind_msg: &[u8]) -> anyhow::Result<Vec<u8>> {
    let sk = IssuerSecretKey::from_der(private_key_der)
        .map_err(|e| anyhow::anyhow!("private key decode failed: {e}"))?;
    let sig = sk
        .blind_sign(blind_msg)
        .map_err(|e| anyhow::anyhow!("blind sign failed: {e}"))?;
    Ok(sig.0)
}

/// Whether a spent token's signature is valid under a poll's public key. A bad
/// key, bad randomizer, or bad signature is simply invalid, never an error the
/// caller must handle specially.
pub fn verify_token(
    public_key_der: &[u8],
    token: &[u8],
    signature: &[u8],
    msg_randomizer: Option<&[u8]>,
) -> bool {
    let Ok(pk) = IssuerPublicKey::from_der(public_key_der) else {
        return false;
    };
    let rnd = match msg_randomizer {
        Some(r) => {
            let Ok(a) = <[u8; 32]>::try_from(r) else {
                return false;
            };
            Some(MessageRandomizer(a))
        }
        None => None,
    };
    pk.verify(&Signature(signature.to_vec()), rnd, token)
        .is_ok()
}

/// Ensure a poll has an issuer keypair, generating and storing one if absent.
/// Idempotent: a poll's key never changes once set. Keygen runs on a blocking
/// thread so it does not stall the async runtime.
pub async fn ensure_issuer_key(pool: &db::Pool, poll_id: i64) -> anyhow::Result<()> {
    if db::voting::has_issuer_key(pool, poll_id).await? {
        return Ok(());
    }
    let (public, private) = tokio::task::spawn_blocking(generate_issuer_keypair).await??;
    db::voting::insert_issuer_key(pool, poll_id, &public, &private).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blind_rsa_signatures::PublicKey as BrsaPk;

    /// The full loop with serialized keys: generate, blind, sign, finalize,
    /// verify. Proves the DER round-trip and the spend-side verify agree.
    #[test]
    fn the_issued_token_verifies_and_a_wrong_one_does_not() {
        let (pk_der, sk_der) = generate_issuer_keypair().unwrap();
        let pk = BrsaPk::<Sha384, PSS, Randomized>::from_der(&pk_der).unwrap();

        // Client: blind a random token.
        let token: [u8; 32] = [7u8; 32];
        let blinding = pk.blind(&mut DefaultRng, token).unwrap();

        // Server: blind-sign with the stored private DER.
        let blind_sig_bytes =
            blind_sign(&sk_der, AsRef::<[u8]>::as_ref(&blinding.blind_message)).unwrap();
        let blind_sig = blind_rsa_signatures::BlindSignature(blind_sig_bytes);

        // Client: finalize to a signature over the token.
        let sig = pk.finalize(&blind_sig, &blinding, token).unwrap();
        let rnd = blinding.msg_randomizer.map(|r| r.0.to_vec());

        // Server (spend): the token verifies; a different token does not.
        assert!(verify_token(&pk_der, &token, &sig.0, rnd.as_deref()));
        let other_token = [9u8; 32];
        assert!(!verify_token(&pk_der, &other_token, &sig.0, rnd.as_deref()));
        // A signature under a different poll's key does not verify.
        let (other_pk_der, _) = generate_issuer_keypair().unwrap();
        assert!(!verify_token(&other_pk_der, &token, &sig.0, rnd.as_deref()));
    }
}
