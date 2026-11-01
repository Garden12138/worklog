//! xcli local transport. Never reads native CLI logins or silently switches providers.
use crate::error::{AppError, AppResult};
use crate::llm::LlmConfig;
use reqwest::{Client, Response};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    pub model: String,
    pub inference_location: String,
}

pub fn validate_endpoint(value: &str) -> AppResult<String> {
    let parsed = url::Url::parse(value).map_err(|_| invalid_endpoint())?;
    let loopback = match parsed.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if parsed.scheme() != "http"
        || !loopback
        || parsed.port().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path().trim_end_matches('/') != "/v1"
    {
        return Err(invalid_endpoint());
    }
    Ok(value.trim_end_matches('/').into())
}

fn invalid_endpoint() -> AppError {
    AppError::validation(
        "base_url",
        "xcli requires an explicit loopback IP, port and /v1 path",
    )
}

pub fn validate_config(config: &LlmConfig) -> AppResult<()> {
    validate_endpoint(&config.base_url)?;
    if config.extra_headers.keys().any(|key| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "authorization"
                | "host"
                | "origin"
                | "cookie"
                | "proxy-authorization"
                | "x-xcli-remote-consent"
        )
    }) {
        return Err(AppError::validation(
            "extra_headers",
            "xcli authentication and privacy headers cannot be overridden",
        ));
    }
    Ok(())
}

pub fn client(timeout_seconds: i64) -> AppResult<Client> {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(timeout_seconds.clamp(5, 600) as u64))
        .build()
        .map_err(|_| AppError::new("llm_error", "Unable to initialize the local gateway client"))
}

pub fn transport_error() -> AppError {
    AppError::new(
        "llm_unreachable",
        "Unable to reach xcli; check that Model Gateway is running",
    )
}

pub fn status_error(status: u16) -> AppError {
    let message = match status {
        401 => "xcli rejected the local application token; check issuance or revocation",
        403 => "xcli denied this model scope or upstream data-location permission",
        429 => "xcli or its upstream reached a rate or quota limit",
        504 => "xcli model request timed out",
        _ => "xcli returned an unsuccessful response",
    };
    AppError::new("llm_error", format!("{message} (HTTP {status})"))
}

pub async fn json_response(mut response: Response) -> AppResult<Value> {
    if !response.status().is_success() {
        return Err(status_error(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| transport_error())? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err(AppError::new(
                "llm_error",
                "xcli response exceeds the Worklog limit",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| AppError::new("llm_error", "xcli returned invalid JSON"))
}

pub async fn probe_with(client: &Client, config: &LlmConfig) -> AppResult<ConnectionInfo> {
    validate_config(config)?;
    if config.api_key.trim().is_empty() {
        return Err(AppError::new(
            "llm_not_configured",
            "An independent xcli application token is required",
        ));
    }
    let response = client
        .get(format!("{}/models", validate_endpoint(&config.base_url)?))
        .bearer_auth(&config.api_key)
        .send()
        .await
        .map_err(|_| transport_error())?;
    let catalog = json_response(response).await?;
    let models = catalog
        .get("data")
        .and_then(Value::as_array)
        .filter(|_| catalog.get("object").and_then(Value::as_str) == Some("list"))
        .ok_or_else(|| AppError::new("llm_error", "xcli model catalog is invalid"))?;
    let selected: Vec<_> = models
        .iter()
        .filter(|model| model.get("id").and_then(Value::as_str) == Some(&config.model))
        .collect();
    if selected.len() != 1
        || !selected[0]
            .pointer("/xcli/capabilities")
            .and_then(Value::as_array)
            .is_some_and(|caps| caps.iter().any(|cap| cap.as_str() == Some("text.generate")))
    {
        return Err(AppError::new(
            "llm_error",
            "The configured xcli alias is missing or cannot generate text",
        ));
    }
    let location = selected[0]
        .pointer("/xcli/inference_location")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Ok(ConnectionInfo {
        model: config.model.clone(),
        inference_location: match location {
            "local" | "remote" => location,
            _ => "unknown",
        }
        .into(),
    })
}

pub async fn probe(config: &LlmConfig) -> AppResult<ConnectionInfo> {
    probe_with(&client(config.timeout_seconds)?, config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    fn config(url: String) -> LlmConfig {
        LlmConfig {
            provider: "xcli".into(),
            base_url: url,
            model: "xcli/text-default".into(),
            api_key: "synthetic-private-token".into(),
            extra_headers: BTreeMap::new(),
            timeout_seconds: 5,
        }
    }

    #[test]
    fn validates_local_transport_and_redacts_errors() {
        assert!(validate_endpoint("http://127.0.0.1:18741/v1").is_ok());
        assert!(validate_endpoint("http://[::1]:18741/v1/").is_ok());
        for bad in [
            "http://localhost:18741/v1",
            "http://192.168.1.1:18741/v1",
            "https://example.com/v1",
            "http://user@127.0.0.1:18741/v1",
            "http://127.0.0.1:18741/v1?token=secret",
            "http://127.0.0.1:18741/other",
        ] {
            assert!(validate_endpoint(bad).is_err());
        }
        let mut settings = config("http://127.0.0.1:18741/v1".into());
        settings
            .extra_headers
            .insert("Authorization".into(), "private-canary".into());
        assert!(validate_config(&settings).is_err());
        assert!(!format!("{:?}", status_error(401)).contains("synthetic-private-token"));
    }

    #[tokio::test]
    async fn worklog_requests_catalog_then_chat_without_sending_prompts_to_models() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let settings = config(format!("http://{}/v1", listener.local_addr().unwrap()));
        let server = std::thread::spawn(move || {
            for path in ["GET /v1/models", "POST /v1/chat/completions"] {
                let (mut socket, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert!(first.starts_with(path));
                let mut length = 0;
                let mut auth = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if lower.starts_with("content-length:") {
                        length = line.split(':').nth(1).unwrap().trim().parse().unwrap();
                    }
                    if lower.starts_with("authorization:") {
                        auth = line.contains("synthetic-private-token");
                    }
                }
                assert!(auth);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let reply = if path.starts_with("GET") {
                    assert!(body.is_empty());
                    r#"{"object":"list","data":[{"id":"xcli/text-default","xcli":{"capabilities":["text.generate"],"inference_location":"remote"}}]}"#
                } else {
                    let value: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(value["model"], "xcli/text-default");
                    r#"{"choices":[{"message":{"content":"synthetic report"}}]}"#
                };
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
            }
        });
        let result = crate::llm::generate_report(
            Some(&settings),
            "weekly_report",
            "2026-10-19",
            "2026-10-25",
            &[],
        )
        .await
        .unwrap();
        assert!(result.used_llm);
        assert_eq!(result.content, "synthetic report");
        server.join().unwrap();
    }
}
