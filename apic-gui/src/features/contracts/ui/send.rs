//! The contracts feature's send state: the selected environment, the request
//! in flight and the live reply. `ContractsState` holds one [`SendState`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use apic_core::edit::EditModel;
use apic_core::http::Reply;
use apic_core::json::JsonContent;
use apic_core::vars::{Vars, fill_path_params, path_params, substitute};
use eframe::egui;

use crate::features::contracts::data::send as send_job;
use crate::features::contracts::ui::state::ContractsEvent;

/// The most of a reply body the Live pane shows, in bytes.
pub(crate) const DISPLAY_LIMIT: usize = 2 * 1024 * 1024;

/// A reply body as the Live pane shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LiveBody {
    /// A body whose `Content-Type` names JSON, for the highlighted code view.
    Json(String),
    /// Any other UTF-8 body.
    Text(String),
    /// A body that is not UTF-8, by its length in bytes.
    Binary(usize),
}

/// A received reply, prepared once for drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveReply {
    /// The status code, e.g. 200.
    pub(crate) status: u16,

    /// The reason phrase, empty when unknown.
    pub(crate) reason: String,

    /// Time from sending to the end of the body, in milliseconds.
    pub(crate) millis: u128,

    /// The whole body's length in bytes.
    pub(crate) size: usize,

    /// The reply headers in received order.
    pub(crate) headers: Vec<(String, String)>,

    /// The body, cut at [`DISPLAY_LIMIT`].
    pub(crate) body: LiveBody,

    /// Whether the body was cut.
    pub(crate) truncated: bool,
}

impl LiveReply {
    /// Prepares `reply` for drawing: decodes the body once and cuts it at
    /// [`DISPLAY_LIMIT`] on a character boundary.
    pub(crate) fn from_reply(reply: &Reply) -> Self {
        let json = reply.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value.to_ascii_lowercase().contains("json")
        });
        let (body, truncated) = match std::str::from_utf8(&reply.body) {
            Ok(text) => {
                let mut end = text.len().min(DISPLAY_LIMIT);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let shown = text[..end].to_string();
                let body = if json {
                    LiveBody::Json(shown)
                } else {
                    LiveBody::Text(shown)
                };
                (body, end < text.len())
            }
            Err(_) => (LiveBody::Binary(reply.body.len()), false),
        };
        LiveReply {
            status: reply.status,
            reason: reply.reason.clone(),
            millis: reply.elapsed.as_millis(),
            size: reply.body.len(),
            headers: reply.headers.clone(),
            body,
            truncated,
        }
    }
}

/// Everything about sending the open contract. `ContractsState` holds one.
#[derive(Default)]
pub(crate) struct SendState {
    /// The selected environment, `None` for no environment.
    pub(crate) env: Option<String>,

    /// The selected environment's values as last loaded. Only their names
    /// are used, for the variable colours. Never drawn.
    pub(crate) env_vars: Vars,

    /// The environment names listed the last time the menu was opened.
    pub(crate) envs: Vec<String>,

    /// Whether the right pane shows the live reply instead of the examples.
    pub(crate) show_live: bool,

    /// The last reply, or why the last send failed. `None` before any send
    /// and while one is in flight.
    pub(crate) live: Option<Result<LiveReply, String>>,

    /// The in-flight send's channel, `None` when nothing is in flight.
    pub(crate) pending: Option<Receiver<Result<Reply, String>>>,

    /// The path-param popup's `(name, value)` rows while it is open.
    pub(crate) prompt: Option<Vec<(String, String)>>,

    /// Path-param values typed this session, by contract path, to prefill
    /// the popup next time.
    pub(crate) params: HashMap<PathBuf, Vars>,
}

impl SendState {
    /// Re-reads the environment names in `apic_dir`.
    pub(crate) fn refresh_envs(&mut self, apic_dir: &Path) {
        self.envs = apic_core::env::list(apic_dir)
            .into_iter()
            .map(|entry| entry.name)
            .collect();
    }

    /// Selects `name` and loads its values for the variable colours.
    ///
    /// # Arguments
    ///
    /// * `name` - the environment, `None` for no environment
    /// * `apic_dir` - the project's `.apic` directory, `None` with no project
    ///
    /// # Returns
    ///
    /// [`ContractsEvent::EnvChosen`], after the load error when the
    /// environment does not load.
    pub(crate) fn choose_env(
        &mut self,
        name: Option<String>,
        apic_dir: Option<&Path>,
    ) -> Vec<ContractsEvent> {
        let mut events = Vec::new();
        self.env_vars = match (name.as_deref(), apic_dir) {
            (Some(name), Some(dir)) => match apic_core::env::load(dir, name) {
                Ok(vars) => vars,
                Err(err) => {
                    events.push(ContractsEvent::Status(err));
                    Vars::new()
                }
            },
            _ => Vars::new(),
        };
        self.env = name.clone();
        events.push(ContractsEvent::EnvChosen(name));
        events
    }

    /// Selects the saved environment `saved` when `apic_dir` still lists it,
    /// and no environment otherwise.
    pub(crate) fn restore_env(&mut self, saved: Option<&str>, apic_dir: &Path) {
        self.refresh_envs(apic_dir);
        let name = saved
            .filter(|saved| self.envs.iter().any(|e| e == saved))
            .map(str::to_string);
        self.env_vars = name
            .as_deref()
            .and_then(|name| apic_core::env::load(apic_dir, name).ok())
            .unwrap_or_default();
        self.env = name;
    }

    /// Whether a send is in flight.
    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// How many headers the live reply has, 0 without one.
    pub(crate) fn live_header_count(&self) -> usize {
        match &self.live {
            Some(Ok(reply)) => reply.headers.len(),
            _ => 0,
        }
    }

    /// Drops the live reply and any send in flight, for a newly loaded
    /// contract.
    pub(crate) fn clear(&mut self) {
        self.live = None;
        self.pending = None;
    }

    /// Drops the send in flight. Its result is discarded when it arrives.
    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }

    /// Collects the in-flight send's result if it has arrived, and asks for
    /// another frame in 100 ms while it has not. Called every frame.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.pending else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.pending = None;
                self.live = Some(result.map(|reply| LiveReply::from_reply(&reply)));
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.live = Some(Err("The send stopped before it finished".to_string()));
            }
        }
    }

    /// Starts sending `model` as shown, unsaved edits included, and shows the
    /// Live pane. The file is never written. When the URL still has `{name}`
    /// path params once the environment is applied, opens the path-param
    /// popup instead, prefilled with the values typed for `key` earlier this
    /// session. A request that cannot be built, or an environment that does
    /// not load, becomes the Live pane's error.
    ///
    /// # Arguments
    ///
    /// * `model` - the open contract
    /// * `key` - the open contract's path, which keys the remembered values
    /// * `apic_dir` - the project's `.apic` directory, `None` with no project
    ///
    /// # Returns
    ///
    /// A status-line warning naming the undefined variables in the URL and
    /// headers, or "no project".
    pub(crate) fn send(
        &mut self,
        model: &EditModel,
        key: Option<&Path>,
        apic_dir: Option<&Path>,
    ) -> Vec<ContractsEvent> {
        self.send_with(model, key, apic_dir, Vars::new())
    }

    /// Sends with the popup's values and remembers them for `key`. A blank
    /// value keeps the popup open and sends nothing.
    ///
    /// # Arguments
    ///
    /// * `model` - the open contract
    /// * `key` - the open contract's path
    /// * `apic_dir` - the project's `.apic` directory, `None` with no project
    ///
    /// # Returns
    ///
    /// What [`SendState::send`] returns.
    pub(crate) fn submit_params(
        &mut self,
        model: &EditModel,
        key: Option<&Path>,
        apic_dir: Option<&Path>,
    ) -> Vec<ContractsEvent> {
        let Some(values) = self.prompt.take() else {
            return Vec::new();
        };
        if values.iter().any(|(_, value)| value.trim().is_empty()) {
            self.prompt = Some(values);
            return Vec::new();
        }
        let extra: Vars = values.into_iter().collect();
        if let Some(key) = key {
            self.params
                .entry(key.to_path_buf())
                .or_default()
                .extend(extra.clone());
        }
        self.send_with(model, key, apic_dir, extra)
    }

    /// [`SendState::send`] with `extra` values laid over the environment.
    fn send_with(
        &mut self,
        model: &EditModel,
        key: Option<&Path>,
        apic_dir: Option<&Path>,
        extra: Vars,
    ) -> Vec<ContractsEvent> {
        let mut events = Vec::new();
        if self.is_pending() {
            return events;
        }
        let Some(apic_dir) = apic_dir else {
            events.push(ContractsEvent::Status("no project".into()));
            return events;
        };
        let (contract, mut vars) = match self.build(model, apic_dir) {
            Ok(built) => built,
            Err(err) => {
                self.show_live = true;
                self.live = Some(Err(err));
                return events;
            }
        };
        vars.extend(extra);
        let url = fill_path_params(&substitute(&contract.url, &vars).0, &vars);
        let missing = path_params(&url);
        if !missing.is_empty() {
            let saved = key.and_then(|k| self.params.get(k));
            let rows = missing
                .into_iter()
                .map(|name| {
                    let value = saved
                        .and_then(|s| s.get(&name))
                        .cloned()
                        .unwrap_or_default();
                    (name, value)
                })
                .collect();
            self.prompt = Some(rows);
            return events;
        }
        self.show_live = true;
        self.start(contract, vars, apic_dir, &mut events);
        events
    }

    /// Makes sure `.apic/.gitignore` keeps local env files out of git,
    /// reloads the selected environment from disk and turns `model` into the
    /// contract to send.
    fn build(&mut self, model: &EditModel, apic_dir: &Path) -> Result<(JsonContent, Vars), String> {
        apic_core::env::ensure_gitignore(apic_dir)?;
        let vars = match &self.env {
            Some(name) => apic_core::env::load(apic_dir, name)?,
            None => Vars::new(),
        };
        self.env_vars = vars.clone();
        let json = model.to_json()?;
        let contract = apic_core::json::json_get(&json, None).map_err(|err| err.to_string())?;
        Ok((contract, vars))
    }

    /// Warns about undefined names, then starts the background send.
    fn start(
        &mut self,
        contract: JsonContent,
        vars: Vars,
        apic_dir: &Path,
        events: &mut Vec<ContractsEvent>,
    ) {
        let undefined = undefined_names(&contract, &vars);
        if !undefined.is_empty() {
            events.push(ContractsEvent::Status(format!(
                "not defined, sent as written: {}",
                undefined.join(", ")
            )));
        }
        let project_root = apic_dir.parent().unwrap_or(apic_dir).to_path_buf();
        self.live = None;
        self.pending = Some(send_job::start(contract, vars, project_root));
    }
}

/// The undefined variables in `contract`'s URL and header values, each once
/// and written `$name`.
fn undefined_names(contract: &JsonContent, vars: &Vars) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let texts = std::iter::once(contract.url.as_str())
        .chain(contract.headers.iter().map(|h| h.value.as_str()));
    for text in texts {
        for name in substitute(text, vars).1 {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.into_iter().map(|name| format!("${name}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::test_support::{serve_once, tempdir};
    use crate::features::contracts::ui::state::{ContractsAction, ContractsPaths, ContractsState};

    /// A 200 reply with a small JSON body.
    const OK: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 7\r\nConnection: close\r\n\r\n{\"a\":1}";

    /// `GET $base/hello` with an `X-Token: $token` header.
    const CALL: &str = r#"{ "name": "call", "method": "GET", "url": "$base/hello",
        "headers": [ { "name": "X-Token", "value": "$token" } ], "responses": [] }"#;

    /// Writes `.apic/env/dev.toml` with `base` set to `url` and, when given,
    /// `token`.
    fn write_env(apic_dir: &Path, url: &str, token: Option<&str>) {
        let dir = apic_dir.join("env");
        std::fs::create_dir_all(&dir).expect("env dir creates");
        let mut text = format!("base = {url:?}\n");
        if let Some(token) = token {
            text.push_str(&format!("token = {token:?}\n"));
        }
        std::fs::write(dir.join("dev.toml"), text).expect("env file writes");
    }

    /// A project holding `contract` as `contracts/call.json` and a `dev` env
    /// whose `base` is `url` and whose `token` is `one`, with the contract
    /// open and `dev` selected.
    fn open_contract(url: &str, contract: &str) -> (ContractsState, ContractsPaths) {
        let root = tempdir();
        let contracts_root = root.join("contracts");
        std::fs::create_dir_all(&contracts_root).expect("contracts dir creates");
        std::fs::write(contracts_root.join("call.json"), contract).expect("contract writes");
        let apic_dir = root.join(".apic");
        write_env(&apic_dir, url, Some("one"));
        let paths = ContractsPaths {
            contracts_root: Some(contracts_root),
            apic_dir,
        };
        let mut state = ContractsState::default();
        state.reload(&paths);
        let i = state
            .entries
            .iter()
            .position(|e| e.rel == "call.json")
            .expect("call.json is listed");
        state.handle(ContractsAction::Load(i), Some(&paths));
        assert!(state.model.is_some(), "call.json opens");
        state.send.env = Some("dev".to_string());
        (state, paths)
    }

    /// Polls until the send finishes, with a bound so a stuck send fails
    /// instead of hanging the suite.
    fn settle_send(state: &mut ContractsState) {
        let ctx = egui::Context::default();
        for _ in 0..500 {
            state.send.poll(&ctx);
            if !state.send.is_pending() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the send did not finish within 500 polls");
    }

    /// The raw request the one-shot server received, lowercased.
    fn received(rx: &std::sync::mpsc::Receiver<Vec<u8>>) -> String {
        let bytes = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the server received a request");
        String::from_utf8_lossy(&bytes).to_ascii_lowercase()
    }

    #[test]
    fn send_shows_live_and_delivers_the_on_screen_contract() {
        let (url, rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL);
        let file = state.path.clone().expect("contract is open");
        let before = std::fs::read_to_string(&file).expect("contract reads");
        state.model.as_mut().expect("contract is open").url = "$base/edited".to_string();

        state.handle(ContractsAction::Send, Some(&paths));

        assert!(state.send.is_pending());
        assert!(state.send.show_live, "Send selects Live");
        settle_send(&mut state);
        let Some(Ok(reply)) = &state.send.live else {
            panic!("no reply: {:?}", state.send.live);
        };
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, LiveBody::Json("{\"a\":1}".to_string()));
        assert!(received(&rx).starts_with("get /edited "));
        assert_eq!(
            std::fs::read_to_string(&file).expect("contract reads"),
            before
        );
    }

    #[test]
    fn cancel_discards_the_reply() {
        let (url, rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL);

        state.handle(ContractsAction::Send, Some(&paths));
        state.handle(ContractsAction::CancelSend, Some(&paths));

        assert!(!state.send.is_pending());
        received(&rx);
        state.send.poll(&egui::Context::default());
        assert!(state.send.live.is_none());
    }

    #[test]
    fn every_send_reloads_the_env() {
        let (url, rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL);
        state.handle(ContractsAction::Send, Some(&paths));
        settle_send(&mut state);
        assert!(received(&rx).contains("x-token: one"));

        let (url, rx) = serve_once(OK);
        write_env(&paths.apic_dir, &url, Some("two"));
        state.handle(ContractsAction::Send, Some(&paths));
        settle_send(&mut state);

        assert!(received(&rx).contains("x-token: two"));
        assert!(paths.apic_dir.join(".gitignore").is_file());
    }

    #[test]
    fn undefined_names_are_reported_on_the_status_line() {
        let (url, _rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL);
        write_env(&paths.apic_dir, &url, None);

        let events = state.handle(ContractsAction::Send, Some(&paths));

        assert!(
            events
                .iter()
                .any(|e| matches!(e, ContractsEvent::Status(s) if s.contains("$token"))),
            "{events:?}"
        );
        settle_send(&mut state);
    }

    #[test]
    fn a_missing_env_shows_its_error_without_sending() {
        let (mut state, paths) = open_contract("http://127.0.0.1:1", CALL);
        state.send.env = Some("nope".to_string());

        state.handle(ContractsAction::Send, Some(&paths));

        assert!(!state.send.is_pending());
        assert!(state.send.show_live);
        assert!(matches!(&state.send.live, Some(Err(err)) if err.contains("nope")));
    }

    #[test]
    fn loading_a_contract_clears_the_live_reply_and_the_send() {
        let (url, _rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL);
        state.handle(ContractsAction::Send, Some(&paths));
        state.send.live = Some(Err("old".to_string()));

        let i = state.selected.expect("contract is selected");
        state.handle(ContractsAction::Load(i), Some(&paths));

        assert!(!state.send.is_pending());
        assert!(state.send.live.is_none());
    }

    #[test]
    fn set_live_switches_the_right_pane() {
        let (mut state, paths) = open_contract("http://127.0.0.1:1", CALL);
        state.handle(ContractsAction::SetLive(true), Some(&paths));
        assert!(state.send.show_live);
        state.handle(ContractsAction::SetLive(false), Some(&paths));
        assert!(!state.send.show_live);
    }

    /// A reply with `content_type` and `body`.
    fn reply(content_type: &str, body: Vec<u8>) -> Reply {
        Reply {
            status: 200,
            reason: "OK".to_string(),
            headers: vec![("Content-Type".to_string(), content_type.to_string())],
            body,
            elapsed: Duration::from_millis(5),
        }
    }

    #[test]
    fn a_reply_body_is_json_text_or_binary() {
        let json = LiveReply::from_reply(&reply("application/json; charset=utf-8", b"{}".to_vec()));
        assert_eq!(json.body, LiveBody::Json("{}".to_string()));
        let text = LiveReply::from_reply(&reply("text/plain", b"hi".to_vec()));
        assert_eq!(text.body, LiveBody::Text("hi".to_string()));
        let binary = LiveReply::from_reply(&reply("image/png", vec![0xff, 0xfe, 0x00]));
        assert_eq!(binary.body, LiveBody::Binary(3));
    }

    #[test]
    fn a_long_body_is_cut_at_the_display_limit() {
        let live = LiveReply::from_reply(&reply("text/plain", vec![b'a'; DISPLAY_LIMIT + 10]));
        assert!(live.truncated);
        assert_eq!(live.size, DISPLAY_LIMIT + 10);
        assert_eq!(live.body, LiveBody::Text("a".repeat(DISPLAY_LIMIT)));
    }

    /// `GET $base/users/{id}`.
    const CALL_WITH_ID: &str = r#"{ "name": "user", "method": "GET", "url": "$base/users/{id}",
        "headers": [], "responses": [] }"#;

    #[test]
    fn an_unfilled_path_param_asks_first_then_sends() {
        let (url, rx) = serve_once(OK);
        let (mut state, paths) = open_contract(&url, CALL_WITH_ID);

        state.handle(ContractsAction::Send, Some(&paths));

        assert!(!state.send.is_pending(), "no request starts");
        assert_eq!(
            state.send.prompt,
            Some(vec![("id".to_string(), String::new())])
        );
        state.handle(ContractsAction::SubmitParams, Some(&paths));
        assert!(state.send.prompt.is_some(), "a blank value keeps the popup");
        assert!(!state.send.is_pending());

        state.send.prompt = Some(vec![("id".to_string(), "42".to_string())]);
        state.handle(ContractsAction::SubmitParams, Some(&paths));

        assert!(state.send.prompt.is_none());
        settle_send(&mut state);
        assert!(received(&rx).starts_with("get /users/42 "));

        state.handle(ContractsAction::Send, Some(&paths));
        assert_eq!(
            state.send.prompt,
            Some(vec![("id".to_string(), "42".to_string())]),
            "the value is prefilled next time"
        );
    }

    #[test]
    fn choose_env_emits_env_chosen_and_loads_the_names() {
        let (mut state, paths) = open_contract("http://127.0.0.1:1", CALL);

        let events = state.handle(
            ContractsAction::ChooseEnv(Some("dev".to_string())),
            Some(&paths),
        );

        assert!(events.contains(&ContractsEvent::EnvChosen(Some("dev".to_string()))));
        assert!(
            state.send.env_vars.contains_key("base") && state.send.env_vars.contains_key("token")
        );

        let events = state.handle(ContractsAction::ChooseEnv(None), Some(&paths));

        assert!(events.contains(&ContractsEvent::EnvChosen(None)));
        assert_eq!(state.send.env, None);
        assert!(state.send.env_vars.is_empty());
    }
}
