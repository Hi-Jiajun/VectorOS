# VectorOS vs Landscape: Architecture Comparison and Improvement Roadmap

## Overview

This document compares the VectorOS (VPP-based) and Landscape (eBPF-based) router architectures, identifies strengths and weaknesses of each, and proposes specific improvements for VectorOS based on lessons learned from Landscape's design.

---

## 1. Architecture Comparison

### 1.1 High-Level Architecture

| Aspect | VectorOS | Landscape |
|--------|----------|-----------|
| Data Plane | VPP (userspace DPDK) | eBPF (kernel) + AF_PACKET |
| Control Plane | Rust (tokio + axum) | Rust (tokio + axum) |
| Frontend | Svelte + Tailwind | Vue + TypeScript |
| Language Mix | Rust 75%, Python 15%, JS 10% | Rust 67%, Vue/TS 27%, C 6% |
| Routing Protocols | FRRouting (BGP/OSPF via FPM) | eBPF-based routing + netlink |
| Configuration | TOML file + JSON file (split) | TOML (single source) |
| Persistence | File-based (no database) | Sea-ORM (SQLite) + DuckDB |
| API Documentation | None | OpenAPI via utoipa + Scalar |
| Authentication | None | JWT Bearer token |
| Real-time Updates | None | WebSocket support |
| TLS | None | Rustls with ACME/Let's Encrypt |
| Service Lifecycle | Manual process management | Graceful shutdown with timeouts |
| Container Support | None | Docker integration via bollard |

### 1.2 Module Organization

**VectorOS** uses a flat module structure:
```
control-plane/src/
  main.rs          -- Entry point
  api/             -- Routes + handlers (single handlers.rs, ~665 lines)
  config/          -- TOML config loader
  vpp/             -- VPP binary API client
  services/        -- FRR integration, logger
```

**Landscape** uses a workspace of focused crates:
```
landscape/
  landscape-common/       -- Shared types, utilities, API response
  landscape-database/     -- Sea-ORM persistence + migrations
  landscape-dns/          -- Per-flow DNS resolver
  landscape-gateway/      -- Traffic steering engine
  landscape-ebpf/         -- eBPF loaders + kernel programs
  landscape-webserver/    -- HTTP server, API routes, middleware
  landscape-protobuf/     -- Inter-component protocol definitions
  landscape-macro/        -- Custom derive macros
  landscape-types/        -- Core type definitions
```

### 1.3 Control Plane Communication

**VectorOS**: The Rust control plane shells out to Python scripts (`python3 /root/VectorOS/vpp-tools/*.py`) for most operations. This creates a process-per-request overhead and introduces a second runtime dependency. The Rust VPP binary API client exists but is incomplete (only PPPoE is implemented natively).

**Landscape**: All operations run in-process. eBPF programs are loaded via `libbpf-rs`, networking uses `rtnetlink`/`netlink`, and DNS runs natively via `hickory-dns`. No external process spawning for core operations.

---

## 2. Strengths and Weaknesses

### 2.1 VectorOS Strengths

- **VPP Data Plane**: VPP provides wire-speed packet processing with DPDK, which is significantly faster than eBPF for high-throughput scenarios (10Gbps+). VPP's graph-node architecture allows complex packet processing pipelines.
- **FRRouting Integration**: Native BGP/OSPF support via FRRouting is mature and production-ready for enterprise routing scenarios.
- **Simplicity**: The current codebase is straightforward and easy to understand. For a small team, this is an advantage during early development.
- **PPPoE Support**: The native Rust PPPoE binary API client demonstrates the ability to communicate directly with VPP without external processes.

### 2.2 VectorOS Weaknesses

- **Python Dependency in Hot Path**: Every API call spawns a Python process (`Command::new("python3")`). This adds 50-200ms latency per request, consumes memory, and creates a fragile dependency chain.
- **Hardcoded Paths**: Scripts reference `/root/VectorOS/vpp-tools/` directly, making deployment inflexible.
- **No Error Type System**: All handlers return `Json<Value>` with ad-hoc `{"error": "..."}` objects. There is no structured error taxonomy, no HTTP status codes for errors, and no machine-readable error identifiers.
- **No API Versioning**: All routes live under `/api/` with no version prefix. Breaking changes cannot be introduced without breaking clients.
- **No Authentication**: The management API is completely open. Any network client can configure the router.
- **No API Documentation**: No OpenAPI spec, no auto-generated docs, no TypeScript bindings for the frontend.
- **No Real-time Updates**: The frontend must poll for status changes. No WebSocket or SSE support.
- **No Database**: Configuration lives in separate TOML and JSON files with no transactional guarantees, migrations, or rollback support.
- **Monolithic Handler File**: `handlers.rs` is 665 lines with 30+ handler functions. Repeated boilerplate for subprocess execution.
- **No Graceful Shutdown**: The server does not handle SIGTERM/SIGINT gracefully. Active connections may be dropped.
- **No Request Tracing**: No request ID propagation, no correlation across service calls.
- **No Metrics**: No Prometheus endpoint, no request duration histograms, no error rate counters.

### 2.3 Landscape Strengths

- **Unified Language**: 93% Rust + TypeScript. No Python runtime dependency. Everything runs in a single process.
- **Structured Error Handling**: `LandscapeApiError` enum with domain-specific variants, automatic HTTP status mapping, machine-readable error IDs, and JSON error args for client-side localization.
- **Standardized API Response**: `LandscapeApiResp<T>` wrapper ensures all responses follow the same `{data, error_id, message, args}` structure.
- **API Documentation**: `utoipa` OpenAPI generation with Scalar UI at `/docs`. TypeScript bindings auto-generated from Rust types.
- **Authentication**: JWT Bearer token auth middleware on all API routes, with a separate query-string auth path for WebSockets.
- **Database Backing**: Sea-ORM with migrations provides transactional config storage, versioned schema, and rollback support.
- **Service Lifecycle Management**: Graceful shutdown with a 30-second timeout, orderly service teardown sequence.
- **Real-time Updates**: WebSocket routes for live data (Docker tasks, terminal, config dumps).
- **Hot-Swappable Config**: `ArcSwap` for runtime config changes without restarts.
- **Channel-based Communication**: `tokio::mpsc` and `broadcast` channels for decoupled service communication.

### 2.4 Landscape Weaknesses

- **eBPF Throughput Limits**: While eBPF is excellent for packet classification and steering, it cannot match VPP+DPDK for raw forwarding performance at line rate on 10Gbps+ links.
- **GPL License Constraint**: eBPF components are GPL-2.0, which may limit commercial use.
- **Complexity**: The workspace has 9+ crates and significant infrastructure (protobuf, migrations, Docker integration). This increases the barrier to entry for new contributors.
- **No FRRouting Integration**: Relies on netlink for routing rather than a mature routing daemon, which may limit BGP/OSPF feature completeness.

---

## 3. Specific Improvement Recommendations for VectorOS

### 3.1 Replace Python Subprocess Calls with Rust-Native VPP Communication

**Current Problem**: Every API handler spawns `python3 /root/VectorOS/vpp-tools/*.py`. This is the single most impactful architectural weakness.

**Recommendation**: Extend the existing Rust VPP binary API client (`control-plane/src/vpp/`) to cover all management operations. The client infrastructure already exists -- `VppClient`, `VppMessage`, and the PPPoE API demonstrate the pattern.

**Implementation Steps**:
1. Create a `VppService` struct that holds a `VppClient` connection and provides async methods for each operation.
2. Move the logic from Python scripts into Rust: interface management, NAT, DNS, DHCP, firewall, IPv6.
3. For VPP CLI operations that cannot be done via binary API, use `tokio::process::Command` with proper timeout and error handling instead of synchronous `std::process::Command`.
4. Use a connection pool or a single persistent connection with proper error recovery.

**Estimated Impact**: Eliminates Python runtime dependency, reduces per-request latency by 50-200ms, improves reliability.

### 3.2 Implement a Structured Error Type System

**Current Problem**: All handlers return `Json<Value>` with inconsistent error shapes (`{"error": "..."}`, `{"error": "Parse error: ..."}`, `{"error": "Command error: ..."}`).

**Recommendation**: Adopt Landscape's pattern of a centralized error enum with domain-specific variants.

**Implementation**:
```rust
// control-plane/src/error.rs

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("VPP communication failed: {0}")]
    Vpp(#[from] VppError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Service not available: {service}")]
    ServiceUnavailable { service: String },

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_id, message) = match &self {
            Self::Vpp(e) => (StatusCode::BAD_GATEWAY, "vpp.error", e.to_string()),
            Self::Config(e) => (StatusCode::BAD_REQUEST, "config.error", e.to_string()),
            Self::ServiceUnavailable { service } => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service.unavailable",
                format!("{} is not available", service),
            ),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "request.invalid", msg.clone()),
            Self::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, "internal.error", e.to_string()),
        };

        let body = ApiResponse::<()>::error(error_id, &message);
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
```

### 3.3 Standardize API Response Format

**Current Problem**: Responses are raw JSON values with no consistent envelope.

**Recommendation**: Define a standard response wrapper.

```rust
// control-plane/src/api/response.rs

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: Option<T>,
    pub error: Option<ApiErrorBody>,
}

#[derive(Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self { data: Some(data), error: None }
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
```

### 3.4 Introduce API Versioning

**Current Problem**: All routes are under `/api/` with no version. Any change breaks clients.

**Recommendation**: Prefix all routes with `/api/v1/`. The frontend and any API consumers should target the versioned path. This allows introducing `/api/v2/` in the future for breaking changes.

**Route changes**:
```
/api/health           --> /api/v1/health
/api/interfaces       --> /api/v1/interfaces
/api/pppoe/clients    --> /api/v1/pppoe/clients
...
```

### 3.5 Add Authentication Middleware

**Current Problem**: The router management API is completely open.

**Recommendation**: Implement JWT Bearer token authentication similar to Landscape.

**Implementation Steps**:
1. Add a login endpoint that validates credentials and returns a JWT.
2. Implement an auth middleware layer using `axum::middleware::from_fn` that validates the Bearer token on protected routes.
3. Exempt `/api/v1/health` from auth (for monitoring probes).
4. Store the JWT secret in configuration (not hardcoded).

**Dependencies to add**:
```toml
jsonwebtoken = "9"
```

### 3.6 Add OpenAPI Documentation

**Current Problem**: No API documentation exists. Frontend developers must read source code.

**Recommendation**: Use `utoipa` for OpenAPI generation, matching Landscape's approach.

**Implementation Steps**:
1. Add `utoipa` and `utoipa-axum` dependencies.
2. Annotate request/response types with `#[derive(utoipa::ToSchema)]`.
3. Annotate handler functions with `#[utoipa::path(...)]`.
4. Mount the Scalar UI at `/docs`.

### 3.7 Refactor Handler Boilerplate

**Current Problem**: Every handler repeats the same subprocess execution pattern (~15 lines of boilerplate each).

**Recommendation**: Extract a generic helper or use a service layer.

```rust
// Generic helper for subprocess-based operations
async fn run_vpp_tool(tool: &str, action: &str, args: &[(&str, &str)]) -> ApiResult<Value> {
    let mut cmd = tokio::process::Command::new("python3");
    cmd.arg(format!("/root/VectorOS/vpp-tools/{}", tool));
    cmd.arg(action);
    for (key, value) in args {
        cmd.arg(format!("--{}", key));
        cmd.arg(value);
    }

    let output = cmd.output()
        .await
        .map_err(|e| ApiError::ServiceUnavailable { service: tool.to_string() })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ApiError::Vpp(VppError::CommandFailed(stderr.to_string())));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("Parse error: {}", e)))
}
```

This reduces each handler to 3-5 lines.

### 3.8 Add Graceful Shutdown

**Current Problem**: Active connections are dropped on SIGTERM/SIGINT.

**Recommendation**: Follow Landscape's pattern with `tokio::select!` between the server future and a shutdown signal.

```rust
async fn run_server(app: Router, listen: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let server = axum::serve(listener, app);

    let server_handle = server.with_graceful_shutdown(shutdown_signal());

    tokio::select! {
        result = server_handle => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = shutdown_signal() => {
            tracing::info!("Shutdown signal received, stopping server...");
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }
}
```

### 3.9 Add Request Tracing and Metrics

**Current Problem**: No way to trace requests through the system or measure performance.

**Recommendation**:
1. Add `tower_http::trace::TraceLayer` for request/response logging.
2. Add request ID propagation via `x-request-id` header.
3. Add a `/metrics` endpoint using `prometheus` or `metrics` crate.

```rust
use tower_http::trace::TraceLayer;

let app = Router::new()
    .merge(api_routes())
    .layer(TraceLayer::new_for_http())
    .layer(PropagateRequestIdLayer::x_request_id());
```

### 3.10 Configuration Management Improvements

**Current Problem**: VectorOS has two separate config systems: a Rust-side TOML file (`/etc/vectoros/config.toml`) and a Python-side JSON file (`/etc/vectoros/config.json`). Changes made via the API may not be reflected in the other.

**Recommendation**: Consolidate to a single configuration source.

1. Choose one format (TOML is preferred since it is human-readable and already used by the Rust side).
2. Use the Rust config module as the single source of truth.
3. Persist changes atomically (write to temp file, then rename).
4. Support config versioning or migration for forward/backward compatibility.

### 3.11 WebSocket Support for Real-time Updates

**Current Problem**: The frontend must poll for status changes (PPPoE connection state, interface status, etc.).

**Recommendation**: Add WebSocket routes for live status streaming.

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    // Subscribe to VPP status changes
    // Send updates as they occur
}
```

---

## 4. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| P0 | Refactor handlers into service layer with async subprocess | 2 days | Eliminates sync blocking |
| P0 | Implement `ApiError` enum and `ApiResponse<T>` wrapper | 1 day | Consistent API contract |
| P0 | Add API versioning (`/api/v1/`) | 0.5 days | Future-proofing |
| P1 | Add request tracing middleware | 0.5 days | Observability |
| P1 | Add graceful shutdown | 0.5 days | Reliability |

### Phase 2: Security and Documentation (Weeks 3-4)

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| P0 | Implement JWT authentication | 2 days | Security |
| P1 | Add OpenAPI docs via utoipa | 2 days | Developer experience |
| P1 | Generate TypeScript bindings for frontend | 1 day | Type safety |
| P2 | Consolidate config to single TOML file | 1 day | Configuration simplicity |

### Phase 3: VPP Native Integration (Weeks 5-8)

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| P0 | Implement VPP binary API for interface management | 3 days | Eliminates Python for interfaces |
| P0 | Implement VPP binary API for NAT | 2 days | Eliminates Python for NAT |
| P1 | Implement VPP binary API for DNS | 2 days | Eliminates Python for DNS |
| P1 | Implement VPP binary API for DHCP | 2 days | Eliminates Python for DHCP |
| P2 | Implement VPP binary API for firewall | 2 days | Eliminates Python for firewall |
| P2 | Remove Python scripts dependency | 1 day | Simplified deployment |

### Phase 4: Advanced Features (Weeks 9-12)

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| P1 | Add WebSocket support for real-time updates | 3 days | Better UX |
| P1 | Add Prometheus metrics endpoint | 2 days | Monitoring |
| P2 | Add SQLite persistence for config | 2 days | Atomic config changes |
| P2 | Add TLS support | 3 days | Security |
| P3 | Add TypeScript binding generation | 1 day | Frontend type safety |

---

## 5. Key Takeaways

1. **The Python subprocess pattern is the biggest architectural weakness in VectorOS.** It adds latency, creates a fragile dependency chain, and makes error handling inconsistent. The VPP binary API client in Rust already exists -- it should be extended to cover all operations.

2. **Landscape's error handling and API response patterns are directly adoptable.** The `ApiError` enum with domain variants and the `ApiResponse<T>` wrapper are battle-tested patterns that solve VectorOS's current ad-hoc error handling.

3. **VectorOS has a genuine architectural advantage in the VPP data plane.** For high-throughput routing scenarios (10Gbps+), VPP with DPDK outperforms eBPF. The goal should be to match Landscape's control plane quality while retaining VPP's data plane performance.

4. **Security cannot be deferred.** Authentication should be implemented before any production deployment. Landscape's JWT approach is a good model.

5. **API documentation is a force multiplier.** Adding OpenAPI docs with `utoipa` will improve both frontend development speed and third-party integration capability.
