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
        "wifi_status" => per_wifi_device(ubus, "info").map(|v| summarize_wifi(&v)),
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

/// Turn raw iwinfo output into plain facts. Small models misread terse fields such as
/// `wpa:[2,3]` or invent standards, so band, standards and security are spelled out here and
/// the ambiguous AP-side `signal`/`quality` fields are left out (client signal is in wifi_clients).
fn summarize_wifi(raw: &Value) -> Value {
    let mut out = Map::new();
    for (dev, i) in raw.as_object().into_iter().flatten() {
        let band = match i["frequency"].as_u64() {
            Some(f) if f < 3000 => "2.4 GHz",
            Some(f) if f < 5900 => "5 GHz",
            Some(_) => "6 GHz",
            None => "unknown",
        };
        out.insert(
            dev.clone(),
            json!({
                "ssid": i["ssid"], "mode": i["mode"], "band": band, "channel": i["channel"],
                "frequency_mhz": i["frequency"], "channel_width": i["htmode"],
                "supported_standards": i["hwmodes_text"], "security": wifi_security(&i["encryption"]),
                "tx_power_dbm": i["txpower"], "country": i["country"], "encryption_raw": i["encryption"],
            }),
        );
    }
    Value::Object(out)
}

fn wifi_security(e: &Value) -> String {
    if e["enabled"] != true {
        return "open (no encryption)".into();
    }
    let list = |k: &str| -> Vec<String> {
        e[k].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_uppercase)).collect()
    };
    let wpa: Vec<String> = e["wpa"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|v| if v == 1 { "WPA".to_string() } else { format!("WPA{v}") })
        .collect();
    if wpa.is_empty() {
        return "encrypted, not WPA (see encryption_raw)".into();
    }
    format!("{} ({}, {})", wpa.join("/"), list("authentication").join("+"), list("ciphers").join("+"))
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

    // values copied from a real Archer C6 v2: the model once reported WPA3 and "ac" on the wrong bands
    #[test]
    fn wifi_summary_is_spelled_out() {
        let raw = json!({
            "phy0-ap0": {"ssid":"Gilead","mode":"Master","channel":157,"frequency":5785,"htmode":"VHT80",
                "hwmodes_text":"ac/n","txpower":30,"country":"US","signal":-44,"quality":66,
                "encryption":{"enabled":true,"wpa":[2],"authentication":["psk"],"ciphers":["ccmp"]}},
            "phy1-ap0": {"ssid":"Gilead","mode":"Master","channel":1,"frequency":2412,"htmode":"HT20",
                "hwmodes_text":"b/g/n","txpower":20,"country":"US","signal":-48,"quality":62,
                "encryption":{"enabled":true,"wpa":[2,3],"authentication":["psk","sae"],"ciphers":["ccmp"]}}
        });
        let s = summarize_wifi(&raw);
        assert_eq!(s["phy0-ap0"]["band"], "5 GHz");
        assert_eq!(s["phy0-ap0"]["security"], "WPA2 (PSK, CCMP)");
        assert_eq!(s["phy1-ap0"]["band"], "2.4 GHz");
        assert_eq!(s["phy1-ap0"]["supported_standards"], "b/g/n");
        assert_eq!(s["phy1-ap0"]["security"], "WPA2/WPA3 (PSK+SAE, CCMP)");
        assert!(s["phy0-ap0"].get("signal").is_none() && s["phy0-ap0"].get("quality").is_none());
        assert_eq!(wifi_security(&json!({"enabled":false})), "open (no encryption)");
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
