use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::Duration;

pub trait Ubus: Send + Sync {
    fn call(&self, object: &str, method: &str, params: Value) -> Result<Value, String>;
}

const NULL_SESSION: &str = "00000000000000000000000000000000";

/// Talks to rpcd's JSON-RPC endpoint (uhttpd-mod-ubus) as the read-only user.
pub struct HttpUbus {
    url: String,
    user: String,
    pass: String,
    session: Mutex<Option<String>>,
}

impl HttpUbus {
    pub fn new(url: &str, user: &str, pass: &str) -> Self {
        HttpUbus {
            url: url.into(),
            user: user.into(),
            pass: pass.into(),
            session: Mutex::new(None),
        }
    }

    fn rpc(&self, sid: &str, object: &str, method: &str, params: &Value) -> Result<Value, String> {
        let body = json!({"jsonrpc":"2.0","id":1,"method":"call","params":[sid, object, method, params]});
        let resp: Value = ureq::post(&self.url)
            .timeout(Duration::from_secs(15))
            .send_json(body)
            .map_err(|e| format!("ubus http: {e}"))?
            .into_json()
            .map_err(|e| format!("ubus json: {e}"))?;
        if let Some(err) = resp.get("error") {
            return Err(format!("ubus rpc error: {err}"));
        }
        let result = resp.get("result").and_then(Value::as_array).ok_or("ubus: malformed response")?;
        let code = result.first().and_then(Value::as_i64).unwrap_or(-1);
        match code {
            0 => Ok(result.get(1).cloned().unwrap_or(Value::Null)),
            6 => Err("ubus: access denied".into()),
            c => Err(format!("ubus: call failed with status {c}")),
        }
    }

    fn login(&self) -> Result<String, String> {
        let r = self.rpc(
            NULL_SESSION,
            "session",
            "login",
            &json!({"username": self.user, "password": self.pass, "timeout": 3600}),
        )?;
        let sid = r["ubus_rpc_session"].as_str().ok_or("ubus: login returned no session")?.to_string();
        *self.session.lock().unwrap() = Some(sid.clone());
        Ok(sid)
    }
}

impl Ubus for HttpUbus {
    fn call(&self, object: &str, method: &str, params: Value) -> Result<Value, String> {
        let cached = self.session.lock().unwrap().clone();
        if let Some(sid) = cached {
            match self.rpc(&sid, object, method, &params) {
                Err(e) if e.contains("access denied") || e.contains("rpc error") => {} // maybe expired; retry below
                other => return other,
            }
        }
        let sid = self.login()?;
        self.rpc(&sid, object, method, &params)
    }
}

/// Canned responses keyed by "object.method" for development without a router.
pub struct MockUbus {
    data: Value,
}

impl MockUbus {
    pub fn builtin() -> Self {
        MockUbus { data: serde_json::from_str(include_str!("../fixtures/mock.json")).expect("valid fixture") }
    }
}

impl Ubus for MockUbus {
    fn call(&self, object: &str, method: &str, _params: Value) -> Result<Value, String> {
        self.data
            .get(format!("{object}.{method}"))
            .cloned()
            .ok_or_else(|| format!("mock: no fixture for {object}.{method}"))
    }
}
