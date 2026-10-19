use crate::game::Game;
use crate::net;
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FALLBACK_VERSION: &str = "9.6.158";
const RETRIES: u32 = 4;

#[derive(Clone, Debug)]
pub struct Choice {
    pub value: Value,
    pub label: String,
    pub region: Option<Value>,
}

#[derive(Clone, Debug, Default)]
pub struct Account {
    pub name: String,
    pub guilds: Vec<Choice>,
    pub regions: Vec<Choice>,
    pub visibilities: Vec<Choice>,
}

pub struct Report<'a> {
    pub file: &'a str,
    pub guild: Option<&'a Value>,
    pub region: Option<&'a Value>,
    pub visibility: Option<&'a Value>,
    pub description: &'a str,
    pub parser: i64,
    pub mode: u8,
}

pub struct Site {
    pub base: String,
    host: &'static str,
    version: String,
    jar: Vec<(String, String)>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn latest_version() -> Option<String> {
    let url = "https://api.github.com/repos/RPGLogs/Uploaders-archon/releases/latest";
    let headers = [("User-Agent", "warlogger"), ("Accept", "application/vnd.github+json")];
    let response = net::send("GET", url, &headers, Vec::new()).ok()?;
    let release: Value = serde_json::from_slice(&response.body).ok()?;

    let tag = release["tag_name"].as_str()?.trim_start_matches('v');
    (!tag.is_empty()).then(|| tag.to_owned())
}

fn message(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let json: Value = serde_json::from_str(&text).unwrap_or_default();

    ["message", "error", "reason"]
        .iter()
        .find_map(|k| json[k].as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .unwrap_or_else(|| text.chars().take(300).collect())
}

fn choices(list: &Value) -> Vec<Choice> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|item| Choice {
            value: item["value"].clone(),
            label: item["label"].as_str().unwrap_or_default().to_owned(),
            region: item.get("regionId").filter(|r| !r.is_null()).cloned(),
        })
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn multipart(fields: &[(&str, &str)], file: Vec<u8>) -> (String, Vec<u8>) {
    let boundary = format!("----WarloggerBoundary{:x}", now_ms());
    let mut body = Vec::with_capacity(file.len() + 512);

    for (name, value) in fields {
        body.extend(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").bytes(),
        );
    }

    body.extend(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"logfile\"; filename=\"blob\"\r\nContent-Type: application/zip\r\n\r\n"
        )
        .bytes(),
    );
    body.extend(file);
    body.extend(format!("\r\n--{boundary}--\r\n").bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let at = tag.find(&key)? + key.len();
    let rest = &tag[at..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &rest[1..];
    Some(rest[..rest.find(quote)?].replace("&amp;", "&"))
}

impl Site {
    pub fn new(game: &Game) -> Self {
        Self {
            base: game.base(),
            host: game.host,
            version: latest_version().unwrap_or_else(|| FALLBACK_VERSION.to_owned()),
            jar: Vec::new(),
        }
    }

    pub fn report_url(&self, code: &str) -> String {
        format!("{}/reports/{code}", self.base)
    }

    fn agent(&self) -> String {
        format!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ArchonApp/{} Chrome/138.0.7204.251 Electron/37.7.0 Safari/537.36",
            self.version
        )
    }

    fn store(&mut self, cookies: &[String]) {
        for cookie in cookies {
            let Some((name, value)) = cookie.split(';').next().and_then(|c| c.split_once('=')) else {
                continue;
            };

            let (name, value) = (name.trim().to_owned(), value.trim().to_owned());
            self.jar.retain(|(n, _)| *n != name);
            if !value.is_empty() && value != "deleted" {
                self.jar.push((name, value));
            }
        }
    }

    fn fetch(&mut self, method: &str, url: &str, kind: Option<&str>, body: Vec<u8>) -> Result<Vec<u8>, String> {
        let agent = self.agent();
        let same = url.starts_with(&self.base);
        let cookie = self
            .jar
            .iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        let xsrf = self.jar.iter().find(|(n, _)| n == "XSRF-TOKEN").map(|(_, v)| decode(v));

        let mut headers = vec![
            ("User-Agent", agent.as_str()),
            ("Accept", "application/json, text/html, */*"),
        ];
        if let Some(kind) = kind {
            headers.push(("Content-Type", kind));
        }
        if same && !cookie.is_empty() {
            headers.push(("Cookie", &cookie));
        }
        if let (true, Some(token)) = (same, xsrf.as_deref()) {
            headers.push(("X-XSRF-TOKEN", token));
        }

        for attempt in 0..=RETRIES {
            let retry = Duration::from_secs(1 << attempt);
            let response = match net::send(method, url, &headers, body.clone()) {
                Ok(r) => r,
                Err(e) if attempt < RETRIES => {
                    crate::log!("{url}: {e}, retrying");
                    std::thread::sleep(retry);
                    continue;
                }
                Err(e) => return Err(format!("{url}: {e}")),
            };

            if same {
                self.store(&response.cookies);
            }

            match response.status {
                200..=299 => return Ok(response.body),
                429 | 500.. if attempt < RETRIES => std::thread::sleep(retry),
                401 | 419 => return Err("not logged in; run `warlogger login`".into()),
                status => return Err(format!("{} (HTTP {status})", message(&response.body))),
            }
        }

        Err(format!("{url}: gave up after {RETRIES} retries"))
    }

    fn json(&mut self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        let url = format!("{}{path}", self.base);
        let (kind, body) = match body {
            Some(b) => (Some("application/json"), b.to_string().into_bytes()),
            None => (None, Vec::new()),
        };

        let bytes = self.fetch(method, &url, kind, body)?;
        match bytes.is_empty() {
            true => Ok(Value::Null),
            false => serde_json::from_slice(&bytes).map_err(|e| format!("{path}: {e}")),
        }
    }

    pub fn login(&mut self, email: &str, password: &str) -> Result<Account, String> {
        let body = json!({
            "email": email,
            "password": password,
            "version": self.version,
        });
        let v = self.json("POST", "/desktop-client/log-in", Some(body))?;

        let user = &v["user"];
        if !user.is_object() {
            return Err(format!(
                "login to {} failed: {}",
                self.host,
                message(v.to_string().as_bytes())
            ));
        }

        let pick = |key: &str| choices(if v[key].is_array() { &v[key] } else { &user[key] });
        Ok(Account {
            name: user["userName"]
                .as_str()
                .or(user["name"].as_str())
                .unwrap_or(email)
                .to_owned(),
            guilds: pick("guildSelectItems"),
            regions: pick("regionOrServerSelectItems"),
            visibilities: pick("reportVisibilitySelectItems"),
        })
    }

    pub fn parser(&mut self) -> Result<Vec<String>, String> {
        let url = format!(
            "{}/desktop-client/parser?id=1&ts={}&gameContentDetectionEnabled=false&metersEnabled=false&liveFightDataEnabled=false&gameVersionId=",
            self.base,
            now_ms()
        );
        let html = String::from_utf8_lossy(&self.fetch("GET", &url, None, Vec::new())?).into_owned();

        let mut scripts = Vec::new();
        let mut rest = html.as_str();
        while let Some(at) = rest.find("<script") {
            rest = &rest[at..];
            let Some(open) = rest.find('>') else { break };
            let Some(close) = rest.find("</script>") else { break };
            let (tag, code) = (&rest[..open], &rest[open + 1..close]);
            rest = &rest[close + 9..];

            let kind = attribute(tag, "type").unwrap_or_default();
            if !(kind.is_empty() || kind.contains("javascript")) {
                continue;
            }

            match attribute(tag, "src") {
                Some(src) if src.contains("rpglogs.") || src.starts_with('/') => {
                    let src = if src.starts_with('/') {
                        format!("{}{src}", self.base)
                    } else {
                        src
                    };
                    scripts.push(String::from_utf8_lossy(&self.fetch("GET", &src, None, Vec::new())?).into_owned());
                }
                Some(_) => {}
                None if !code.trim().is_empty() => scripts.push(code.to_owned()),
                None => {}
            }
        }

        match scripts.is_empty() {
            true => Err(format!("{url}: no parser scripts found")),
            false => Ok(scripts),
        }
    }

    pub fn create(&mut self, r: &Report) -> Result<String, String> {
        let now = now_ms();
        let body = json!({
            "clientVersion": self.version,
            "parserVersion": r.parser,
            "startTime": now,
            "endTime": now,
            "guildId": r.guild,
            "fileName": r.file,
            "serverOrRegion": r.region,
            "visibility": r.visibility,
            "reportTagId": null,
            "description": r.description,
            "logMode": r.mode,
        });

        let v = self.json("POST", "/desktop-client/create-report", Some(body))?;
        v["code"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("create-report: {}", message(v.to_string().as_bytes())))
    }

    pub fn master(&mut self, code: &str, segment: i64, players: &str, zip: Vec<u8>) -> Result<(), String> {
        let segment = segment.to_string();
        let fields = [
            ("segmentId", segment.as_str()),
            ("isRealTime", "false"),
            ("players", players),
        ];
        let (kind, body) = multipart(&fields, zip);
        let url = format!("{}/desktop-client/set-report-master-table/{code}", self.base);

        self.fetch("POST", &url, Some(&kind), body).map(drop)
    }

    pub fn segment(&mut self, code: &str, parameters: Value, zip: Vec<u8>) -> Result<i64, String> {
        let parameters = parameters.to_string();
        let (kind, body) = multipart(&[("parameters", &parameters)], zip);
        let url = format!("{}/desktop-client/add-report-segment/{code}", self.base);

        let bytes = self.fetch("POST", &url, Some(&kind), body)?;
        let v: Value = serde_json::from_slice(&bytes).unwrap_or_default();
        Ok(v["nextSegmentId"].as_i64().unwrap_or(0))
    }

    pub fn terminate(&mut self, code: &str) -> Result<(), String> {
        let url = format!("{}/desktop-client/terminate-report/{code}", self.base);
        self.fetch("POST", &url, None, Vec::new()).map(drop)
    }
}
