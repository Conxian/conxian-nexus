//! [CON-SEC-01] Security middleware for Nexus API
//! Provides security headers configuration.
//!
//! Rate limiting is provided by tower-http's built-in utilities.

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
