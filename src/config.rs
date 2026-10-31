use std::fs;

#[derive(Clone, Debug)]
pub struct Config {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub system_prompt: String,
    pub max_tool_rounds: usize,
    pub timeout_secs: u64,
    pub listen: String,
    pub ubus_url: String,
    pub ubus_user: String,
    pub ubus_pass: String,
}

pub const DEFAULT_SYSTEM_PROMPT: &str = "You are a read-only assistant running on an OpenWrt router. \
Answer the user's questions about this router by calling the provided tools to read its live state. \
You cannot change anything on the router; if asked to, explain what the user could do themselves. \
Report only what the tool output says; do not infer Wi-Fi standards, security modes or health \
beyond the fields provided, and say when a value is not in the data. Use plain text without emojis, \
and be concise. Never ask for or repeat secrets such as passwords or keys.";

impl Default for Config {
    fn default() -> Self {
        Config {
            endpoint: "https://api.openai.com/v1".into(),
            model: String::new(),
            api_key: String::new(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            max_tool_rounds: 6,
            timeout_secs: 60,
            listen: "127.0.0.1:8765".into(),
            ubus_url: "http://127.0.0.1/ubus".into(),
            ubus_user: "aiagent".into(),
            ubus_pass: String::new(),
        }
    }
}

impl Config {
    /// Load the `main` section of a UCI file. A missing file yields defaults.
    /// AIAGENT_ENDPOINT / AIAGENT_MODEL / AIAGENT_KEY override the file (dev use).
    pub fn load(path: &str) -> Config {
        let mut cfg = Config::default();
        if let Ok(text) = fs::read_to_string(path) {
            for (k, v) in parse_uci_section(&text, "aiagent") {
                cfg.set(&k, v);
            }
        }
        for (var, key) in [
            ("AIAGENT_ENDPOINT", "endpoint"),
            ("AIAGENT_MODEL", "model"),
            ("AIAGENT_KEY", "api_key"),
        ] {
            if let Ok(v) = std::env::var(var) {
                cfg.set(key, v);
            }
        }
        cfg
    }

    fn set(&mut self, key: &str, v: String) {
        match key {
            "endpoint" => self.endpoint = v,
            "model" => self.model = v,
            "api_key" => self.api_key = v,
            "system_prompt" if !v.trim().is_empty() => self.system_prompt = v,
            "max_tool_rounds" => self.max_tool_rounds = v.parse().unwrap_or(self.max_tool_rounds),
            "timeout" => self.timeout_secs = v.parse().unwrap_or(self.timeout_secs),
            "listen" => self.listen = v,
            "ubus_url" => self.ubus_url = v,
            "ubus_user" => self.ubus_user = v,
            "ubus_pass" => self.ubus_pass = v,
            _ => {}
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.endpoint.is_empty() && !self.model.is_empty()
    }
}

/// Parse `option k 'v'` lines of the first `config <section_type>` block.
fn parse_uci_section(text: &str, section_type: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    let mut seen = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = match line.split_once(char::is_whitespace) {
            Some((w, r)) => (w, r.trim()),
            None => (line, ""),
        };
        match word {
            "config" => {
                let ty = rest.split_whitespace().next().unwrap_or("");
                inside = !seen && ty == section_type;
                if inside {
                    seen = true;
                }
            }
            "option" if inside => {
                if let Some((k, v)) = rest.split_once(char::is_whitespace) {
                    out.push((k.to_string(), unquote(v.trim())));
                }
            }
            _ => {}
        }
    }
    out
}

fn unquote(v: &str) -> String {
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'\'' || b[0] == b'"') && b[b.len() - 1] == b[0] {
        let inner = &v[1..v.len() - 1];
        // UCI escapes an embedded single quote as '\''
        inner.replace("'\\''", "'")
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uci() {
        let t = "config aiagent 'main'\n\toption endpoint 'http://x/v1'\n\toption model \"m\"\n\toption timeout 30\n";
        let kv = parse_uci_section(t, "aiagent");
        assert_eq!(kv[0], ("endpoint".into(), "http://x/v1".into()));
        assert_eq!(kv[1], ("model".into(), "m".into()));
        assert_eq!(kv[2], ("timeout".into(), "30".into()));
    }
}
