//! Lingva (primary) + MyMemory (fallback) translation client.
//!
//! Mirrors the Python `f9_talk/translate/__init__.py` semantics:
//! - Empty / None text → return ""
//! - src == tgt → return text unchanged
//! - Lingva first, fall back to MyMemory on any exception
//! - On total failure: log warn + return the original text untouched

#![forbid(unsafe_code)]

use std::time::Duration;

use serde::Deserialize;
use tracing::{debug, warn};

const LINGVA_TIMEOUT: Duration = Duration::from_millis(4000);
const MYMEMORY_TIMEOUT: Duration = Duration::from_millis(5000);

pub struct Translator {
    client: reqwest::Client,
    src: String,
    tgt: String,
    mymemory_email: Option<String>,
}

impl Translator {
    /// Build a translator. `src`/`tgt` are ISO 639-1/3 codes (`en`, `ar`,
    /// etc.). When `src == tgt`, `translate()` short-circuits.
    pub fn new(src: impl Into<String>, tgt: impl Into<String>) -> Self {
        let mymemory_email = std::env::var("MYMEMORY_EMAIL")
            .ok()
            .filter(|s| !s.is_empty());
        Self {
            client: reqwest::Client::builder()
                .user_agent("f9-talk/0.4 (+https://github.com/moamen1358/F9_talk)")
                .build()
                .expect("reqwest client"),
            src: src.into(),
            tgt: tgt.into(),
            mymemory_email,
        }
    }

    pub fn pair(&self) -> (&str, &str) {
        (&self.src, &self.tgt)
    }

    /// Translate `text`. On any error, returns `text` unchanged so the
    /// dictation pipeline never hard-fails over a translation outage.
    pub async fn translate(&self, text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        if self.src == self.tgt {
            return text.to_string();
        }
        match self.lingva(text).await {
            Ok(out) => return out,
            Err(e) => debug!("lingva failed ({e}); falling back to mymemory"),
        }
        match self.mymemory(text).await {
            Ok(out) => out,
            Err(e) => {
                warn!(
                    "translate {}→{} failed: {e}; returning untranslated text",
                    self.src, self.tgt
                );
                text.to_string()
            }
        }
    }

    async fn lingva(&self, text: &str) -> Result<String, TranslateError> {
        // URL-encode the text into the path segment.
        let encoded = urlencoding::encode(text);
        let url = format!(
            "https://lingva.ml/api/v1/{}/{}/{}",
            self.src, self.tgt, encoded
        );
        let resp = self
            .client
            .get(&url)
            .timeout(LINGVA_TIMEOUT)
            .send()
            .await
            .map_err(|e| TranslateError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(TranslateError::HttpStatus(resp.status().as_u16()));
        }
        let body: LingvaBody = resp
            .json()
            .await
            .map_err(|e| TranslateError::Parse(e.to_string()))?;
        Ok(body.translation)
    }

    async fn mymemory(&self, text: &str) -> Result<String, TranslateError> {
        let langpair = format!("{}|{}", self.src, self.tgt);
        let mut req = self
            .client
            .get("https://api.mymemory.translated.net/get")
            .query(&[("q", text), ("langpair", &langpair)])
            .timeout(MYMEMORY_TIMEOUT);
        if let Some(email) = self.mymemory_email.as_ref() {
            req = req.query(&[("de", email.as_str())]);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| TranslateError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(TranslateError::HttpStatus(resp.status().as_u16()));
        }
        let body: MyMemoryBody = resp
            .json()
            .await
            .map_err(|e| TranslateError::Parse(e.to_string()))?;
        // MyMemory returns `responseStatus` as either a JSON number or
        // string; in either form, anything other than 200 is a soft
        // failure (rate-limited, unsupported pair, etc.).
        if body.response_status.as_str() != "200" {
            return Err(TranslateError::Provider(format!(
                "mymemory status {} ({})",
                body.response_status, body.response_details
            )));
        }
        Ok(body.response_data.translated_text)
    }
}

#[derive(Deserialize)]
struct LingvaBody {
    translation: String,
}

#[derive(Deserialize)]
struct MyMemoryBody {
    #[serde(rename = "responseStatus", deserialize_with = "stringy_status")]
    response_status: String,
    #[serde(rename = "responseDetails", default)]
    response_details: String,
    #[serde(rename = "responseData")]
    response_data: MyMemoryData,
}

#[derive(Deserialize)]
struct MyMemoryData {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

/// MyMemory returns `responseStatus` as either a number or a string
/// depending on the request — coerce both to a string for comparison.
fn stringy_status<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let v: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        other => Err(Error::custom(format!(
            "responseStatus must be string or number, got {other:?}"
        ))),
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TranslateError {
    #[error("network: {0}")]
    Network(String),
    #[error("http {0}")]
    HttpStatus(u16),
    #[error("parse: {0}")]
    Parse(String),
    #[error("provider: {0}")]
    Provider(String),
}
