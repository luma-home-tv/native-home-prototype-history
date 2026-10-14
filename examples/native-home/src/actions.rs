//! Restricted normal-app launch adapter. Every operation uses fixed services/parameters,
//! current platform metadata and readback. It has no setting-write or generic service action.
use crate::model::{valid_app_id, Action};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::mpsc, thread};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Request {
    Application { id: String },
    Input { id: String },
    LgSettings {},
}
impl Request {
    pub fn from_action(action: Action) -> Option<Self> {
        match action {
            Action::RequestApplication(id) => Some(Self::Application { id }),
            Action::RequestInput(id) => Some(Self::Input { id }),
            Action::OpenLgSettings => Some(Self::LgSettings {}),
            Action::Inspect(_) => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub apps: bool,
    pub inputs: bool,
    pub settings: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Receipt {
    pub request: Request,
    pub target: Option<String>,
    pub admitted: bool,
    pub verified: bool,
    pub detail: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Apps,
    LaunchPoints,
    Inputs,
    Foreground,
    Windows,
    Launch,
}
pub trait Platform {
    fn call(&mut self, op: Op, payload: Value) -> Result<Value, String>;
    fn wait(&mut self);
}
fn accepted(value: &Value) -> bool {
    value.get("returnValue").and_then(Value::as_bool) == Some(true)
}
fn get<P: Platform>(p: &mut P, op: Op) -> Result<Value, String> {
    let data = p.call(op, json!({}))?;
    if !accepted(&data) {
        return Err("Platform read refused".into());
    }
    Ok(data)
}
fn target(
    request: &Request,
    own_id: &str,
    apps: &Value,
    points: Option<&Value>,
    inputs: Option<&Value>,
) -> Result<String, String> {
    let rows = apps
        .get("apps")
        .and_then(Value::as_array)
        .ok_or("Missing application metadata")?;
    if rows.len() > 512 {
        return Err("Application metadata exceeds limit".into());
    }
    let (id, visible) = match request {
        Request::Application { id } => {
            if !valid_app_id(id) || id == own_id {
                return Err("Invalid or self application target".into());
            }
            (id.clone(), true)
        }
        Request::LgSettings {} => ("com.palm.app.settings".to_owned(), false),
        Request::Input { id } => {
            let app = match id.as_str() {
                "HDMI_1" => "com.webos.app.hdmi1",
                "HDMI_2" => "com.webos.app.hdmi2",
                "HDMI_3" => "com.webos.app.hdmi3",
                "HDMI_4" => "com.webos.app.hdmi4",
                _ => return Err("Unknown input identity".into()),
            };
            let devices = inputs
                .and_then(|v| v.get("devices"))
                .and_then(Value::as_array)
                .ok_or("Missing input metadata")?;
            let matches = devices
                .iter()
                .filter(|d| d.get("id").and_then(Value::as_str) == Some(id))
                .collect::<Vec<_>>();
            if matches.len() != 1 || matches[0].get("appId").and_then(Value::as_str) != Some(app) {
                return Err("Input target mismatch".into());
            }
            (app.to_owned(), false)
        }
    };
    let matches = rows
        .iter()
        .filter(|a| a.get("id").and_then(Value::as_str) == Some(id.as_str()))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("Unregistered or ambiguous application".into());
    }
    let app = matches[0];
    let kind = app.get("type").and_then(Value::as_str).unwrap_or("");
    if !matches!(kind, "native" | "native_builtin" | "web") {
        return Err("Application is not launchable".into());
    }
    if visible {
        if app.get("visible").and_then(Value::as_bool) != Some(true) {
            return Err("Hidden application target".into());
        }
        let points = points
            .and_then(|v| v.get("launchPoints"))
            .and_then(Value::as_array)
            .ok_or("Missing launch points")?;
        if points.len() > 512
            || !points
                .iter()
                .any(|p| p.get("id").and_then(Value::as_str) == Some(id.as_str()))
        {
            return Err("No current launch point".into());
        }
    }
    Ok(id)
}
pub fn execute<P: Platform>(
    platform: &mut P,
    own_id: &str,
    request: Request,
    caps: Capabilities,
) -> Receipt {
    let mut receipt = Receipt {
        request: request.clone(),
        target: None,
        admitted: false,
        verified: false,
        detail: String::new(),
    };
    let result = (|| {
        let allowed = match request {
            Request::Application { .. } => caps.apps,
            Request::Input { .. } => caps.inputs,
            Request::LgSettings {} => caps.settings,
        };
        if !allowed {
            return Err("Operation has not been validated".into());
        }
        let before = get(platform, Op::Foreground)?;
        if before.get("appId").and_then(Value::as_str) != Some(own_id) {
            return Err("Dashboard is not foreground".into());
        }
        let apps = get(platform, Op::Apps)?;
        let points = if matches!(request, Request::Application { .. }) {
            Some(get(platform, Op::LaunchPoints)?)
        } else {
            None
        };
        let inputs = if matches!(request, Request::Input { .. }) {
            Some(get(platform, Op::Inputs)?)
        } else {
            None
        };
        let id = target(&request, own_id, &apps, points.as_ref(), inputs.as_ref())?;
        receipt.target = Some(id.clone());
        // Recheck ownership after the reads; an old queued gesture cannot switch another app.
        let current = get(platform, Op::Foreground)?;
        if current.get("appId").and_then(Value::as_str) != Some(own_id) {
            return Err("Foreground changed before launch".into());
        }
        let reply = platform.call(Op::Launch, json!({"id":id}))?;
        if !accepted(&reply) {
            return Err("Launch refused".into());
        }
        receipt.admitted = true;
        for _ in 0..8 {
            let current = get(platform, Op::Foreground)?;
            let overlay = if matches!(request, Request::LgSettings {}) {
                let windows = get(platform, Op::Windows)?;
                windows
                    .get("windows")
                    .and_then(Value::as_array)
                    .is_some_and(|rows| {
                        rows.iter().any(|r| {
                            r.get("appId").and_then(Value::as_str) == receipt.target.as_deref()
                                && r.get("windowType").and_then(Value::as_str)
                                    == Some("_WEBOS_WINDOW_TYPE_OVERLAY")
                        })
                    })
            } else {
                false
            };
            if overlay || current.get("appId").and_then(Value::as_str) == receipt.target.as_deref()
            {
                if let Request::Input { id } = &request {
                    let inputs = get(platform, Op::Inputs)?;
                    let active =
                        inputs
                            .get("devices")
                            .and_then(Value::as_array)
                            .is_some_and(|rows| {
                                rows.iter().any(|r| {
                                    r.get("id").and_then(Value::as_str) == Some(id)
                                        && r.get("activate").and_then(Value::as_bool) == Some(true)
                                })
                            });
                    if !active {
                        platform.wait();
                        continue;
                    }
                }
                receipt.verified = true;
                return Ok(());
            }
            platform.wait();
        }
        Err("Launch admitted; foreground/input readback did not confirm it".into())
    })();
    receipt.detail = match result {
        Ok(()) => "Target readback confirmed".into(),
        Err(error) => error,
    };
    receipt
}
#[cfg(target_arch = "arm")]
struct NativePlatform {
    bus: plx_platform::webos::ls2::Registration,
}
#[cfg(target_arch = "arm")]
impl Platform for NativePlatform {
    fn call(&mut self, op: Op, payload: Value) -> Result<Value, String> {
        use plx_platform::webos::ls2;
        let uri = match op {
            Op::Apps => "luna://com.webos.applicationManager/listApps",
            Op::LaunchPoints => "luna://com.webos.applicationManager/listLaunchPoints",
            Op::Inputs => "luna://com.webos.service.eim/getAllInputStatus",
            Op::Foreground => "luna://com.webos.applicationManager/getForegroundAppInfo",
            Op::Windows => "luna://com.webos.surfacemanager/getForegroundWindowInfo",
            Op::Launch => "luna://com.webos.applicationManager/launch",
        };
        let raw = self
            .bus
            .call(
                uri,
                &payload.to_string(),
                std::time::Duration::from_millis(700),
            )
            .map_err(|error| match error {
                ls2::Fail::Timeout => "Platform call timed out".into(),
                ls2::Fail::Setup { stage, detail, .. } => {
                    format!("Platform call failed at {stage}: {detail}")
                }
            })?;
        if raw.len() > 2 * 1024 * 1024 {
            return Err("Platform reply too large".into());
        }
        serde_json::from_str(&raw).map_err(|_| "Malformed platform reply".into())
    }
    fn wait(&mut self) {
        thread::sleep(std::time::Duration::from_millis(120));
    }
}
pub struct Worker {
    requests: mpsc::SyncSender<Request>,
    replies: mpsc::Receiver<Receipt>,
    thread: thread::JoinHandle<()>,
}
impl Worker {
    #[cfg(target_arch = "arm")]
    pub fn native(own_id: String, caps: Capabilities) -> std::io::Result<Self> {
        let (requests, receive) = mpsc::sync_channel::<Request>(1);
        let (send, replies) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("home-launch".into())
            .spawn(move || {
                let mut platform = plx_platform::webos::ls2::register()
                    .ok()
                    .map(|bus| NativePlatform { bus });
                while let Ok(request) = receive.recv() {
                    let receipt = match &mut platform {
                        Some(p) => execute(p, &own_id, request, caps),
                        None => Receipt {
                            request,
                            target: None,
                            admitted: false,
                            verified: false,
                            detail: "Normal app registration unavailable".into(),
                        },
                    };
                    if send.send(receipt).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            replies,
            thread,
        })
    }
    pub fn queue(&self, request: Request) -> Result<(), String> {
        self.requests
            .try_send(request)
            .map_err(|_| "Another launch is pending".into())
    }
    pub fn poll(&self) -> Vec<Receipt> {
        self.replies.try_iter().collect()
    }
    pub fn finish(self) -> Vec<Receipt> {
        let Self {
            requests,
            replies,
            thread,
        } = self;
        drop(requests);
        let _ = thread.join();
        replies.try_iter().collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        foreground: String,
        after: String,
        rows: Value,
        points: Value,
        inputs: Value,
        launches: Vec<Value>,
        accepted: bool,
        window_after: Option<String>,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                foreground: "com.example.home".into(),
                after: "com.example.target".into(),
                rows: json!({"returnValue":true,"apps":[{"id":"com.example.target","type":"native","visible":true},{"id":"com.palm.app.settings","type":"web","visible":false},{"id":"com.webos.app.hdmi1","type":"native","visible":false}]}),
                points: json!({"returnValue":true,"launchPoints":[{"id":"com.example.target"}]}),
                inputs: json!({"returnValue":true,"devices":[{"id":"HDMI_1","appId":"com.webos.app.hdmi1","activate":true}]}),
                launches: Vec::new(),
                accepted: true,
                window_after: None,
            }
        }
    }
    impl Platform for Fake {
        fn call(&mut self, op: Op, payload: Value) -> Result<Value, String> {
            Ok(match op {
                Op::Apps => self.rows.clone(),
                Op::LaunchPoints => self.points.clone(),
                Op::Inputs => self.inputs.clone(),
                Op::Windows => {
                    json!({"returnValue":true,"windows":if self.launches.is_empty(){json!([])}else{json!([{"appId":self.window_after.as_ref().unwrap_or(&self.after),"windowType":"_WEBOS_WINDOW_TYPE_OVERLAY"}])}})
                }
                Op::Foreground => {
                    json!({"returnValue":true,"appId":if self.launches.is_empty(){&self.foreground}else{&self.after}})
                }
                Op::Launch => {
                    self.launches.push(payload);
                    json!({"returnValue":self.accepted})
                }
            })
        }
        fn wait(&mut self) {}
    }
    #[test]
    fn registered_application_launch_has_exact_payload_and_readback() {
        let mut p = Fake::new();
        let r = execute(
            &mut p,
            "com.example.home",
            Request::Application {
                id: "com.example.target".into(),
            },
            Capabilities {
                apps: true,
                ..Default::default()
            },
        );
        assert!(r.admitted && r.verified);
        assert_eq!(p.launches, vec![json!({"id":"com.example.target"})]);
    }
    #[test]
    fn unknown_self_hidden_stub_and_unvalidated_requests_never_launch() {
        for id in [
            "bad;command",
            "com.example.gone",
            "com.example.home",
            "com.palm.app.settings",
        ] {
            let mut p = Fake::new();
            let r = execute(
                &mut p,
                "com.example.home",
                Request::Application { id: id.into() },
                Capabilities {
                    apps: true,
                    ..Default::default()
                },
            );
            assert!(!r.admitted);
            assert!(p.launches.is_empty());
        }
        let mut p = Fake::new();
        p.rows["apps"][0]["type"] = json!("stub");
        assert!(
            !execute(
                &mut p,
                "com.example.home",
                Request::Application {
                    id: "com.example.target".into()
                },
                Capabilities {
                    apps: true,
                    ..Default::default()
                }
            )
            .admitted
        );
        let mut p = Fake::new();
        assert!(
            !execute(
                &mut p,
                "com.example.home",
                Request::LgSettings {},
                Capabilities::default()
            )
            .admitted
        );
        assert!(p.launches.is_empty());
    }
    #[test]
    fn admission_is_not_execution_and_unrelated_foreground_blocks_requests() {
        let mut p = Fake::new();
        p.after = "com.example.other".into();
        let r = execute(
            &mut p,
            "com.example.home",
            Request::Application {
                id: "com.example.target".into(),
            },
            Capabilities {
                apps: true,
                ..Default::default()
            },
        );
        assert!(r.admitted && !r.verified);
        let mut p = Fake::new();
        p.foreground = "com.example.other".into();
        let r = execute(
            &mut p,
            "com.example.home",
            Request::LgSettings {},
            Capabilities {
                settings: true,
                ..Default::default()
            },
        );
        assert!(!r.admitted && p.launches.is_empty());
    }
    #[test]
    fn hidden_lg_routes_are_explicit_and_input_metadata_must_match() {
        let mut p = Fake::new();
        p.after = "com.palm.app.settings".into();
        assert!(
            execute(
                &mut p,
                "com.example.home",
                Request::LgSettings {},
                Capabilities {
                    settings: true,
                    ..Default::default()
                }
            )
            .verified
        );
        let mut p = Fake::new();
        p.after = "com.webos.app.hdmi1".into();
        assert!(
            execute(
                &mut p,
                "com.example.home",
                Request::Input {
                    id: "HDMI_1".into()
                },
                Capabilities {
                    inputs: true,
                    ..Default::default()
                }
            )
            .verified
        );
        let mut p = Fake::new();
        p.inputs["devices"][0]["appId"] = json!("com.example.other");
        assert!(
            !execute(
                &mut p,
                "com.example.home",
                Request::Input {
                    id: "HDMI_1".into()
                },
                Capabilities {
                    inputs: true,
                    ..Default::default()
                }
            )
            .admitted
        );
        assert!(p.launches.is_empty());
    }
    #[test]
    fn settings_overlay_readback_is_distinct_from_the_underlying_foreground_card() {
        let mut p = Fake::new();
        p.after = "com.example.home".into();
        p.window_after = Some("com.palm.app.settings".into());
        let r = execute(
            &mut p,
            "com.example.home",
            Request::LgSettings {},
            Capabilities {
                settings: true,
                ..Default::default()
            },
        );
        assert!(r.admitted && r.verified);
        let mut p = Fake::new();
        p.after = "com.example.home".into();
        p.window_after = Some("com.example.other".into());
        let r = execute(
            &mut p,
            "com.example.home",
            Request::LgSettings {},
            Capabilities {
                settings: true,
                ..Default::default()
            },
        );
        assert!(r.admitted && !r.verified);
    }
    #[test]
    fn wire_requests_cannot_add_uri_command_or_launch_parameters() {
        assert!(serde_json::from_value::<Request>(
            json!({"kind":"LgSettings","uri":"luna://bad","command":"bad"})
        )
        .is_err());
        assert!(serde_json::from_value::<Request>(
            json!({"kind":"Application","id":"com.example.target","params":{"url":"bad"}})
        )
        .is_err());
    }
}
