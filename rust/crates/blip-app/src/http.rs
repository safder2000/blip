//! Plain HTTPS requests (GitHub for updates, Basecamp) using the system's TLS: SChannel on
//! Windows, Secure Transport on macOS, so company proxies' certificates just work.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::Value;

pub const UA: &str = "Blip desktop pet (https://github.com/safder2000/blip)";

pub fn agent() -> &'static ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| {
        let mut b = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(Duration::from_secs(60)).user_agent(UA);
        if let Ok(tls) = native_tls::TlsConnector::new() {
            b = b.tls_connector(Arc::new(tls));
        }
        b.build()
    })
}

pub enum Fail {
    Status(u16, Option<u64>),
    Transport(String),
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Status(c, _) => write!(f, "HTTP {c}"),
            Fail::Transport(t) => write!(f, "{t}"),
        }
    }
}

fn result(r: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, Fail> {
    match r {
        Ok(r) => Ok(r),
        Err(ureq::Error::Status(code, resp)) => Err(Fail::Status(code, resp.header("Retry-After").and_then(|v| v.parse().ok()))),
        Err(ureq::Error::Transport(t)) => Err(Fail::Transport(t.to_string())),
    }
}

/// GET returning JSON (null for an empty body).
pub fn get_json(url: &str, token: Option<&str>, accept: &str) -> Result<Value, Fail> {
    let mut req = agent().get(url).set("Accept", accept);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let resp = result(req.call())?;
    let text = resp.into_string().map_err(|e| Fail::Transport(e.to_string()))?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| Fail::Transport(e.to_string()))
}

/// POST a form (or an empty body) returning JSON.
pub fn post(url: &str, token: Option<&str>, form: Option<&[(&str, &str)]>) -> Result<Value, Fail> {
    let mut req = agent().post(url).set("Accept", "application/json");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let resp = match form {
        Some(f) => result(req.send_form(f))?,
        None => result(req.send_bytes(&[]))?,
    };
    let text = resp.into_string().unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Download a file's bytes.
pub fn get_bytes(url: &str, limit: u64) -> Result<Vec<u8>, Fail> {
    let resp = result(agent().get(url).set("Accept", "application/octet-stream").timeout(Duration::from_secs(300)).call())?;
    let mut out = vec![];
    use std::io::Read;
    resp.into_reader().take(limit).read_to_end(&mut out).map_err(|e| Fail::Transport(e.to_string()))?;
    Ok(out)
}

pub fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
}
