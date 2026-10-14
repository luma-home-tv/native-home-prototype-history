use std::collections::{BTreeMap, BTreeSet};

use plx_ui::form::{Form, FormSection, RowKey, RowKind};
use plx_ui::table::Row;
use serde::{Deserialize, Serialize};
use crate::strings::Message;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub icon: Option<String>,
}

fn yes() -> bool { true }

/// A launch target comes from the platform's launch points, never an arbitrary user string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launchable {
    pub id: String,
    pub title: String,
    pub key: RowKey,
    pub icon: Option<String>,
}

#[derive(Default)]
pub struct Catalogue {
    keys: BTreeMap<String, RowKey>,
    next_key: u32,
    pub applications: Vec<Launchable>,
}

fn valid_app_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 180 && id.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

impl Catalogue {
    /// Preserve keys by identity across reorders and refreshes. Store placeholders and hidden
    /// services are never promoted to actionable applications by a misleading title.
    pub fn refresh(&mut self, applications: &[Application], launch_ids: &BTreeSet<String>) {
        let mut unique = BTreeMap::new();
        for app in applications {
            if !app.visible || !matches!(app.kind.as_str(), "native" | "native_builtin" | "web")
                || !valid_app_id(&app.id) || !launch_ids.contains(&app.id) {
                continue;
            }
            unique.entry(app.id.clone()).or_insert(app);
        }
        self.applications = unique.into_values().map(|app| {
            let key = *self.keys.entry(app.id.clone()).or_insert_with(|| {
                self.next_key += 1;
                RowKey(self.next_key)
            });
            Launchable { id: app.id.clone(), title: app.title.clone(), key, icon: app.icon.clone() }
        }).collect();
        self.applications.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Page { Home, Applications, Inputs, Controls, Picture, Sound, Network, Accessibility, General, PanelCare }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Inspect(String),
    RequestApplication(String),
    RequestInput(String),
    OpenLgSettings,
}

/// No setting-write action exists in this pilot. A future broker cannot acquire write authority
/// merely because a settings descriptor was returned successfully.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setting {
    pub id: String,
    /// A registry-assigned identity key, independent of row order or current value.
    pub focus_key: u32,
    pub label: String,
    pub value: String,
    pub context: Context,
    pub protected: bool,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub input: String,
    pub dynamic_range: String,
    pub picture_mode: String,
}

impl Setting {
    pub fn still_current(&self, current: &Context) -> bool { &self.context == current }
}

pub fn application_form(catalogue: &Catalogue) -> Form<String, Action, Page> {
    let mut section = FormSection::new(Message::Applications.text());
    for app in &catalogue.applications {
        section = section.item_keyed(app.id.clone(), app.key, RowKind::Button,
            Action::RequestApplication(app.id.clone()), Row::new(&app.title).server_label());
    }
    Form::new().section(section)
}

pub fn controls_form(settings: &[Setting], current: &Context) -> Form<String, Action, Page> {
    let mut section = FormSection::new(Message::CurrentSettings.text());
    for setting in settings {
        let unavailable = !setting.available || !setting.still_current(current);
        let detail = if setting.protected { Message::ManagedByLg.text() }
            else if unavailable { Message::UnavailableContext.text() }
            else { "" };
        let value = if unavailable { "Unavailable" } else { &setting.value };
        section = section.item_keyed(setting.id.clone(), RowKey(setting.focus_key), RowKind::Button,
            Action::Inspect(setting.id.clone()), Row::new(&setting.label).detail(detail).value(value).server_value())
            .disabled(unavailable || setting.protected);
    }
    let lg = FormSection::new(Message::MoreSettings.text()).item_keyed("lg-settings".to_owned(), RowKey(1000),
        RowKind::Button, Action::OpenLgSettings, Row::new(Message::OpenLgSettings.text()));
    Form::new().section(section).section(lg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plx_ui::form::{Activation, FormTable};

    fn app(id: &str, kind: &str, visible: bool) -> Application {
        Application { id: id.into(), title: id.into(), kind: kind.into(), visible, icon: None }
    }
    fn context() -> Context {
        Context { input: "HDMI_1".into(), dynamic_range: "sdr".into(), picture_mode: "cinema".into() }
    }
    fn setting(id: &str, protected: bool) -> Setting {
        Setting { id: id.into(), focus_key: if protected { 1 } else { 2 }, label: id.into(), value: "50".into(), context: context(), protected, available: true }
    }

    #[test]
    fn drawer_excludes_placeholders_hidden_services_and_unregistered_targets() {
        let apps = [app("com.example.video", "native", true), app("com.example.store", "stub", true),
            app("com.example.service", "native", false), app("com.example.unregistered", "web", true),
            app("bad;command", "native", true)];
        let ids = apps.iter().map(|a| a.id.clone()).collect();
        let mut catalogue = Catalogue::default(); catalogue.refresh(&apps, &ids);
        // Remove an application from the authoritative launch-point list.
        let ids = BTreeSet::from(["com.example.video".into(), "com.example.store".into(), "com.example.service".into(), "bad;command".into()]);
        catalogue.refresh(&apps, &ids);
        assert_eq!(catalogue.applications.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["com.example.video"]);
    }

    #[test]
    fn drawer_focus_identity_survives_rename_reorder_and_duplicates() {
        let first = [app("com.example.a", "native", true), app("com.example.b", "web", true)];
        let ids = first.iter().map(|a| a.id.clone()).collect();
        let mut catalogue = Catalogue::default(); catalogue.refresh(&first, &ids);
        let key = catalogue.applications[0].key;
        let mut renamed = first[0].clone(); renamed.title = "Zulu".into();
        catalogue.refresh(&[first[1].clone(), renamed.clone(), renamed], &ids);
        assert_eq!(catalogue.applications.len(), 2);
        assert_eq!(catalogue.applications.iter().find(|a| a.id == "com.example.a").unwrap().key, key);
    }

    #[test]
    fn reused_native_form_cannot_activate_protected_or_stale_controls() {
        let settings = [setting("panel-protection", true), setting("brightness", false)];
        let mut table = FormTable::new(2000);
        table.set(controls_form(&settings, &context()), None);
        assert_eq!(table.activate(0), None);
        assert_eq!(table.activate(1), Some(Activation::Action(Action::Inspect("brightness".into()))));
        let mut hdr = context(); hdr.dynamic_range = "hdr".into();
        table.set(controls_form(&settings, &hdr), None);
        assert_eq!(table.activate(1), None);
        assert_eq!(table.table.sections[0].rows[1].value.as_deref(), Some("Unavailable"));
        assert_eq!(table.activate(2), Some(Activation::Action(Action::OpenLgSettings)));
    }

    #[test]
    fn application_activation_is_bound_to_registered_identity() {
        let apps = [app("com.example.video", "native", true)];
        let ids = BTreeSet::from([apps[0].id.clone()]);
        let mut catalogue = Catalogue::default(); catalogue.refresh(&apps, &ids);
        let mut table = FormTable::new(2000); table.set(application_form(&catalogue), None);
        assert_eq!(table.activate(0), Some(Activation::Action(Action::RequestApplication(apps[0].id.clone()))));
        catalogue.refresh(&[], &BTreeSet::new()); table.set(application_form(&catalogue), None);
        assert_eq!(table.activate(0), None);
    }
}
