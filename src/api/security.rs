//! [CON-SEC-01] Security middleware for Nexus API
//! Provides security headers configuration.
//!
//! Rate limiting is provided by tower-http's built-in utilities.

use axum::{
    extract::Request,
    http::{header, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Security headers configuration for production use
pub struct SecurityHeadersConfig {
    /// X-Frame-Options header value
    pub x_frame_options: &'static str,
    /// X-Content-Type-Options header value  
    pub x_content_type_options: &'static str,
    /// X-XSS-Protection header value
    pub x_xss_protection: &'static str,
    /// Referrer-Policy header value
    pub referrer_policy: &'static str,
    /// Strict-Transport-Security header value
    pub strict_transport_security: &'static str,
    /// Content-Security-Policy header value (optional)
    pub content_security_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            x_frame_options: "DENY",
            x_content_type_options: "nosniff",
            x_xss_protection: "0",
            referrer_policy: "strict-origin-when-cross-origin",
            strict_transport_security: "max-age=31536000; includeSubDomains",
            content_security_policy: None,
        }
    }
}

impl SecurityHeadersConfig {
    /// Strict security headers for production
    pub fn strict() -> Self {
        Self {
            strict_transport_security: "max-age=31536000; includeSubDomains; preload",
            ..Default::default()
        }
    }

    /// Production headers with custom CSP
    pub fn with_csp(csp: impl Into<String>) -> Self {
        Self {
            content_security_policy: Some(csp.into()),
            ..Default::default()
        }
    }
}

/// Middleware to apply secure headers to HTTP responses
pub async fn apply_security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    let config = SecurityHeadersConfig::strict();

    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static(config.x_frame_options),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static(config.x_content_type_options),
    );

    if let Ok(name) = header::HeaderName::from_bytes(b"x-xss-protection") {
        headers.insert(name, HeaderValue::from_static(config.x_xss_protection));
    }

    if let Ok(name) = header::HeaderName::from_bytes(b"referrer-policy") {
        headers.insert(name, HeaderValue::from_static(config.referrer_policy));
    }

    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static(config.strict_transport_security),
    );

    if let Some(ref csp) = config.content_security_policy {
        if let Ok(v) = HeaderValue::from_str(csp) {
            headers.insert(header::CONTENT_SECURITY_POLICY, v);
        }
    }

    response
}
