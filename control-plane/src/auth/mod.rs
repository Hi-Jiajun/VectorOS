use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use utoipa::ToSchema;

use crate::security;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponseWithCsrf {
    pub token: String,
    pub expires_in: usize,
    pub csrf_token: String,
}

const DEFAULT_USERNAME: &str = "admin";
const TOKEN_EXPIRY_HOURS: usize = 24;

fn get_secret() -> String {
    env::var("JWT_SECRET").unwrap_or_else(|_| "vectoros-secret-key".to_string())
}

pub fn generate_token(username: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: username.to_string(),
        iat: now,
        exp: now + (TOKEN_EXPIRY_HOURS * 3600),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(get_secret().as_bytes()))
}

pub fn validate_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(get_secret().as_bytes()), &Validation::default())?;
    Ok(token_data.claims)
}

/// Verify user credentials against stored password hash.
///
/// Authentication flow:
/// 1. If environment variables VECTOROS_USERNAME/VECTOROS_PASSWORD are set,
///    use them as plaintext fallback (for backward compatibility).
/// 2. Otherwise, verify against bcrypt hash stored in the database.
/// 3. On success, return true. On failure, return false.
///
/// The login handler should call `security::audit::log_audit_event` to record
/// both successful and failed login attempts.
pub fn verify_credentials(username: &str, password: &str) -> bool {
    let expected_user = env::var("VECTOROS_USERNAME").unwrap_or_else(|_| DEFAULT_USERNAME.to_string());

    // If env-var based credentials are set, use them (plaintext comparison
    // for backward compatibility -- the env vars should be set to the same
    // values as what was hashed).
    if username == expected_user {
        // First, try env var override
        if let Ok(expected_pass) = env::var("VECTOROS_PASSWORD") {
            return password == expected_pass;
        }
    }

    // Fall back to bcrypt hash from database
    if let Some(hash) = security::get_password_hash(username) {
        return security::verify_password(password, &hash);
    }

    false
}

/// Change a user's password.
///
/// Validates the old password, hashes the new one, and stores it.
pub fn change_password(username: &str, old_password: &str, new_password: &str) -> Result<(), String> {
    // Verify old password
    if !verify_credentials(username, old_password) {
        return Err("Current password is incorrect".to_string());
    }

    // Validate new password
    security::validation::validate_password(new_password)?;

    // Hash and store
    let hash = security::hash_password(new_password)?;
    security::store_password_hash(username, &hash)?;

    security::audit::log_audit_event(
        username,
        security::audit::AuditAction::PasswordChange,
        None,
        None,
        None,
        None,
        Some("Password changed successfully"),
    );

    Ok(())
}

pub async fn auth_middleware(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();
    // Allow login, health check, and non-API routes without authentication
    if path == "/api/auth/login" || path == "/api/health" || !path.starts_with("/api/") {
        return Ok(next.run(request).await);
    }
    let auth_header = request.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    let token = match auth_header {
        Some(v) if v.starts_with("Bearer ") => v[7..].to_string(),
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    match validate_token(&token) {
        Ok(claims) => {
            request.extensions_mut().insert(claims);
            Ok(next.run(request).await)
        }
        Err(_) => {
            // Log failed authentication attempt
            let ip = security::rate_limit::extract_client_ip(request.headers());
            security::audit::log_audit_event(
                "anonymous",
                security::audit::AuditAction::UnauthorizedAccess,
                Some(&request.method().to_string()),
                Some(&path),
                Some(&ip.to_string()),
                Some(401),
                Some("Invalid or expired JWT token"),
            );
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
