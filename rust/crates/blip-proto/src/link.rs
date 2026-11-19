//! Reading the server address people paste (`parse`/`team_for` in `blip/link.py`).
//!
//! - `https://blip.example.com/#team=3f9c...`  private team on that server
//! - `blip.example.com`                        a domain: wss://blip.example.com/ws
//! - `192.168.1.200:30620`, `http://host:port`  plain ws://
//! - a plain address means the server's shared team

pub const DEFAULT_PORT: u16 = 47620;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// ws:// or wss:// URL to connect to
    pub url: String,
    /// private team code from `#team=`, or empty
    pub team: String,
    /// http(s):// base, for sharing
    pub web: String,
    pub host: String,
}

fn team_in(text: &str) -> String {
    for sep in ['#', '&', '?'] {
        let pat = format!("{sep}team=");
        if let Some(i) = text.find(&pat) {
            let code: String = text[i + pat.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if (4..=64).contains(&code.len()) {
                return code;
            }
        }
    }
    String::new()
}

pub fn parse(text: &str) -> Option<Link> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let team = team_in(text);
    let base = text.split(['#', '?']).next().unwrap_or("").trim_end_matches('/');
    let (scheme, rest) = match base.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => (String::new(), base),
    };
    let hostport = rest.split('/').next().unwrap_or("");
    if hostport.is_empty() {
        return None;
    }
    let (host, mut port) = match hostport.matches(':').count() {
        1 => {
            let (h, p) = hostport.split_once(':').unwrap();
            if !p.chars().all(|c| c.is_ascii_digit()) || p.is_empty() {
                return None;
            }
            (h.to_string(), p.to_string())
        }
        _ => (hostport.to_string(), String::new()),
    };
    let secure = match scheme.as_str() {
        "https" | "wss" | "blip" => true,
        "http" | "ws" => false,
        "" => {
            let is_ip = host.parse::<std::net::IpAddr>().is_ok() || host == "localhost" || !host.contains('.');
            let secure = !(is_ip || !port.is_empty());
            if !secure && port.is_empty() {
                port = DEFAULT_PORT.to_string();
            }
            secure
        }
        _ => return None,
    };
    let hp = if port.is_empty() { host.clone() } else { format!("{host}:{port}") };
    Some(Link {
        url: format!("{}://{hp}/ws", if secure { "wss" } else { "ws" }),
        team,
        web: format!("{}://{hp}", if secure { "https" } else { "http" }),
        host,
    })
}

/// Team code for everyone who simply enters this server's address.
pub fn shared_team(link: &Link) -> String {
    format!("shared:{}", link.web.to_lowercase())
}

/// The team a pet uses: its private team if it has one, else the server's shared team.
pub fn team_for(server: &str, team: &str) -> String {
    match parse(server) {
        None => String::new(),
        Some(l) if !team.is_empty() => {
            let _ = l;
            team.to_string()
        }
        Some(l) if !l.team.is_empty() => l.team.clone(),
        Some(l) => shared_team(&l),
    }
}

pub fn invite_link(link: &Link, team: &str) -> String {
    if team.is_empty() { link.web.clone() } else { format!("{}/#team={team}", link.web) }
}
