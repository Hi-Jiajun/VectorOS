use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Serialize;

/// Standardized API success response envelope.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: T,
    pub timestamp: String,
}

/// Standardized API error detail.
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Standardized API error response envelope.
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: ApiErrorBody,
    pub timestamp: String,
}

/// Wraps a success payload in the standard envelope.
pub fn success_response<T: Serialize>(data: T) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data,
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// Creates a 200 OK success response.
pub fn ok_response<T: Serialize>(data: T) -> (StatusCode, Json<ApiResponse<T>>) {
    (StatusCode::OK, Json(success_response(data)))
}

/// Creates a 201 Created success response.
pub fn created_response<T: Serialize>(data: T) -> (StatusCode, Json<ApiResponse<T>>) {
    (StatusCode::CREATED, Json(success_response(data)))
}

/// Builds a JSON error response envelope with the given HTTP status code.
pub fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        status,
        Json(ApiErrorResponse {
            success: false,
            error: ApiErrorBody {
                code: code.to_string(),
                message: message.to_string(),
                details: None,
            },
            timestamp: Utc::now().to_rfc3339(),
        }),
    )
}

/// Builds a JSON error response envelope with additional details.
pub fn error_response_with_details(
    status: StatusCode,
    code: &str,
    message: &str,
    details: serde_json::Value,
) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        status,
        Json(ApiErrorResponse {
            success: false,
            error: ApiErrorBody {
                code: code.to_string(),
                message: message.to_string(),
                details: Some(details),
            },
            timestamp: Utc::now().to_rfc3339(),
        }),
    )
}

// Re-export axum Json for convenience within this module.
pub use axum::Json;

/// Implement IntoResponse for ApiResponse so it can be returned from handlers.
impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}
