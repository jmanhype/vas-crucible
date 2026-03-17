use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use thiserror::Error;

use crate::jwt::claims::Claims;

const HARD_TTL_SECONDS: i64 = 60;

pub trait TimeSource: Send + Sync {
    fn now(&self) -> i64;
}

#[derive(Debug, Default)]
pub struct SystemTimeSource;

impl TimeSource for SystemTimeSource {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_secs() as i64
    }
}

#[derive(Clone)]
pub enum JwtKeySource {
    Hs256 { secret: Arc<[u8]> },
    Rs256 { public_key_pem: Arc<[u8]> },
}

impl std::fmt::Debug for JwtKeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hs256 { .. } => f.debug_struct("Hs256").finish(),
            Self::Rs256 { .. } => f.debug_struct("Rs256").finish(),
        }
    }
}

#[derive(Clone)]
pub struct JwtVerifier {
    key_source: JwtKeySource,
    time_source: Arc<dyn TimeSource>,
}

impl JwtVerifier {
    pub fn new(key_source: JwtKeySource) -> Self {
        Self {
            key_source,
            time_source: Arc::new(SystemTimeSource),
        }
    }

    pub fn with_time_source(key_source: JwtKeySource, time_source: Arc<dyn TimeSource>) -> Self {
        Self {
            key_source,
            time_source,
        }
    }

    pub fn verify(&self, token: &str, expected_intent_hash: &str) -> Result<Claims, JwtError> {
        let header = decode_header(token).map_err(JwtError::InvalidToken)?;

        let expected_algorithm = match &self.key_source {
            JwtKeySource::Hs256 { .. } => Algorithm::HS256,
            JwtKeySource::Rs256 { .. } => Algorithm::RS256,
        };

        let key = match (&self.key_source, header.alg) {
            (JwtKeySource::Hs256 { secret }, Algorithm::HS256) => DecodingKey::from_secret(secret),
            (JwtKeySource::Rs256 { public_key_pem }, Algorithm::RS256) => {
                DecodingKey::from_rsa_pem(public_key_pem).map_err(JwtError::InvalidKey)?
            }
            (_, actual) => {
                return Err(JwtError::AlgorithmMismatch {
                    expected: expected_algorithm,
                    actual,
                });
            }
        };

        let mut validation = Validation::new(expected_algorithm);
        validation.validate_exp = false;
        validation.required_spec_claims = ["exp", "iat", "sub"]
            .into_iter()
            .map(str::to_owned)
            .collect();

        let token_data = decode::<Claims>(token, &key, &validation).map_err(JwtError::InvalidToken)?;
        let claims = token_data.claims;
        let now = self.time_source.now();

        if claims.exp < now {
            return Err(JwtError::Expired);
        }

        if claims.iat > now {
            return Err(JwtError::IssuedInFuture);
        }

        if claims.exp - claims.iat > HARD_TTL_SECONDS {
            return Err(JwtError::TtlExceeded);
        }

        if claims.intent_hash != expected_intent_hash {
            return Err(JwtError::IntentHashMismatch);
        }

        if claims.permissions.is_empty() {
            return Err(JwtError::MissingPermissions);
        }

        Ok(claims)
    }
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("token decoding failed: {0}")]
    InvalidToken(jsonwebtoken::errors::Error),
    #[error("configured verification key is invalid: {0}")]
    InvalidKey(jsonwebtoken::errors::Error),
    #[error("token algorithm mismatch: expected {expected:?}, got {actual:?}")]
    AlgorithmMismatch {
        expected: Algorithm,
        actual: Algorithm,
    },
    #[error("token is expired")]
    Expired,
    #[error("token issued in the future")]
    IssuedInFuture,
    #[error("token exceeded the 60 second ttl")]
    TtlExceeded,
    #[error("intent_hash claim does not match request intent")]
    IntentHashMismatch,
    #[error("token has no permissions claim")]
    MissingPermissions,
}
