use crate::error::AppError;
use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn create_token(secret: &str, ttl_hours: i64) -> Result<String, AppError> {
    let exp = (Utc::now() + Duration::hours(ttl_hours)).timestamp() as usize;
    let claims = Claims {
        sub: "admin".into(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(e.to_string()))
}

pub fn verify_token(secret: &str, token: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized("Invalid token".into()))
}

pub struct AuthUser {
    #[allow(dead_code)] // reserved for future audit/RBAC; claims.sub is validated
    pub sub: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
        let token = auth
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
        let claims = verify_token(&state.settings.jwt_secret, token)?;
        Ok(AuthUser { sub: claims.sub })
    }
}

pub fn password_ok(expected: &str, got: &str) -> bool {
    use subtle::ConstantTimeEq;
    if expected.len() != got.len() {
        // still compare something to reduce timing leak on length
        let _ = expected.as_bytes().ct_eq(expected.as_bytes());
        return false;
    }
    expected.as_bytes().ct_eq(got.as_bytes()).into()
}
