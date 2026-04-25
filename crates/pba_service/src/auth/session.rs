use cookie::time::Duration;
use cookie::{Cookie, Key, SameSite};
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;

const SESSION_COOKIE: &str = "pba_session";

/// User session stored in a signed cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub sub: String,
    pub display_name: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
}

/// Read the session from the signed cookie.
pub fn get_session(cookies: &Cookies, key: &Key) -> Option<UserSession> {
    let signed = cookies.signed(key);
    let cookie = signed.get(SESSION_COOKIE)?;
    serde_json::from_str(cookie.value()).ok()
}

/// Write the session to a signed cookie.
pub fn set_session(cookies: &Cookies, key: &Key, session: &UserSession) {
    let value = serde_json::to_string(session).expect("Failed to serialize session");
    let mut cookie = Cookie::new(SESSION_COOKIE, value);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(Duration::hours(8));
    cookies.signed(key).add(cookie);
}

/// Clear the session cookie.
pub fn clear_session(cookies: &Cookies, key: &Key) {
    let cookie = Cookie::build(SESSION_COOKIE).path("/").build();
    cookies.signed(key).remove(cookie);
}
