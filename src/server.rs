use crate::agent;
use crate::config::Config;
use crate::llm::Llm;
use crate::ubus::Ubus;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_BODY: u64 = 64 * 1024;
const MAX_SESSIONS: usize = 16;
const JOB_TTL: Duration = Duration::from_secs(600);

#[derive(Default)]
struct Session {
    history: Vec<Value>,
    busy: bool,
}

enum JobState {
    Pending,
    Done(Result<String, String>),
}

struct Job {
    state: JobState,
    created: Instant,
}

struct State {
    config_path: String,
    llm: Arc<dyn Llm>,
    ubus: Arc<dyn Ubus>,
    sessions: Mutex<HashMap<String, Session>>,
    jobs: Mutex<HashMap<String, Job>>,
    counter: AtomicU32,
}

pub fn serve(config_path: &str, llm: Arc<dyn Llm>, ubus: Arc<dyn Ubus>) -> Result<(), String> {
    let cfg = Config::load(config_path);
    let server = Server::http(&cfg.listen).map_err(|e| format!("cannot listen on {}: {e}", cfg.listen))?;
    eprintln!("aiagent listening on {}", cfg.listen);
    let state = Arc::new(State {
        config_path: config_path.into(),
        llm,
        ubus,
        sessions: Mutex::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
        counter: AtomicU32::new(1),
    });
    for req in server.incoming_requests() {
        handle(&state, req);
    }
    Ok(())
}

/// Always HTTP 200: the only client is the rpcd bridge using uclient-fetch, which drops the
/// body of non-2xx responses. Failures are reported as {"error": "..."} instead.
fn respond(req: Request, _code: u16, body: Value) {
    let h = Header::from_bytes("Content-Type", "application/json").unwrap();
    let _ = req.respond(Response::from_string(body.to_string()).with_header(h));
}

fn handle(state: &Arc<State>, mut req: Request) {
    let method = req.method().clone();
    let url = req.url().to_string();
    let mut raw = String::new();
    let _ = req.as_reader().take(MAX_BODY).read_to_string(&mut raw);
    let body: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    match (&method, url.as_str()) {
        (Method::Get, "/status") => {
            let cfg = Config::load(&state.config_path);
            respond(req, 200, json!({"ok":true,"configured":cfg.is_configured(),"model":cfg.model,"endpoint":cfg.endpoint}));
        }
        (Method::Post, "/chat") => match start_chat(state, &body) {
            Ok(job) => respond(req, 200, json!({"job": job})),
            Err((code, msg)) => respond(req, code, json!({"error": msg})),
        },
        (Method::Post, "/reset") => {
            let sid = body["session"].as_str().unwrap_or("default");
            state.sessions.lock().unwrap().remove(sid);
            respond(req, 200, json!({"ok":true}));
        }
        (Method::Get, u) if u.starts_with("/job/") => {
            let id = &u[5..];
            let mut jobs = state.jobs.lock().unwrap();
            let out = match jobs.get(id) {
                None => (404, json!({"error":"unknown job"})),
                Some(Job { state: JobState::Pending, .. }) => (200, json!({"state":"pending"})),
                Some(Job { state: JobState::Done(Ok(r)), .. }) => (200, json!({"state":"done","reply":r})),
                Some(Job { state: JobState::Done(Err(e)), .. }) => (200, json!({"state":"error","error":e})),
            };
            if out.1["state"] != "pending" && out.0 == 200 {
                jobs.remove(id);
            }
            drop(jobs);
            respond(req, out.0, out.1);
        }
        _ => respond(req, 404, json!({"error":"not found"})),
    }
}

fn start_chat(state: &Arc<State>, body: &Value) -> Result<String, (u16, String)> {
    let sid = body["session"].as_str().unwrap_or("default").to_string();
    let message = body["message"].as_str().unwrap_or("").trim().to_string();
    if message.is_empty() {
        return Err((400, "empty message".into()));
    }
    let cfg = Config::load(&state.config_path);
    if !cfg.is_configured() {
        return Err((400, "endpoint and model are not configured (Services > AI Agent > Settings)".into()));
    }

    let mut history = {
        let mut sessions = state.sessions.lock().unwrap();
        if !sessions.contains_key(&sid) && sessions.len() >= MAX_SESSIONS {
            return Err((503, "too many active sessions".into()));
        }
        let s = sessions.entry(sid.clone()).or_default();
        if s.busy {
            return Err((409, "previous message is still being processed".into()));
        }
        s.busy = true;
        std::mem::take(&mut s.history)
    };

    let job_id = format!("j{}", state.counter.fetch_add(1, Ordering::Relaxed));
    {
        let mut jobs = state.jobs.lock().unwrap();
        jobs.retain(|_, j| j.created.elapsed() < JOB_TTL);
        jobs.insert(job_id.clone(), Job { state: JobState::Pending, created: Instant::now() });
    }

    let st = Arc::clone(state);
    let jid = job_id.clone();
    std::thread::spawn(move || {
        let result = agent::run_turn(&cfg, st.llm.as_ref(), st.ubus.as_ref(), &mut history, &message);
        if let Some(s) = st.sessions.lock().unwrap().get_mut(&sid) {
            s.history = history;
            s.busy = false;
        }
        if let Some(j) = st.jobs.lock().unwrap().get_mut(&jid) {
            j.state = JobState::Done(result);
        }
    });
    Ok(job_id)
}
