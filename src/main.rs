mod agent;
mod config;
mod llm;
mod server;
mod tools;
mod ubus;

use std::sync::Arc;
use ubus::Ubus;

const USAGE: &str = "usage:
  aiagent serve [--config PATH] [--mock]   run the chat daemon
  aiagent ask \"question\" [--config PATH] [--mock]
  aiagent tools                            print the tool schemas

  --mock       use built-in canned router data instead of ubus
  --config     UCI file (default /etc/config/aiagent, or $AIAGENT_CONFIG)
  env overrides: AIAGENT_ENDPOINT, AIAGENT_MODEL, AIAGENT_KEY";

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mock = take_flag(&mut args, "--mock");
    let config_path = take_value(&mut args, "--config")
        .or_else(|| std::env::var("AIAGENT_CONFIG").ok())
        .unwrap_or_else(|| "/etc/config/aiagent".into());

    let result = match args.first().map(String::as_str) {
        Some("serve") => {
            let ubus = make_ubus(&config_path, mock);
            server::serve(&config_path, Arc::new(llm::HttpLlm), ubus)
        }
        Some("ask") if args.len() > 1 => ask(&config_path, mock, &args[1..].join(" ")),
        Some("tools") => {
            println!("{}", serde_json::to_string_pretty(&tools::definitions()).unwrap());
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("aiagent: {e}");
        std::process::exit(1);
    }
}

fn make_ubus(config_path: &str, mock: bool) -> Arc<dyn Ubus> {
    if mock {
        return Arc::new(ubus::MockUbus::builtin());
    }
    let c = config::Config::load(config_path);
    Arc::new(ubus::HttpUbus::new(&c.ubus_url, &c.ubus_user, &c.ubus_pass))
}

fn ask(config_path: &str, mock: bool, question: &str) -> Result<(), String> {
    let cfg = config::Config::load(config_path);
    if !cfg.is_configured() {
        return Err("endpoint/model not configured".into());
    }
    let ubus = make_ubus(config_path, mock);
    let mut history = Vec::new();
    let reply = agent::run_turn(&cfg, &llm::HttpLlm, ubus.as_ref(), &mut history, question)?;
    println!("{reply}");
    Ok(())
}

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != flag);
    args.len() != before
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Some(v)
}
