use axum::{middleware::Next, response::Response};

/// Security headers middleware that adds defensive HTTP headers to all responses.
///
/// These headers protect against common web vulnerabilities:
/// - Clickjacking (X-Frame-Options)
/// - MIME-type sniffing (X-Content-Type-Options)
/// - XSS (X-XSS-Protection)
/// - Information leakage (Referrer-Policy, Permissions-Policy)
/// - Caching of sensitive data (Cache-Control)
pub async fn security_headers_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Prevent MIME-type sniffing
    headers.insert(
        "x-content-type-options",
        "nosniff".parse().expect("valid header value"),
    );

    // Prevent clickjacking
    headers.insert(
        "x-frame-options",
        "DENY".parse().expect("valid header value"),
    );

    // XSS protection (legacy but still useful for older browsers)
    headers.insert(
        "x-xss-protection",
        "1; mode=block".parse().expect("valid header value"),
    );

    // Control referrer information
    headers.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin"
            .parse()
            .expect("valid header value"),
    );

    // Restrict browser features
    headers.insert(
        "permissions-policy",
        "camera=(), microphone=(), geolocation=(), payment=()"
            .parse()
            .expect("valid header value"),
    );

    // Prevent caching of API responses
    if path.starts_with("/api/") {
        headers.insert(
            "cache-control",
            "no-store, no-cache, must-revalidate"
                .parse()
                .expect("valid header value"),
        );
        headers.insert(
            "pragma",
            "no-cache".parse().expect("valid header value"),
        );
    }

    // Content Security Policy for API responses
    if path.starts_with("/api/") {
        headers.insert(
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'"
                .parse()
                .expect("valid header value"),
        );
    }

    // Strict Transport Security (HSTS)
    headers.insert(
        "strict-transport-security",
        "max-age=31536000; includeSubDomains"
            .parse()
            .expect("valid header value"),
    );

    // Remove server identification headers
    headers.remove("server");
    headers.remove("x-powered-by");

    response
}
