use axum::{
    extract::Query,
    http::{header, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use tracing;

const W9_DB_URL: &str = "https://db.w9.nu";
const SESSION_COOKIE_NAME: &str = "w9_search_session";
const SESSION_MAX_AGE_SECONDS: i64 = 60 * 60 * 24 * 7;

#[derive(Debug, Clone)]
pub struct UserSession {
    pub email: String,
    pub role: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
}

fn app_base_url() -> String {
    std::env::var("APP_BASE_URL")
        .unwrap_or_else(|_| {
            let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
            format!("http://localhost:{}", port)
        })
        .trim_end_matches('/')
        .to_string()
}

fn callback_url() -> String {
    format!("{}/oauth/callback", app_base_url())
}

pub fn login_url() -> String {
    format!(
        "{}/oauth/authorize?redirect_uri={}&response_type=code&client_id=w9-search",
        W9_DB_URL,
        urlencoding::encode(&callback_url())
    )
}

fn secure_cookie() -> bool {
    app_base_url().starts_with("https://")
}

fn encode_session(session: &UserSession) -> String {
    URL_SAFE_NO_PAD.encode(format!(
        "{}:{}:{}",
        session.email, session.role, session.access_token
    ))
}

fn build_cookie_header(value: &str, max_age_seconds: i64) -> HeaderValue {
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={};{}",
        SESSION_COOKIE_NAME,
        value,
        max_age_seconds,
        if secure_cookie() { " Secure" } else { "" }
    );

    HeaderValue::from_str(&cookie).expect("valid cookie header")
}

pub fn require_session(headers: &HeaderMap) -> Option<UserSession> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        let Some(value) = cookie.strip_prefix(&format!("{}=", SESSION_COOKIE_NAME)) else {
            continue;
        };

        let decoded = URL_SAFE_NO_PAD.decode(value.as_bytes()).ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let mut parts = decoded.splitn(3, ':');
        let email = parts.next()?;
        let second = parts.next()?;
        let third = parts.next();

        let (role, access_token) = match third {
            Some(token) => (second, token),
            None => ("client", second),
        };

        if !email.is_empty() && !role.is_empty() && !access_token.is_empty() {
            return Some(UserSession {
                email: email.to_string(),
                role: role.to_string(),
                access_token: access_token.to_string(),
            });
        }
    }

    None
}

pub fn require_admin(headers: &HeaderMap) -> Option<UserSession> {
    require_session(headers).filter(|session| session.role == "admin")
}

pub fn can_choose_model_role(role: &str) -> bool {
    matches!(role, "admin" | "dev" | "developer")
}

async fn redirect_to_login() -> Response {
    Redirect::to("/login").into_response()
}

fn popup_close_html(target: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>W9 Search Login</title></head><body><script>(function(){{const target = {target:?}; if (window.opener && !window.opener.closed) {{ try {{ window.opener.location.href = target; window.opener.focus(); }} catch (_) {{}} window.close(); }} else {{ window.location.replace(target); }}}})();</script><p>Signing you in...</p></body></html>"#
    )
}

pub async fn callback(Query(query): Query<OAuthCallbackQuery>) -> Response {
    let redirect_uri = callback_url();
    let client = reqwest::Client::new();

    let token_response = match client
        .post(format!("{}/oauth/token", W9_DB_URL))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", query.code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("OAuth token exchange failed: {}", e);
            return redirect_to_login().await;
        }
    };

    if !token_response.status().is_success() {
        let status = token_response.status();
        let error_text = token_response.text().await.unwrap_or_default();
        tracing::error!("OAuth token exchange returned {}: {}", status, error_text);
        return redirect_to_login().await;
    }

    let token_json = match token_response.json::<serde_json::Value>().await {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("Failed to parse OAuth token response: {}", e);
            return redirect_to_login().await;
        }
    };

    let access_token = match token_json
        .get("access_token")
        .and_then(|value| value.as_str())
    {
        Some(token) if !token.is_empty() => token.to_string(),
        _ => {
            tracing::error!("OAuth token response missing access_token: {}", token_json);
            return redirect_to_login().await;
        }
    };

    let user_response = match client
        .get(format!("{}/api/auth/me", W9_DB_URL))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("OAuth user lookup failed: {}", e);
            return redirect_to_login().await;
        }
    };

    if !user_response.status().is_success() {
        let status = user_response.status();
        let error_text = user_response.text().await.unwrap_or_default();
        tracing::error!("OAuth user lookup returned {}: {}", status, error_text);
        return redirect_to_login().await;
    }

    let user_json = match user_response.json::<serde_json::Value>().await {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("Failed to parse OAuth user response: {}", e);
            return redirect_to_login().await;
        }
    };

    let email = match user_json.get("email").and_then(|value| value.as_str()) {
        Some(email) if !email.is_empty() => email.to_string(),
        _ => {
            tracing::error!("OAuth user response missing email: {}", user_json);
            return redirect_to_login().await;
        }
    };
    let role = user_json
        .get("role")
        .and_then(|value| value.as_str())
        .unwrap_or("client")
        .to_string();

    tracing::info!("OAuth login succeeded for {} ({})", email, role);

    let session = UserSession {
        email,
        role,
        access_token,
    };
    let session_value = encode_session(&session);

    let mut response = Html(popup_close_html("/")).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        build_cookie_header(&session_value, SESSION_MAX_AGE_SECONDS),
    );
    response
}

pub async fn logout() -> Response {
    let mut response = Redirect::to("/login").into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, build_cookie_header("", 0));
    response
}
