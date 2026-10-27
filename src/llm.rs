use crate::config::Config;
use serde_json::{json, Value};
use std::time::Duration;

pub trait Llm: Send + Sync {
    /// Returns the assistant message object from an OpenAI-style chat completion.
    fn complete(&self, cfg: &Config, messages: &[Value], tools: &Value) -> Result<Value, String>;
}

pub struct HttpLlm;

fn completions_url(endpoint: &str) -> String {
    let e = endpoint.trim_end_matches('/');
    if e.ends_with("/chat/completions") {
        e.to_string()
    } else {
        format!("{e}/chat/completions")
    }
}

impl Llm for HttpLlm {
    fn complete(&self, cfg: &Config, messages: &[Value], tools: &Value) -> Result<Value, String> {
        let body = json!({"model": cfg.model, "messages": messages, "tools": tools, "tool_choice": "auto"});
        let mut req = ureq::post(&completions_url(&cfg.endpoint)).timeout(Duration::from_secs(cfg.timeout_secs));
        if !cfg.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", cfg.api_key));
        }
        let resp = match req.send_json(body) {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                let text = r.into_string().unwrap_or_default();
                let snippet: String = text.chars().take(300).collect();
                return Err(format!("LLM endpoint returned HTTP {code}: {snippet}"));
            }
            Err(e) => return Err(format!("LLM request failed: {e}")),
        };
        let v: Value = resp.into_json().map_err(|e| format!("LLM response not JSON: {e}"))?;
        v["choices"][0]["message"]
            .as_object()
            .map(|m| Value::Object(m.clone()))
            .ok_or_else(|| "LLM response had no choices[0].message".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_building() {
        assert_eq!(completions_url("http://h/v1/"), "http://h/v1/chat/completions");
        assert_eq!(completions_url("http://h/v1/chat/completions"), "http://h/v1/chat/completions");
    }
}
