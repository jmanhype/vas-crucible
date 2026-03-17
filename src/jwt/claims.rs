use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub intent_hash: String,
    pub permissions: Vec<String>,
}

