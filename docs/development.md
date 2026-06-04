# VectorOS Development Guide

## Getting Started

### Prerequisites

Install the development tools:

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update

# Node.js (via nvm)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 18
nvm use 18

# System dependencies (Debian/Ubuntu)
sudo apt-get install -y \
  build-essential python3 python3-pip \
  libclang-dev pkg-config libssl-dev git
```

### Clone and Build

```bash
git clone --recursive https://github.com/Hi-Jiajun/vectoros.git
cd vectoros

# Build control plane
cargo build

# Build frontend
cd frontend
npm install
npm run build
cd ..
```

### Run in Development

```bash
# Start the control plane (requires root for VPP access)
sudo cargo run -- --config config.toml --api-listen 0.0.0.0:8080

# In another terminal, start the frontend dev server
cd frontend
npm run dev
```

The frontend dev server runs on `http://localhost:5173` with hot-reload.

## Project Structure

```
vectoros/
├── Cargo.toml                      # Rust workspace definition
├── control-plane/                  # Rust control plane
│   ├── Cargo.toml                  # Dependencies and features
│   └── src/
│       ├── main.rs                 # Entry point, CLI, service init
│       ├── api/
│       │   ├── mod.rs              # AppState, server startup
│       │   ├── routes.rs           # All API route definitions
│       │   ├── handlers.rs         # Request handlers (~3500 lines)
│       │   ├── response.rs         # Standardized response envelopes
│       │   ├── error.rs            # Error types (planned)
│       │   ├── openapi.rs          # OpenAPI spec generation (utoipa)
│       │   └── websocket.rs        # WebSocket real-time updates
│       ├── auth/
│       │   └── mod.rs              # JWT auth, middleware
│       ├── config/
│       │   └── mod.rs              # TOML config loading
│       ├── db/
│       │   └── mod.rs              # SQLite database, schema
│       ├── vpp/
│       │   ├── mod.rs              # Module exports
│       │   ├── client.rs           # VPP binary API client
│       │   ├── message.rs          # VPP binary API message encoding
│       │   ├── native.rs           # vppctl command execution
│       │   └── pppoe.rs            # PPPoE API bindings
│       └── services/
│           ├── mod.rs              # Module exports
│           ├── manager.rs          # Service lifecycle orchestrator
│           ├── impls.rs            # Service trait implementations
│           ├── dhcp.rs             # DHCP server management
│           ├── dns.rs              # DNS resolver management
│           ├── firewall.rs         # Firewall rules/groups/aliases
│           ├── frr.rs              # FRRouting integration
│           ├── nat.rs              # NAT configuration
│           ├── pppoe_auto.rs       # PPPoE auto-connect
│           ├── vpn.rs              # VPN tunnel management
│           ├── qos.rs              # QoS policers
│           ├── traffic.rs          # Traffic control
│           ├── flow.rs             # Flow monitoring
│           ├── conntrack.rs        # Connection tracking
│           ├── ipv6.rs             # IPv6 management
│           ├── diag.rs             # Network diagnostics
│           ├── logs.rs             # Log management
│           ├── monitor.rs          # System monitoring
│           ├── config_cli.rs       # VyOS-style config CLI
│           ├── config_io.rs        # Config import/export
│           └── logger.rs           # Logging utilities
├── frontend/                       # Svelte frontend
│   ├── package.json                # Node.js dependencies
│   ├── svelte.config.js            # SvelteKit configuration
│   ├── vite.config.ts              # Vite bundler config
│   └── src/
│       ├── app.html                # HTML template
│       ├── routes/
│       │   ├── +layout.svelte      # Root layout
│       │   ├── +page.svelte        # Dashboard page
│       │   ├── interfaces/         # Interface management
│       │   ├── pppoe/              # PPPoE client
│       │   ├── firewall/           # Firewall rules
│       │   ├── frr/                # FRRouting
│       │   ├── dhcp/               # DHCP server
│       │   ├── dns/                # DNS resolver
│       │   ├── vpn/                # VPN tunnels
│       │   ├── qos/                # QoS
│       │   ├── traffic/            # Traffic control
│       │   ├── flow/               # Flow monitoring
│       │   ├── conntrack/          # Connection tracking
│       │   ├── config/             # Configuration management
│       │   ├── monitor/            # System monitoring
│       │   ├── logs/               # Log viewer
│       │   ├── diag/               # Diagnostics
│       │   ├── services/           # Service management
│       │   ├── settings/           # Settings
│       │   └── ipv6/               # IPv6
│       └── lib/                    # Shared components
├── vpp/                            # VPP source (git submodule)
│   └── src/plugins/pppoeclient/    # PPPoE client plugin
├── vpp-tools/                      # Python VPP management scripts
│   ├── pppoe_manager.py
│   ├── interface_bind.py
│   ├── nat_manager.py
│   └── ... (20+ scripts)
├── vpp-plugins/                    # Additional VPP plugins
├── config/
│   └── startup.vpp                 # VPP startup configuration
└── docs/                           # Documentation
```

## Code Conventions

### Rust

- **Formatting**: Use `cargo fmt` before committing
- **Linting**: Use `cargo clippy` to catch common issues
- **Naming**: Follow Rust conventions (snake_case for functions, CamelCase for types)
- **Error handling**: Use `anyhow::Result` for application errors, `thiserror` for typed errors
- **Async**: Use tokio for async runtime, avoid blocking in async contexts

### Frontend

- **Formatting**: Use Prettier for consistent formatting
- **Components**: SvelteKit file-based routing
- **Styling**: Tailwind CSS utility classes
- **TypeScript**: Used for type safety

### API Design

- Use `#[derive(Serialize, Deserialize)]` for all API types
- Add `#[derive(utoipa::ToSchema)]` for OpenAPI generation
- Use `#[utoipa::path(...)]` annotations on handler functions
- Follow the standardized response envelope format
- Use error codes in the `domain.specific_error` pattern

## Adding a New API Endpoint

### 1. Define Request/Response Types

In `control-plane/src/api/handlers.rs`:

```rust
#[derive(Debug, Deserialize, ToSchema)]
pub struct MyNewRequest {
    pub name: String,
    pub value: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyNewResponse {
    pub id: u32,
    pub name: String,
}
```

### 2. Implement the Handler

```rust
#[utoipa::path(
    post,
    path = "/api/my-endpoint",
    tag = "MyTag",
    request_body = MyNewRequest,
    responses(
        (status = 200, description = "Success", body = MyNewResponse)
    )
)]
pub async fn my_new_handler(
    Json(req): Json<MyNewRequest>,
) -> Json<Value> {
    match my_service_function(&req.name, req.value) {
        Ok(data) => Json(json!(data)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}
```

### 3. Register the Route

In `control-plane/src/api/routes.rs`:

```rust
.route("/api/my-endpoint", post(handlers::my_new_handler))
```

### 4. Add to OpenAPI

In `control-plane/src/api/openapi.rs`, add the handler to the `paths()` list and request/response types to `components(schemas())`.

## Adding a New Service

### 1. Create the Service Module

Create `control-plane/src/services/my_service.rs`:

```rust
use anyhow::Result;
use serde_json::{json, Value};

pub fn show() -> Result<Value> {
    // Implement status check
    Ok(json!({ "status": "running" }))
}

pub fn enable(config: MyConfig) -> Result<Value> {
    // Implement enable logic
    Ok(json!({ "status": "enabled" }))
}
```

### 2. Export the Module

In `control-plane/src/services/mod.rs`:

```rust
pub mod my_service;
```

### 3. Register with ServiceManager (if lifecycle-managed)

Implement the `Service` trait in `control-plane/src/services/impls.rs` and register in `main.rs`.

## Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Frontend type checking
cd frontend && npm run check
```

## Code Architecture Decisions

### Why Rust for Control Plane?

- Memory safety without garbage collection
- Excellent async ecosystem (tokio)
- Strong type system catches errors at compile time
- Low resource usage for embedded router deployments
- FFI capabilities for VPP binary API integration

### Why Svelte for Frontend?

- Minimal bundle size (compiled away)
- Simple component model
- Excellent developer experience
- Built-in reactivity without virtual DOM

### Why vppctl CLI Instead of Pure Binary API?

- vppctl covers all VPP features automatically
- Simpler implementation for most operations
- Binary API used where performance matters (PPPoE, high-frequency queries)
- Future: migrate more operations to binary API

### Service Manager Pattern

The `ServiceManager` provides a centralized lifecycle orchestrator with:
- State machine enforcement (Stopped -> Starting -> Running -> Stopping -> Stopped)
- Automatic rollback on failed restarts
- Runtime state synchronization via `probe()`
- Hot-reload support

## Contributing

### Branch Naming

- `feature/description` -- New features
- `fix/description` -- Bug fixes
- `docs/description` -- Documentation changes
- `refactor/description` -- Code refactoring

### Commit Messages

Use clear, descriptive commit messages:

```
feat: add PPPoE auto-reconnect with exponential backoff
fix: resolve firewall rule ordering issue
docs: add deployment guide
refactor: extract service handler boilerplate
```

### Pull Request Process

1. Create a feature branch from `main`
2. Make your changes with tests
3. Run `cargo fmt` and `cargo clippy`
4. Run `npm run check` for frontend changes
5. Submit a pull request with a clear description
6. Address review feedback

### Code Review Checklist

- [ ] Code compiles without warnings
- [ ] `cargo clippy` passes
- [ ] Tests pass
- [ ] API changes are documented
- [ ] OpenAPI spec is updated
- [ ] Frontend builds without errors
- [ ] No hardcoded paths or credentials
