use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use vas_crucible::jwt::claims::Claims;

pub fn make_claims(intent_hash: impl Into<String>, now: i64) -> Claims {
    Claims {
        sub: "agent-1".to_string(),
        exp: now + 45,
        iat: now,
        intent_hash: intent_hash.into(),
        permissions: vec!["execute".to_string()],
    }
}

pub fn sign_hs256(secret: &str, claims: &Claims) -> String {
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("failed to sign test token")
}
