use super::Response;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::Agent;
use ureq::http::Request;

const LIMIT: u64 = 512 << 20;

fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();

    AGENT.get_or_init(|| {
        Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into()
    })
}

pub fn send(method: &str, url: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Result<Response, String> {
    let mut request = Request::builder().method(method).uri(url);
    for (key, value) in headers {
        request = request.header(*key, *value);
    }

    let request = request.body(body).map_err(|e| e.to_string())?;
    let mut response = agent().run(request).map_err(|e| e.to_string())?;

    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok().map(str::to_owned))
        .collect();

    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let body = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| e.to_string())?;

    Ok(Response {
        status: response.status().as_u16(),
        cookies,
        location,
        body,
    })
}
