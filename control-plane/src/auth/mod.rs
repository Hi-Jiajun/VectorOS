use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: usize,
}

const DEFAULT_USERNAME: &str = "admin";
const DEFAULT_PASSWORD: &str = "vectoros";
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

pub fn verify_credentials(username: &str, password: &str) -> bool {
    let expected_user = env::var("VECTOROS_USERNAME").unwrap_or_else(|_| DEFAULT_USERNAME.to_string());
    let expected_pass = env::var("VECTOROS_PASSWORD").unwrap_or_else(|_| DEFAULT_PASSWORD.to_string());
    username == expected_user && password == expected_pass
}

pub async fn auth_middleware(mut request: Request, next: Next) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();
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
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}
