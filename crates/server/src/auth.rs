use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, SignedCookieJar};

use crate::AppState;

const SESSION_COOKIE: &str = "beholder_session";

pub async fn auth_middleware(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // No password set - allow all
    let Some(ref expected_password) = state.password else {
        return next.run(request).await;
    };

    let path = request.uri().path().to_string();

    // Allow login page and static assets
    if path == "/login" || path.starts_with("/static/") {
        return next.run(request).await;
    }

    // API and OTLP endpoints: check X-Api-Password header
    if path.starts_with("/api/") || path.starts_with("/v1/") {
        if let Some(api_pw) = request.headers().get("X-Api-Password") {
            if let Ok(pw_str) = api_pw.to_str() {
                if pw_str == expected_password {
                    return next.run(request).await;
                }
            }
        }
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // SSE and web pages: check session cookie
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        if cookie.value() == "authenticated" {
            return next.run(request).await;
        }
    }

    Redirect::to("/login").into_response()
}

pub fn create_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, "authenticated".to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .build()
}
