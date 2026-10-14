//! Typed, bounded read-only updates. Raw service replies do not become fresh state merely
//! because a call was admitted. Independent domains can fail without inventing replacement values.
use crate::{
    model::{valid_app_id, Application, Context, Setting},
    shell::{Input, Snapshot},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading<T> {
    Value(T),
    Failed(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueReading {
    pub apps: Vec<Application>,
    pub launch_ids: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PictureReading {
    pub context: Context,
    pub settings: Vec<Setting>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub observed_ms: u32,
    pub simulated: bool,
    pub catalogue: Reading<CatalogueReading>,
    pub inputs: Reading<Vec<Input>>,
    pub picture: Reading<PictureReading>,
    pub sound: Reading<Vec<(String, String)>>,
}
fn checked<'a>(replies: &'a Value, name: &str) -> Result<&'a Value, String> {
    let value = replies
        .get(name)
        .ok_or_else(|| format!("{name}: missing reply"))?;
    if value.get("returnValue").and_then(Value::as_bool) != Some(true) {
        return Err(format!("{name}: unavailable"));
    }
    Ok(value)
}
fn text(value: &Value, key: &str, limit: usize) -> Result<String, String> {
    let text = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {key}"))?;
    if text.is_empty() || text.len() > limit || text.chars().any(char::is_control) {
        return Err(format!("invalid {key}"));
    }
    Ok(text.to_owned())
}
fn array<'a>(value: &'a Value, key: &str, limit: usize) -> Result<&'a Vec<Value>, String> {
    let list = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing {key}"))?;
    if list.len() > limit {
        return Err(format!("too many {key}"));
    }
    Ok(list)
}
fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(v) if v.len() <= 128 && !v.chars().any(char::is_control) => Some(v.clone()),
        Value::Number(v) => Some(v.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        _ => None,
    }
}
fn reading<T>(result: Result<T, String>) -> Reading<T> {
    match result {
        Ok(value) => Reading::Value(value),
        Err(error) => Reading::Failed(error),
    }
}
pub fn parse(raw: &Value, own_id: &str, observed_ms: u32, simulated: bool) -> Update {
    let replies = &raw["replies"];
    let catalogue = reading((|| {
        let apps = checked(replies, "apps")?;
        let points = checked(replies, "launch_points")?;
        let launch_ids = array(points, "launchPoints", 512)?
            .iter()
            .filter_map(|p| p.get("id").and_then(Value::as_str))
            .filter(|id| valid_app_id(id) && *id != own_id)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let mut unique = BTreeMap::new();
        for app in array(apps, "apps", 512)? {
            let Some(id) = app.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(kind) = app.get("type").and_then(Value::as_str) else {
                continue;
            };
            if !valid_app_id(id)
                || id == own_id
                || !launch_ids.contains(id)
                || app.get("visible").and_then(Value::as_bool) != Some(true)
                || !matches!(kind, "native" | "native_builtin" | "web")
            {
                continue;
            }
            let Ok(title) = text(app, "title", 512) else {
                continue;
            };
            unique.entry(id.to_owned()).or_insert(Application {
                id: id.to_owned(),
                title,
                kind: kind.to_owned(),
                visible: true,
                icon: None,
            });
        }
        Ok(CatalogueReading {
            apps: unique.into_values().collect(),
            launch_ids,
        })
    })());
    let inputs = reading((|| {
        let data = checked(replies, "inputs")?;
        let mut ports = BTreeMap::new();
        for input in array(data, "devices", 16)? {
            let id = text(input, "id", 16)?;
            if !matches!(id.as_str(), "HDMI_1" | "HDMI_2" | "HDMI_3" | "HDMI_4") {
                continue;
            }
            let flag = |key| {
                input
                    .get(key)
                    .and_then(Value::as_bool)
                    .ok_or_else(|| format!("missing {key}"))
            };
            let row = Input {
                id: id.clone(),
                label: text(input, "label", 128)?,
                plugged: flag("hdmiPlugIn")?,
                signal: flag("hdmiSignalExist")?,
                active: flag("activate")?,
            };
            if ports.insert(id, row).is_some() {
                return Err("Duplicate HDMI identity".into());
            }
        }
        if ports.len() != 4 {
            return Err("Incomplete HDMI status".into());
        }
        Ok(ports.into_values().collect())
    })());
    let picture = reading((|| {
        let data = checked(replies, "picture")?;
        let dimension = &data["dimension"];
        let context = Context {
            input: text(dimension, "input", 64)?,
            dynamic_range: text(dimension, "dynamicRange", 64)?,
            picture_mode: text(dimension, "pictureMode", 64)?,
        };
        let values = data
            .get("settings")
            .and_then(Value::as_object)
            .ok_or("Missing picture values")?;
        if values.get("pictureMode").and_then(Value::as_str) != Some(context.picture_mode.as_str())
        {
            return Err("Picture mode/context mismatch".into());
        }
        let mut settings = Vec::new();
        for (i, (id, label)) in [
            ("pictureMode", "Picture mode"),
            ("backlight", "OLED pixel brightness"),
            ("contrast", "Contrast"),
            ("brightness", "Black level"),
        ]
        .into_iter()
        .enumerate()
        {
            let value = values
                .get(id)
                .and_then(scalar)
                .ok_or_else(|| format!("Missing {id}"))?;
            if id != "pictureMode" && values[id].as_i64().is_none() {
                return Err(format!("Invalid numeric {id}"));
            }
            settings.push(Setting {
                id: id.into(),
                focus_key: i as u32 + 1,
                label: label.into(),
                value,
                context: context.clone(),
                protected: false,
                available: true,
            });
        }
        Ok(PictureReading { context, settings })
    })());
    let sound = reading((|| {
        let data = checked(replies, "audio")?;
        let values = data
            .get("volumeStatus")
            .and_then(Value::as_object)
            .ok_or("Missing audio status")?;
        ["soundOutput", "volume", "muteStatus"]
            .into_iter()
            .map(|id| {
                values
                    .get(id)
                    .and_then(scalar)
                    .map(|v| (id.to_owned(), v))
                    .ok_or_else(|| format!("Missing {id}"))
            })
            .collect::<Result<Vec<_>, _>>()
    })());
    Update {
        observed_ms,
        simulated,
        catalogue,
        inputs,
        picture,
        sound,
    }
}
#[derive(Clone, Debug, Default)]
pub struct Freshness {
    pub catalogue: Option<u32>,
    pub inputs: Option<u32>,
    pub picture: Option<u32>,
    pub sound: Option<u32>,
    pub simulated: bool,
}
impl Freshness {
    pub fn current(stamp: Option<u32>, now: u32) -> bool {
        stamp.is_some_and(|ms| now.wrapping_sub(ms) <= 15_000)
    }
}
pub struct Worker {
    latest: Arc<Mutex<Option<Update>>>,
    stop: mpsc::Sender<()>,
    thread: thread::JoinHandle<()>,
}
impl Worker {
    pub fn fixture(
        values: Vec<Value>,
        own_id: String,
        origin: Instant,
        period: Duration,
    ) -> std::io::Result<Self> {
        if values.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Empty live fixture",
            ));
        }
        let mut index = 0;
        Self::start(
            move || {
                move || {
                    let data = values[index.min(values.len() - 1)].clone();
                    index = index.saturating_add(1);
                    parse(&data, &own_id, origin.elapsed().as_millis() as u32, true)
                }
            },
            period,
        )
    }
    fn start<Init, F>(make_reader: Init, period: Duration) -> std::io::Result<Self>
    where
        Init: FnOnce() -> F + Send + 'static,
        F: FnMut() -> Update + 'static,
    {
        let latest = Arc::new(Mutex::new(None));
        let output = latest.clone();
        let (stop, receive) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("home-live-read".into())
            .spawn(move || {
                let mut read = make_reader();
                loop {
                    *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(read());
                    match receive.recv_timeout(period) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => break,
                    }
                }
            })?;
        Ok(Self {
            latest,
            stop,
            thread,
        })
    }
    #[cfg(target_arch = "arm")]
    pub fn native(own_id: String, origin: Instant) -> std::io::Result<Self> {
        // Registration lives on this worker. Its private GLib context never crosses threads.
        Self::start(
            move || {
                let mut reader = None;
                move || {
                    use plx_platform::webos::ls2;
                    use serde_json::json;
                    if reader.is_none() {
                        reader = ls2::register().ok();
                    }
                    let raw = reader
                        .as_ref()
                        .map_or_else(|| json!({}), |bus| crate::native_probe::read(bus, &own_id));
                    parse(&raw, &own_id, origin.elapsed().as_millis() as u32, false)
                }
            },
            Duration::from_secs(2),
        )
    }
    pub fn poll(&self) -> Option<Update> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
    pub fn finish(self) {
        let Self {
            latest: _,
            stop,
            thread,
        } = self;
        let _ = stop.send(());
        let _ = thread.join();
    }
}
/// Keep the last image/value for inspection when unavailable, but withdraw its freshness.
pub fn merge(snapshot: &mut Snapshot, update: &Update, fresh: &mut Freshness) {
    fresh.simulated = update.simulated;
    match &update.catalogue {
        Reading::Value(value) => {
            let icons = snapshot
                .applications
                .iter()
                .filter_map(|a| a.icon.as_ref().map(|icon| (a.id.clone(), icon.clone())))
                .collect::<BTreeMap<_, _>>();
            snapshot.applications = value.apps.clone();
            for app in &mut snapshot.applications {
                app.icon = icons.get(&app.id).cloned();
            }
            snapshot.launch_ids = value.launch_ids.clone();
            fresh.catalogue = Some(update.observed_ms);
        }
        Reading::Failed(_) => fresh.catalogue = None,
    }
    match &update.inputs {
        Reading::Value(value) => {
            snapshot.inputs = value.clone();
            fresh.inputs = Some(update.observed_ms);
        }
        Reading::Failed(_) => fresh.inputs = None,
    }
    match &update.picture {
        Reading::Value(value) => {
            snapshot.context = value.context.clone();
            snapshot
                .settings
                .insert("picture".into(), value.settings.clone());
            fresh.picture = Some(update.observed_ms);
        }
        Reading::Failed(_) => {
            fresh.picture = None;
        }
    }
    match &update.sound {
        Reading::Value(value) => {
            let rows = value
                .iter()
                .enumerate()
                .map(|(i, (id, value))| Setting {
                    id: id.clone(),
                    focus_key: 20 + i as u32,
                    label: match id.as_str() {
                        "soundOutput" => "Sound output",
                        "volume" => "Volume",
                        _ => "Muted",
                    }
                    .into(),
                    value: value.clone(),
                    context: snapshot.context.clone(),
                    protected: false,
                    available: true,
                })
                .collect();
            snapshot.settings.insert("sound".into(), rows);
            fresh.sound = Some(update.observed_ms);
        }
        Reading::Failed(_) => fresh.sound = None,
    }
    expire(snapshot, fresh, update.observed_ms);
}
pub fn expire(snapshot: &mut Snapshot, fresh: &Freshness, now: u32) {
    for (section, stamp) in [("picture", fresh.picture), ("sound", fresh.sound)] {
        if !Freshness::current(stamp, now) {
            if let Some(rows) = snapshot.settings.get_mut(section) {
                for row in rows {
                    row.available = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn raw() -> Value {
        let devices=(1..=4).map(|port|json!({"id":format!("HDMI_{port}"),"label":format!("HDMI {port}"),"hdmiPlugIn":false,"hdmiSignalExist":false,"activate":false})).collect::<Vec<_>>();
        json!({"replies":{
            "apps":{"returnValue":true,"apps":[{"id":"com.example.a","title":"Alpha","type":"native","visible":true},{"id":"com.example.b","title":"Bravo","type":"web","visible":true},{"id":"com.example.self","title":"Dashboard","type":"native","visible":true},{"id":"com.example.stub","title":"Store stub","type":"stub","visible":true}]},
            "launch_points":{"returnValue":true,"launchPoints":[{"id":"com.example.a"},{"id":"com.example.b"},{"id":"com.example.self"},{"id":"com.example.stub"}]},
            "inputs":{"returnValue":true,"devices":devices},
            "picture":{"returnValue":true,"dimension":{"input":"default","dynamicRange":"sdr","pictureMode":"game"},"settings":{"pictureMode":"game","backlight":90,"contrast":85,"brightness":50}},
            "audio":{"returnValue":false,"errorText":"Unavailable"}}})
    }
    #[test]
    fn provider_requires_success_and_shape_and_never_promotes_own_app_or_stubs() {
        let mut data = raw();
        let update = parse(&data, "com.example.self", 10, false);
        let Reading::Value(catalogue) = update.catalogue else {
            panic!("Catalogue should be valid")
        };
        assert_eq!(catalogue.apps.len(), 2);
        assert!(matches!(update.sound, Reading::Failed(_)));
        data["replies"]["apps"]["returnValue"] = json!(false);
        assert!(matches!(
            parse(&data, "com.example.self", 11, false).catalogue,
            Reading::Failed(_)
        ));
        data["replies"]["apps"]["returnValue"] = json!(true);
        data["replies"]["apps"]["apps"] = json!({});
        assert!(matches!(
            parse(&data, "com.example.self", 12, false).catalogue,
            Reading::Failed(_)
        ));
    }
    #[test]
    fn picture_context_mismatch_and_missing_input_flags_are_unavailable() {
        let mut data = raw();
        data["replies"]["picture"]["dimension"]["pictureMode"] = json!("cinema");
        assert!(matches!(
            parse(&data, "com.example.self", 10, false).picture,
            Reading::Failed(_)
        ));
        data["replies"]["inputs"]["devices"][0]
            .as_object_mut()
            .unwrap()
            .remove("hdmiPlugIn");
        assert!(matches!(
            parse(&data, "com.example.self", 10, false).inputs,
            Reading::Failed(_)
        ));
    }
    #[test]
    fn port_identity_is_normalized_even_when_service_order_changes() {
        let mut data = raw();
        data["replies"]["inputs"]["devices"]
            .as_array_mut()
            .unwrap()
            .reverse();
        let Reading::Value(ports) = parse(&data, "com.example.self", 10, false).inputs else {
            panic!("Inputs should be valid")
        };
        assert_eq!(
            ports.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["HDMI_1", "HDMI_2", "HDMI_3", "HDMI_4"]
        );
    }
    #[test]
    fn freshness_expires_and_does_not_accept_future_or_unrelated_clock_stamps() {
        assert!(Freshness::current(Some(100), 15000));
        assert!(!Freshness::current(Some(100), 15101));
        assert!(!Freshness::current(Some(101), 100));
        assert!(!Freshness::current(None, 100));
    }
}
