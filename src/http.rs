use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use serde::Deserialize;

pub async fn parse_json_response<T>(response: reqwest::Response, label: &str) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let body = response
        .text()
        .await
        .context("Failed to read response body")?;

    if status != StatusCode::OK {
        bail!("{label} returned {status}: {body}");
    }

    serde_json::from_str(&body).with_context(|| format!("Failed to parse {label} response: {body}"))
}
