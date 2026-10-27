use crate::config::Config;
use crate::llm::Llm;
use crate::tools;
use crate::ubus::Ubus;
use serde_json::{json, Value};

const MAX_HISTORY: usize = 40;

/// Run one user turn. `history` holds user/assistant/tool messages (no system prompt).
/// On error the history is rolled back to its state before this turn.
pub fn run_turn(
    cfg: &Config,
    llm: &dyn Llm,
    ubus: &dyn Ubus,
    history: &mut Vec<Value>,
    user: &str,
) -> Result<String, String> {
    let start = history.len();
    history.push(json!({"role":"user","content":user}));
    let result = turn_loop(cfg, llm, ubus, history);
    match &result {
        Ok(_) => trim(history),
        Err(_) => history.truncate(start),
    }
    result
}

fn turn_loop(cfg: &Config, llm: &dyn Llm, ubus: &dyn Ubus, history: &mut Vec<Value>) -> Result<String, String> {
    let defs = tools::definitions();
    for _ in 0..=cfg.max_tool_rounds {
        let mut messages = vec![json!({"role":"system","content":cfg.system_prompt})];
        messages.extend(history.iter().cloned());
        let msg = llm.complete(cfg, &messages, &defs)?;
        history.push(msg.clone());

        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {
            return Ok(msg["content"].as_str().unwrap_or("").to_string());
        }
        for call in calls {
            let name = call["function"]["name"].as_str().unwrap_or("");
            let args: Value = call["function"]["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_else(|| json!({}));
            let out = tools::run(ubus, name, &args);
            history.push(json!({"role":"tool","tool_call_id":call["id"],"content":out}));
        }
    }
    Err(format!("gave up after {} tool rounds without a final answer", cfg.max_tool_rounds))
}

fn trim(history: &mut Vec<Value>) {
    if history.len() <= MAX_HISTORY {
        return;
    }
    let mut drop = history.len() - MAX_HISTORY;
    // never start mid-exchange: advance to the next user message
    while drop < history.len() && history[drop]["role"] != "user" {
        drop += 1;
    }
    history.drain(..drop);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ubus::MockUbus;
    use std::sync::Mutex;

    struct Scripted(Mutex<Vec<Result<Value, String>>>, Mutex<Vec<Vec<Value>>>);
    impl Llm for Scripted {
        fn complete(&self, _: &Config, m: &[Value], _: &Value) -> Result<Value, String> {
            self.1.lock().unwrap().push(m.to_vec());
            self.0.lock().unwrap().remove(0)
        }
    }

    fn tool_call(name: &str) -> Value {
        json!({"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":name,"arguments":"{}"}}]})
    }

    #[test]
    fn tool_round_trip() {
        let llm = Scripted(
            Mutex::new(vec![Ok(tool_call("dhcp_leases")), Ok(json!({"role":"assistant","content":"2 clients"}))]),
            Mutex::new(vec![]),
        );
        let mut h = vec![];
        let out = run_turn(&Config::default(), &llm, &MockUbus::builtin(), &mut h, "who is connected?").unwrap();
        assert_eq!(out, "2 clients");
        let second = &llm.1.lock().unwrap()[1];
        let tool_msg = second.last().unwrap();
        assert_eq!(tool_msg["role"], "tool");
        assert!(tool_msg["content"].as_str().unwrap().contains("laptop"));
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn error_rolls_back_history() {
        let llm = Scripted(Mutex::new(vec![Err("boom".into())]), Mutex::new(vec![]));
        let mut h = vec![json!({"role":"user","content":"earlier"})];
        assert!(run_turn(&Config::default(), &llm, &MockUbus::builtin(), &mut h, "x").is_err());
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn round_limit() {
        let cfg = Config { max_tool_rounds: 2, ..Config::default() };
        let llm = Scripted(Mutex::new((0..10).map(|_| Ok(tool_call("system_info"))).collect()), Mutex::new(vec![]));
        let err = run_turn(&cfg, &llm, &MockUbus::builtin(), &mut vec![], "x").unwrap_err();
        assert!(err.contains("tool rounds"));
    }

    #[test]
    fn trim_starts_at_user() {
        let mut h: Vec<Value> = (0..30)
            .flat_map(|i| vec![json!({"role":"user","content":i}), json!({"role":"assistant","content":"a"})])
            .collect();
        trim(&mut h);
        assert!(h.len() <= MAX_HISTORY && h[0]["role"] == "user");
    }
}
