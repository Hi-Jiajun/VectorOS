# VectorOS API Design Guide

This document defines the REST API design standards for VectorOS, covering response format, error handling, versioning, authentication, and naming conventions.

---

## 1. API Response Format

Every API response MUST follow a consistent envelope structure. This eliminates ambiguity for frontend consumers and enables consistent error handling.

### 1.1 Success Response

```json
{
  "data": { ... }
}
```

For list endpoints:
```json
{
  "data": {
    "items": [...],
    "total": 42
  }
}
```

### 1.2 Error Response

```json
{
  "error": {
    "code": "vpp.connection_failed",
    "message": "Failed to connect to VPP socket at /run/vpp/api.sock"
  }
}
```

### 1.1 Rust Implementation

```rust
use serde::Serialize;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorBody>,
}

#[derive(Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            data: Some(data),
            error: None,
        }
    }

    pub fn error(code: &str, message: &str) -> Self {
        Self {
            data: None,
            error: Some(ApiErrorBody {
                code: code.to_string(),
                message: message.to_string(),
            }),
        }
    }
}

// Convenience type for handlers
pub type ApiResult<T> = Result<Json<ApiResponse<T>>, ApiErrorResponse>;

pub struct ApiErrorResponse {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let body = ApiResponse::<()>::error(&self.code, &self.message);
        (self.status, Json(body)).into_response()
    }
}
```

---

## 2. Error Code Convention

Error codes follow a `{domain}.{specific_error}` pattern, enabling clients to handle errors programmatically without parsing message strings.

### 2.1 Error Code Registry

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `auth.unauthorized` | 401 | Missing or invalid authentication token |
| `auth.forbidden` | 403 | Valid token but insufficient permissions |
| `auth.token_expired` | 401 | JWT token has expired |
| `request.invalid_body` | 400 | Request body failed validation |
| `request.missing_param` | 400 | Required parameter is missing |
| `request.not_found` | 404 | Resource does not exist |
| `vpp.connection_failed` | 502 | Cannot connect to VPP binary API socket |
| `vpp.command_failed` | 502 | VPP returned an error for the requested operation |
| `vpp.timeout` | 504 | VPP did not respond within the timeout window |
| `config.read_failed` | 500 | Cannot read configuration file |
| `config.write_failed` | 500 | Cannot write configuration file |
| `config.invalid` | 400 | Configuration validation failed |
| `config.conflict` | 409 | Configuration change conflicts with current state |
| `service.unavailable` | 503 | A required service (FRR, DHCP, DNS) is not running |
| `service.start_failed` | 500 | Failed to start a service |
| `service.stop_failed` | 500 | Failed to stop a service |
| `internal.error` | 500 | Unexpected internal error |

### 2.2 Error Handling Pattern

```rust
// control-plane/src/error.rs

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("{0}")]
    Auth(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("VPP error: {0}")]
    Vpp(#[from] VppError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Auth(_) => StatusCode::UNAUTHORIZED,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Vpp(_) => StatusCode::BAD_GATEWAY,
            Self::Config(_) => StatusCode::BAD_REQUEST,
            Self::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::Auth(_) => "auth.unauthorized",
            Self::BadRequest(_) => "request.invalid_body",
            Self::NotFound(_) => "request.not_found",
            Self::Vpp(_) => "vpp.connection_failed",
            Self::Config(_) => "config.invalid",
            Self::ServiceUnavailable { .. } => "service.unavailable",
            Self::Conflict(_) => "config.conflict",
            Self::Internal(_) => "internal.error",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();
        let body = ApiResponse::<()>::error(code, &message);
        (status, Json(body)).into_response()
    }
}
```

---

## 3. API Versioning Strategy

### 3.1 URL-Based Versioning

All API routes are prefixed with the version: `/api/v{N}/...`

```
GET    /api/v1/health
GET    /api/v1/interfaces
POST   /api/v1/interfaces/{name}/up
GET    /api/v1/pppoe/clients
POST   /api/v1/pppoe/clients
GET    /api/v1/config
POST   /api/v1/config
GET    /api/v1/routes
POST   /api/v1/routes
DELETE /api/v1/routes
GET    /api/v1/nat/status
POST   /api/v1/nat/enable
GET    /api/v1/dhcp/status
POST   /api/v1/dhcp/enable
GET    /api/v1/dns/status
POST   /api/v1/dns/enable
GET    /api/v1/firewall/rules
POST   /api/v1/firewall/rules
DELETE /api/v1/firewall/rules/{id}
POST   /api/v1/firewall/enable
POST   /api/v1/firewall/disable
GET    /api/v1/ipv6/status
GET    /api/v1/ipv6/neighbors
GET    /api/v1/frr/status
GET    /api/v1/frr/routes
POST   /api/v1/frr/routes
DELETE /api/v1/frr/routes
GET    /api/v1/logs
DELETE /api/v1/logs
GET    /api/v1/system
GET    /api/v1/metrics          (future)
```

### 3.2 Version Bump Rules

- **Minor version** (v1 -> v2): Breaking changes to response format, removed fields, changed field types.
- **Patch** (within v1): Adding new endpoints, adding optional request fields, adding new response fields (backward-compatible).

### 3.3 Version Deprecation

When a new version is introduced:
1. The old version remains functional for at least 6 months.
2. Deprecated endpoints return a `Deprecation` header: `Deprecation: true`.
3. A `Sunset` header indicates the removal date: `Sunset: Sat, 01 Jan 2027 00:00:00 GMT`.
4. The frontend is updated to use the new version before the old one is removed.

---

## 4. Authentication Design

### 4.1 Authentication Flow

```
Client                      Server
  |                            |
  |  POST /api/v1/auth/login   |
  |  {username, password}      |
  |--------------------------->|
  |                            |  Validate credentials
  |  {data: {token, expires}}  |
  |<---------------------------|
  |                            |
  |  GET /api/v1/interfaces    |
  |  Authorization: Bearer xxx |
  |--------------------------->|
  |                            |  Validate JWT
  |  {data: {...}}             |
  |<---------------------------|
```

### 4.2 JWT Token Structure

```json
{
  "sub": "admin",
  "iat": 1749033600,
  "exp": 1749120000,
  "scope": ["read", "write"]
}
```

### 4.3 Implementation

```rust
// control-plane/src/api/auth.rs

use axum::extract::State;
use axum::http::{HeaderMap, header};
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
    pub scope: Vec<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: u64,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    // Validate against stored credentials
    if !validate_credentials(&state.config, &req.username, &req.password) {
        return Err(ApiError::Auth("Invalid credentials".into()));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let claims = Claims {
        sub: req.username,
        iat: now,
        exp: now + 3600, // 1 hour
        scope: vec!["read".into(), "write".into()],
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("Token creation failed: {}", e)))?;

    Ok(Json(ApiResponse::success(LoginResponse {
        token,
        expires_in: 3600,
    })))
}

// Middleware
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::Auth("Missing Authorization header".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(ApiError::Auth("Invalid Authorization scheme".into()))?;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| ApiError::Auth("Invalid or expired token".into()))?;

    Ok(next.run(request).await)
}
```

### 4.4 Exempt Routes

The following routes do NOT require authentication:
- `GET /api/v1/health` -- Health probes (used by load balancers, monitoring)
- `POST /api/v1/auth/login` -- Login endpoint itself

All other routes require a valid `Authorization: Bearer <token>` header.

---

## 5. Naming Conventions

### 5.1 URL Naming

- Use plural nouns for collections: `/api/v1/interfaces`, `/api/v1/routes`
- Use kebab-case for multi-word segments: `/api/v1/pppoe/clients`
- Use path parameters for resource identification: `/api/v1/interfaces/{name}`
- Avoid verbs in URLs -- use HTTP methods instead

### 5.2 HTTP Methods

| Method | Use | Example |
|--------|-----|---------|
| `GET` | Read a resource or list | `GET /api/v1/interfaces` |
| `POST` | Create a resource or trigger an action | `POST /api/v1/routes` |
| `PUT` | Replace a resource entirely | `PUT /api/v1/config` |
| `PATCH` | Partially update a resource | `PATCH /api/v1/config` |
| `DELETE` | Remove a resource | `DELETE /api/v1/routes/{id}` |

### 5.3 Query Parameters for Filtering and Pagination

```
GET /api/v1/routes?page=1&per_page=20&protocol=bgp
GET /api/v1/logs?level=warn&limit=100&filter=nat
GET /api/v1/interfaces?state=up
```

### 5.4 Content Types

- Request body: `Content-Type: application/json`
- Response body: `Content-Type: application/json`
- File uploads: `Content-Type: multipart/form-data`

---

## 6. Rate Limiting and Throttling

### 6.1 Recommended Limits

| Endpoint Category | Rate Limit | Notes |
|-------------------|------------|-------|
| `GET /api/v1/health` | Unlimited | Health probes |
| `GET /api/v1/*` | 100 req/min | Read operations |
| `POST /api/v1/*` | 30 req/min | Write/trigger operations |
| `DELETE /api/v1/*` | 30 req/min | Delete operations |
| `POST /api/v1/auth/login` | 5 req/min | Prevent brute force |

### 6.2 Response Headers

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1749033900
```

### 6.3 Rate Limit Exceeded Response

```json
{
  "error": {
    "code": "rate_limit.exceeded",
    "message": "Too many requests. Retry after 60 seconds."
  }
}
```

HTTP Status: `429 Too Many Requests`

---

## 7. Request Validation

### 7.1 Validation Rules

- All string fields that represent IP addresses MUST be validated as valid IPv4 or IPv6.
- MTU values MUST be between 576 and 9000.
- Port numbers MUST be between 1 and 65535.
- Required fields MUST be present and non-empty.
- Enum fields MUST match one of the allowed values.

### 7.2 Validation Error Response

```json
{
  "error": {
    "code": "request.validation_failed",
    "message": "Validation failed",
    "details": [
      {"field": "username", "message": "Username is required"},
      {"field": "mtu", "message": "MTU must be between 576 and 9000"}
    ]
  }
}
```

### 7.3 Implementation with Axum

```rust
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;

// Replace default JSON rejection with structured error
pub async fn json_extractor_middleware(
    rejection: JsonRejection,
) -> (StatusCode, Json<ApiResponse<()>>) {
    let (code, message) = match rejection {
        JsonRejection::MissingJsonContentType(_) => (
            "request.missing_content_type",
            "Request must have Content-Type: application/json",
        ),
        JsonRejection::JsonSyntaxError(e) => (
            "request.invalid_json",
            &format!("Invalid JSON: {}", e),
        ),
        JsonRejection::JsonDataError(e) => (
            "request.invalid_body",
            &format!("Invalid request body: {}", e),
        ),
        _ => (
            "request.invalid",
            "Invalid request",
        ),
    };

    (
        StatusCode::BAD_REQUEST,
        Json(ApiResponse::error(code, message)),
    )
}
```

---

## 8. API Examples

### 8.1 List Interfaces

```
GET /api/v1/interfaces
```

Response:
```json
{
  "data": {
    "items": [
      {
        "name": "GigabitEthernet0/0/0",
        "sw_if_index": 1,
        "admin_up": true,
        "link_up": true,
        "mtu": 1500,
        "ip_addresses": ["192.168.1.1/24"]
      },
      {
        "name": "GigabitEthernet0/0/1",
        "sw_if_index": 2,
        "admin_up": true,
        "link_up": false,
        "mtu": 1500,
        "ip_addresses": []
      }
    ]
  }
}
```

### 8.2 Create PPPoE Client

```
POST /api/v1/pppoe/clients
Content-Type: application/json

{
  "username": "user@isp.com",
  "password": "secret",
  "interface": "GigabitEthernet0/0/0",
  "mtu": 1492,
  "mru": 1492,
  "use_peer_dns": true,
  "add_default_route4": true
}
```

Response:
```json
{
  "data": {
    "pppox_sw_if_index": 3,
    "session_id": 0,
    "client_state": "discovery"
  }
}
```

### 8.3 Add Firewall Rule

```
POST /api/v1/firewall/rules
Content-Type: application/json

{
  "action": "deny",
  "src_ip": "10.0.0.0/8",
  "dst_port": 22,
  "protocol": "tcp",
  "description": "Block SSH from internal network"
}
```

Response:
```json
{
  "data": {
    "id": 1,
    "action": "deny",
    "src_ip": "10.0.0.0/8",
    "dst_port": 22,
    "protocol": "tcp",
    "description": "Block SSH from internal network",
    "created_at": "2026-06-04T12:00:00Z"
  }
}
```

### 8.4 Error Example

```
POST /api/v1/pppoe/clients
Content-Type: application/json

{
  "username": "",
  "password": "secret",
  "interface": "nonexistent0"
}
```

Response:
```json
{
  "error": {
    "code": "request.validation_failed",
    "message": "Validation failed",
    "details": [
      {"field": "username", "message": "Username is required"},
      {"field": "interface", "message": "Interface 'nonexistent0' not found"}
    ]
  }
}
```

---

## 9. Frontend Integration Guidelines

### 9.1 TypeScript Type Generation

Mirror the Rust `Serialize`/`Deserialize` types in TypeScript:

```typescript
// Auto-generated from Rust types (via ts-rs or manual)

interface ApiResponse<T> {
  data?: T;
  error?: ApiErrorBody;
}

interface ApiErrorBody {
  code: string;
  message: string;
}

interface Interface {
  name: string;
  sw_if_index: number;
  admin_up: boolean;
  link_up: boolean;
  mtu: number;
  ip_addresses: string[];
}

interface PppoeClient {
  username: string;
  interface: string;
  mtu: number;
  mru: number;
  use_peer_dns: boolean;
  add_default_route4: boolean;
}
```

### 9.2 Error Handling in Frontend

```typescript
async function apiCall<T>(url: string, options?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${getToken()}`,
      ...options?.headers,
    },
  });

  const body = await response.json();

  if (body.error) {
    throw new ApiError(body.error.code, body.error.message);
  }

  return body.data;
}

class ApiError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

// Usage
try {
  const interfaces = await apiCall<Interface[]>('/api/v1/interfaces');
} catch (e) {
  if (e instanceof ApiError) {
    switch (e.code) {
      case 'auth.unauthorized':
        redirectToLogin();
        break;
      case 'vpp.connection_failed':
        showVppError();
        break;
      default:
        showError(e.message);
    }
  }
}
```

---

## 10. Migration Plan from Current API

The current VectorOS API uses `/api/` without versioning and returns raw JSON. To migrate:

1. **Phase 1**: Add `/api/v1/` prefixed routes that return the new response format. Keep old `/api/` routes working.
2. **Phase 2**: Update frontend to use `/api/v1/` endpoints.
3. **Phase 3**: Add deprecation headers to old `/api/` routes.
4. **Phase 4**: Remove old `/api/` routes after 6 months.

During the transition period, both `/api/` and `/api/v1/` will be available.
