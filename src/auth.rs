use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header, request::Parts, StatusCode},
};
use axum_extra::extract::cookie::CookieJar;
use subtle::ConstantTimeEq;

const COOKIE: &str = "gist_token";

#[derive(Clone)]
pub struct Token(pub String);

impl Token {
    pub fn matches(&self, presented: &str) -> bool {
        let expected = self.0.as_bytes();
        let got = presented.as_bytes();
        if expected.len() != got.len() {
            return false;
        }
        expected.ct_eq(got).into()
    }

    pub fn cookie_name() -> &'static str {
        COOKIE
    }
}

/// Present on every request that passed the token check (header, cookie, or form).
pub struct Authed;

impl<S> FromRequestParts<S> for Authed
where
    S: Send + Sync,
    Token: axum::extract::FromRef<S>,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = Token::from_ref(state);
        if extract_presented(parts).is_some_and(|got| token.matches(&got)) {
            Ok(Authed)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

pub fn extract_presented(parts: &Parts) -> Option<String> {
    extract_from_headers(&parts.headers)
}

pub fn extract_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(raw) = value.to_str() {
            if let Some(token) = from_authorization(raw) {
                return Some(token);
            }
        }
    }
    if let Some(value) = headers.get("x-gist-token") {
        if let Ok(t) = value.to_str() {
            return Some(t.trim().to_string());
        }
    }
    let jar = CookieJar::from_headers(headers);
    jar.get(COOKIE).map(|c| c.value().to_string())
}

fn from_authorization(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if let Some(t) = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
    {
        return Some(t.trim().to_string());
    }
    let b64 = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let pass = decoded.split_once(':').map(|(_, p)| p).unwrap_or(&decoded);
    Some(pass.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_uses_password_as_token() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("gist:secret-token-ok");
        assert_eq!(
            from_authorization(&format!("Basic {encoded}")).as_deref(),
            Some("secret-token-ok")
        );
    }

    #[test]
    fn bearer_still_works() {
        assert_eq!(from_authorization("Bearer abc").as_deref(), Some("abc"));
    }
}
