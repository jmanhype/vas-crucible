use std::sync::Arc;

use vas_crucible::jwt::verifier::{JwtError, JwtKeySource, JwtVerifier, TimeSource};

use crate::common::{make_claims, sign_hs256};

struct FixedTime(i64);

impl TimeSource for FixedTime {
    fn now(&self) -> i64 {
        self.0
    }
}

#[test]
fn verifies_valid_jwt() {
    let now = 1_700_000_000_i64;
    let intent_hash = "intent-valid";
    let verifier = JwtVerifier::with_time_source(
        JwtKeySource::Hs256 {
            secret: Arc::<[u8]>::from(b"secret".to_vec()),
        },
        Arc::new(FixedTime(now)),
    );
    let token = sign_hs256("secret", &make_claims(intent_hash, now));

    let claims = verifier.verify(&token, intent_hash).expect("token should verify");
    assert_eq!(claims.sub, "agent-1");
}

#[test]
fn rejects_expired_jwt() {
    let now = 1_700_000_100_i64;
    let intent_hash = "intent-expired";
    let verifier = JwtVerifier::with_time_source(
        JwtKeySource::Hs256 {
            secret: Arc::<[u8]>::from(b"secret".to_vec()),
        },
        Arc::new(FixedTime(now)),
    );
    let token = sign_hs256("secret", &make_claims(intent_hash, now - 100));

    let error = verifier.verify(&token, intent_hash).expect_err("token should be expired");
    assert!(matches!(error, JwtError::Expired));
}

#[test]
fn rejects_invalid_signature() {
    let now = 1_700_000_000_i64;
    let intent_hash = "intent-signature";
    let verifier = JwtVerifier::with_time_source(
        JwtKeySource::Hs256 {
            secret: Arc::<[u8]>::from(b"secret".to_vec()),
        },
        Arc::new(FixedTime(now)),
    );
    let token = sign_hs256("other-secret", &make_claims(intent_hash, now));

    let error = verifier
        .verify(&token, intent_hash)
        .expect_err("token should fail signature validation");
    assert!(matches!(error, JwtError::InvalidToken(_)));
}

#[test]
fn rejects_intent_hash_mismatch() {
    let now = 1_700_000_000_i64;
    let verifier = JwtVerifier::with_time_source(
        JwtKeySource::Hs256 {
            secret: Arc::<[u8]>::from(b"secret".to_vec()),
        },
        Arc::new(FixedTime(now)),
    );
    let token = sign_hs256("secret", &make_claims("expected-intent", now));

    let error = verifier
        .verify(&token, "actual-intent")
        .expect_err("token should fail intent hash validation");
    assert!(matches!(error, JwtError::IntentHashMismatch));
}

