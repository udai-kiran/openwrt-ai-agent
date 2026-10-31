use crate::ubus::Ubus;
use serde_json::{json, Map, Value};

const MAX_TOOL_BYTES: usize = 6000;
const UCI_ALLOWLIST: [&str; 5] = ["network", "wireless", "dhcp", "firewall", "system"];
const SECRET_HINTS: [&str; 6] = ["key", "psk", "password", "secret", "passphrase", "token"];

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"type":"function","function":{
        "name": name,
        "description": description,
        "parameters": {"type":"object","properties": properties,"required": required}
    }})
}

pub fn definitions() -> Value {
    json!([
        tool("system_info", "Board model, OpenWrt release, kernel, uptime, load average and memory usage.", json!({}), &[]),
        tool("network_interfaces", "All logical network interfaces (lan, wan, ...) with protocol, up/down state, IP addresses, routes and DNS servers.", json!({}), &[]),
        tool("wifi_status", "Wi-Fi interfaces: SSID, channel, mode, encryption, signal, tx power. Keys are redacted.", json!({}), &[]),
        tool("wifi_clients", "Wireless stations currently associated to each Wi-Fi interface, with signal strength and rates.", json!({}), &[]),
        tool("dhcp_leases", "Active DHCP leases: hostname, IP and MAC address of LAN clients.", json!({}), &[]),
        tool("system_log", "Most recent lines of the system log (logd).",
            json!({"lines":{"type":"integer","description":"How many recent lines, 1-200 (default 50)."}}), &[]),
        tool("uci_get", "Read an OpenWrt UCI config file. Secrets are redacted.",
            json!({"config":{"type":"string","enum": UCI_ALLOWLIST,"description":"Config file name."},
                   "section":{"type":"string","description":"Optional section name to read only that section."}}),
            &["config"]),
    ])
}

/// Run a tool and return text for the model. Errors are returned as text so the model can react.
pub fn run(ubus: &dyn Ubus, name: &str, args: &Value) -> String {
    match dispatch(ubus, name, args) {
        Ok(mut v) => {
            redact(&mut v);
            truncate(serde_json::to_string(&v).unwrap_or_default())
        }
        Err(e) => format!("error: {e}"),
    }
}

fn dispatch(ubus: &dyn Ubus, name: &str, args: &Value) -> Result<Value, String> {
    let none = json!({});
    match name {
        "system_info" => Ok(json!({
            "board": ubus.call("system", "board", none.clone())?,
            "info": ubus.call("system", "info", none)?,
        })),
        "network_interfaces" => ubus.call("network.interface", "dump", none),
        "wifi_status" => per_wifi_device(ubus, "info"),
        "wifi_clients" => per_wifi_device(ubus, "assoclist"),
        "dhcp_leases" => ubus.call("luci-rpc", "getDHCPLeases", none),
        "system_log" => {
            let n = args["lines"].as_i64().unwrap_or(50).clamp(1, 200);
            ubus.call("log", "read", json!({"lines": n, "stream": false, "oneshot": true}))
        }
        "uci_get" => {
            let config = args["config"].as_str().ok_or("missing 'config'")?;
            if !UCI_ALLOWLIST.contains(&config) {
                return Err(format!("config '{config}' is not readable; allowed: {}", UCI_ALLOWLIST.join(", ")));
            }
            let mut p = json!({"config": config});
            if let Some(s) = args["section"].as_str() {
                p["section"] = json!(s);
            }
            ubus.call("uci", "get", p)
        }
        other => Err(format!("unknown tool '{other}'")),
    }
}

/// Run an iwinfo method for every wireless interface. (network.wireless status is not used:
/// netifd rejects the session field ubus injects for restricted logins.)
fn per_wifi_device(ubus: &dyn Ubus, method: &str) -> Result<Value, String> {
    let devs = ubus.call("iwinfo", "devices", json!({}))?;
    let mut out = Map::new();
    for d in devs["devices"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        out.insert(d.to_string(), ubus.call("iwinfo", method, json!({"device": d}))?);
    }
    Ok(Value::Object(out))
}

fn redact(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                let lk = k.to_ascii_lowercase();
                if SECRET_HINTS.iter().any(|h| lk.contains(h)) {
                    *val = json!("[redacted]");
                } else {
                    redact(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(redact),
        _ => {}
    }
}

fn truncate(mut s: String) -> String {
    if s.len() > MAX_TOOL_BYTES {
        let mut cut = MAX_TOOL_BYTES;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("...[truncated]");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ubus::MockUbus;

    #[test]
    fn uci_wifi_keys_are_redacted() {
        let out = run(&MockUbus::builtin(), "uci_get", &json!({"config":"wireless"}));
        assert!(!out.contains("hunter2"));
        assert!(out.contains("[redacted]"));
        assert!(out.contains("HomeNet"));
    }

    #[test]
    fn wifi_status_reads_iwinfo() {
        let out = run(&MockUbus::builtin(), "wifi_status", &json!({}));
        assert!(out.contains("phy0-ap0") && out.contains("HomeNet"));
    }

    #[test]
    fn clients_aggregate_per_device() {
        let out = run(&MockUbus::builtin(), "wifi_clients", &json!({}));
        assert!(out.contains("phy0-ap0") && out.contains("AA:BB:CC:DD:EE:01"));
    }

    #[test]
    fn uci_allowlist_enforced() {
        let out = run(&MockUbus::builtin(), "uci_get", &json!({"config":"rpcd"}));
        assert!(out.starts_with("error:"));
    }

    #[test]
    fn truncation_respects_char_boundary() {
        let s = truncate("é".repeat(MAX_TOOL_BYTES));
        assert!(s.ends_with("...[truncated]"));
    }
}
