//! The interactive application shell. Focus, hit resolution, press ownership and card motion
//! come from plx_ui. Only dashboard preferences are writable in this slice; TV actions are
//! typed requests whose executor is a separate, not-yet-enabled boundary.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;

use plx_machine::machine::{
    Canon, Chrome, Cx, Effects, EntryId, FocusKey, GroupId, Host, InputOwner, LogicalState,
    MachineId, PressArm, PressFrom, PressRead, ScreenArg, ScreenId, Tick,
};
use plx_machine::present::Present;
use plx_ui::cards::{CardSource, Grid, GridSpec, RowStyle, Tile, TileLabel};
use plx_ui::form::{Form, FormSection, FormTable, RowKey, RowKind};
use plx_ui::input::{InputMachine, PressEvent};
use plx_ui::label::{HAlign, Label};
use plx_ui::screen::{
    Activate, At, AxisMask, By, Dir, DrawFrame, EdgeRule, ElemKind, FocusTarget, Focusable,
    GroupKind, GroupSpec, Hover, Placed, ScreenEvent, Seat, Step, Stop,
};
use plx_ui::table::Row;
use plx_ui::widgets::{Art, LegacyMeasure};
use plx_ui::{theme, Painter, Rect};
use serde::{Deserialize, Serialize};

use crate::model::{controls_form, Action, Application, Catalogue, Context, Page, Setting};

const RAIL_BASE: u32 = 10_000;
const TABLE_BASE: u32 = 20_000;
const RAIL: Rect = Rect::new(96.0, 260.0, 330.0, 650.0);
const CONTENT: Rect = Rect::new(526.0, 238.0, 1298.0, 690.0);
const CARD_STYLE: RowStyle = RowStyle {
    w: 230.0,
    h: 230.0,
    gap: 48.0,
    margin_x: 96.0,
    radius: 32.0,
    right_reserve: 96.0,
    ..RowStyle::HOME
};
const INPUT_STYLE: RowStyle = RowStyle {
    w: 280.0,
    h: 158.0,
    gap: 32.0,
    ..CARD_STYLE
};
const TABS: [(Page, &str); 4] = [
    (Page::Home, "Home"),
    (Page::Applications, "Applications"),
    (Page::Inputs, "Inputs"),
    (Page::Controls, "Controls"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Input {
    pub id: String,
    pub label: String,
    pub plugged: bool,
    pub signal: bool,
    pub active: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub captured_at: String,
    pub applications: Vec<Application>,
    pub launch_ids: BTreeSet<String>,
    pub inputs: Vec<Input>,
    pub context: Context,
    pub settings: BTreeMap<String, Vec<Setting>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub initialized: bool,
    pub favourites: BTreeSet<String>,
}

pub struct HomeHost;
impl LogicalState for Page {
    fn write(&self, c: &mut Canon) {
        c.u32(*self as u32);
    }
    fn probe(&self, out: &mut String) {
        out.push_str(self.title().unwrap_or("Home"));
    }
}
impl ScreenArg for Page {
    fn chrome(&self) -> Chrome {
        Chrome::TabBar
    }
    fn id(&self) -> ScreenId {
        ScreenId(*self as u32 + 1)
    }
    fn title(&self) -> Option<&str> {
        Some(match self {
            Self::Home => "Home",
            Self::Applications => "Applications",
            Self::Inputs => "Inputs",
            Self::Controls => "Controls",
            Self::Picture => "Picture",
            Self::Sound => "Sound",
            Self::Network => "Network",
            Self::Accessibility => "Accessibility",
            Self::General => "General",
            Self::PanelCare => "Panel care",
        })
    }
    fn same_instance(&self, other: &Self) -> bool {
        self == other
    }
}
impl Host for HomeHost {
    type Arg = Page;
    type Fx = Action;
    type Msg = ();
    type Elem = u32;
    type Views<'a> = ();
    type Init = ();
    type Memory = ();
}

struct Card {
    key: u32,
    id: String,
    title: CString,
    caption: CString,
    initials: CString,
    texture: (u32, f32, f32),
}
struct Cards {
    items: Vec<Card>,
}
impl CardSource<HomeHost> for Cards {
    fn len(&self) -> usize {
        self.items.len()
    }
    fn elem(&self, i: usize) -> u32 {
        self.items[i].key
    }
    fn index_of(&self, e: &u32) -> Option<usize> {
        self.items.iter().position(|c| c.key == *e)
    }
    fn art(&self, i: usize) -> Art<'_> {
        let c = &self.items[i];
        Art::Texture {
            id: c.texture.0,
            width: c.texture.1,
            height: c.texture.2,
            identity: c.key as u64,
        }
    }
    fn label(&self, i: usize) -> TileLabel {
        let c = &self.items[i];
        TileLabel {
            title: Some(c.title.clone()),
            caption: Some(c.caption.clone()),
            ..Default::default()
        }
    }
    fn holdable(&self, _i: usize) -> bool {
        true
    }
    fn overlay(
        &self,
        p: Painter,
        i: usize,
        tile: &Tile,
        _measure: &dyn plx_machine::machine::Measure,
    ) {
        let c = &self.items[i];
        if c.texture.0 == 0 {
            Label::new(
                c.initials.as_ptr(),
                theme::size::DISPLAY,
                theme::TEXT_PRIMARY,
            )
            .bold()
            .h(HAlign::Center)
            .draw(p, tile.rect);
        }
    }
}

struct Surface {
    page: Page,
    grid: Grid,
    cards: Cards,
    rail: FormTable<String, Action, Page>,
    table: FormTable<String, Action, Page>,
    table_mode: bool,
}
fn entry(page: Page) -> EntryId {
    EntryId(page as u32 + 1)
}
fn root_tab(page: Page) -> Page {
    match page {
        Page::Picture
        | Page::Sound
        | Page::Network
        | Page::Accessibility
        | Page::General
        | Page::PanelCare => Page::Controls,
        _ => page,
    }
}
fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap()
}
impl Surface {
    fn keys(&self) -> Vec<u32> {
        if self.table_mode {
            (0..self.table.table.n_rows().max(0) as usize)
                .filter_map(|i| self.table.key_at(i).map(|k| TABLE_BASE + k.0))
                .collect()
        } else {
            self.cards.items.iter().map(|c| c.key).collect()
        }
    }
    fn key_index(&self, k: u32) -> Option<usize> {
        self.keys().iter().position(|x| *x == k)
    }
}
impl Focusable<HomeHost> for Surface {
    fn groups(&self, _cx: &Cx<'_, HomeHost>, out: &mut Vec<GroupSpec>) {
        out.push(GroupSpec {
            id: GroupId(0),
            kind: GroupKind::Column,
            seat: Seat::Remembered,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Geometric; 4],
            extent: RAIL,
            len: 4,
            elem: ElemKind::Control,
        });
        out.push(GroupSpec {
            id: GroupId(1),
            kind: if self.table_mode {
                GroupKind::Column
            } else {
                GroupKind::Grid {
                    cols: 4,
                    holes: &[],
                }
            },
            seat: Seat::Remembered,
            reachable: AxisMask::BOTH,
            edge: [EdgeRule::Geometric; 4],
            extent: CONTENT,
            len: self.keys().len(),
            elem: if self.table_mode {
                ElemKind::Control
            } else {
                ElemKind::Card
            },
        });
    }
    fn group_of(&self, key: &u32, _cx: &Cx<'_, HomeHost>) -> Option<GroupId> {
        if (RAIL_BASE..RAIL_BASE + 4).contains(key) {
            Some(GroupId(0))
        } else {
            self.key_index(*key).map(|_| GroupId(1))
        }
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _cx: &Cx<'_, HomeHost>) -> Step<u32> {
        let rail = (RAIL_BASE..RAIL_BASE + 4).contains(&key.elem);
        let keys = if rail {
            (RAIL_BASE..RAIL_BASE + 4).collect::<Vec<_>>()
        } else {
            self.keys()
        };
        let Some(i) = keys.iter().position(|k| *k == key.elem) else {
            return Step::Edge;
        };
        let cols = if rail || self.table_mode { 1 } else { 4 };
        let next = match dir {
            Dir::Up => i.checked_sub(cols),
            Dir::Down => Some(i + cols),
            Dir::Left if cols > 1 && i % cols > 0 => Some(i - 1),
            Dir::Right if cols > 1 && i % cols + 1 < cols => Some(i + 1),
            _ => None,
        };
        next.and_then(|n| keys.get(n)).map_or(Step::Edge, |k| {
            Step::Move(FocusKey {
                entry: key.entry,
                elem: *k,
            })
        })
    }
    fn place(&self, key: &u32, cx: &Cx<'_, HomeHost>, at: At) -> Option<Placed> {
        if (RAIL_BASE..RAIL_BASE + 4).contains(key) {
            let i = (*key - RAIL_BASE) as usize;
            let rect = self.rail.table.row_frame(RAIL, i as i32)?;
            Some(Placed {
                rect,
                rest_rect: rect,
                clip: RAIL,
                index: Some(i as u32),
            })
        } else if self.table_mode {
            let i = self
                .table
                .index_of_key(RowKey(key.checked_sub(TABLE_BASE)?))?;
            let rect = self.table.table.row_frame(CONTENT, i as i32)?;
            Some(Placed {
                rect,
                rest_rect: rect,
                clip: CONTENT,
                index: Some(i as u32),
            })
        } else {
            self.grid.place(cx, &self.cards, key, at)
        }
    }
    fn reconcile(&self, want: FocusKey<u32>, _cx: &Cx<'_, HomeHost>) -> FocusKey<u32> {
        if self.group_of(&want.elem, _cx).is_some() {
            want
        } else {
            FocusKey {
                entry: entry(self.page),
                elem: self.keys().first().copied().unwrap_or(RAIL_BASE),
            }
        }
    }
    fn seat(&self, group: GroupId, from: Placed, _cx: &Cx<'_, HomeHost>) -> FocusKey<u32> {
        let key = if group == GroupId(0) {
            RAIL_BASE
                + TABS
                    .iter()
                    .position(|(p, _)| *p == root_tab(self.page))
                    .unwrap_or(0) as u32
        } else {
            let keys = self.keys();
            let mut best = keys.first().copied().unwrap_or(RAIL_BASE);
            let mut dist = f32::MAX;
            for k in keys {
                if let Some(p) = self.place(&k, _cx, At::SpringTarget) {
                    let d = (p.rect.cx() - from.rect.cx()).powi(2)
                        + (p.rect.cy() - from.rect.cy()).powi(2);
                    if d < dist {
                        dist = d;
                        best = k;
                    }
                }
            }
            best
        };
        FocusKey {
            entry: entry(self.page),
            elem: key,
        }
    }
}

pub struct Shell {
    snapshot: Snapshot,
    catalogue: Catalogue,
    prefs: Preferences,
    input: InputMachine<u32>,
    surface: Surface,
    textures: BTreeMap<String, (u32, f32, f32)>,
    now: u32,
    status: CString,
    pending: Vec<Action>,
    exit: bool,
    preferences_changed: bool,
}
impl Shell {
    pub fn new(snapshot: Snapshot, mut prefs: Preferences) -> Self {
        let mut catalogue = Catalogue::default();
        catalogue.refresh(&snapshot.applications, &snapshot.launch_ids);
        let seeded = !prefs.initialized;
        if seeded {
            let preferred = [
                "com.beb.plxnative",
                "youtube.leanback.v4",
                "netflix",
                "amazon",
                "com.apple.appletv",
                "com.disney.disneyplus-prod",
                "bbc.iplayer.3.0",
                "com.crunchyroll.webapp",
            ];
            prefs.favourites.extend(
                catalogue
                    .applications
                    .iter()
                    .filter(|a| preferred.contains(&a.id.as_str()))
                    .map(|a| a.id.clone()),
            );
            if prefs.favourites.is_empty() {
                prefs
                    .favourites
                    .extend(catalogue.applications.iter().take(8).map(|a| a.id.clone()));
            }
            prefs.initialized = true;
        }
        let mut shell = Self {
            snapshot,
            catalogue,
            prefs,
            input: InputMachine::new(),
            surface: Self::blank(Page::Home),
            textures: BTreeMap::new(),
            now: 0,
            status: cstr("Captured TV state • hold OK to pin an application"),
            pending: Vec::new(),
            exit: false,
            preferences_changed: seeded,
        };
        shell.rebuild(Page::Home);
        shell
    }
    fn blank(page: Page) -> Surface {
        let mut rail = FormSection::new("Native Home");
        for (i, (p, title)) in TABS.into_iter().enumerate() {
            rail = rail.item_keyed(
                title.to_owned(),
                RowKey(i as u32 + 1),
                RowKind::Nav(p),
                Action::Inspect(title.to_owned()),
                Row::new(title),
            );
        }
        let mut rt = FormTable::new(2000);
        rt.set(Form::new().section(rail), None);
        Surface {
            page,
            grid: Grid::new(
                entry(page),
                GridSpec::new(282.0, 260.0).columns(4, CARD_STYLE, 566.0),
            ),
            cards: Cards { items: Vec::new() },
            rail: rt,
            table: FormTable::new(2000),
            table_mode: false,
        }
    }
    fn context<'a>(&'a self, measure: &'a dyn plx_machine::machine::Measure) -> Cx<'a, HomeHost> {
        let owner = InputOwner::Entry(entry(self.surface.page));
        Cx {
            views: (),
            tick: Tick {
                ms: self.now,
                dt_us: 16_667,
            },
            measure,
            press: PressRead {
                scale: self.input.press.scale(),
                is_long: self.input.press.is_long(self.now),
                held_ms: self.input.press.held_ms(self.now),
                owner: self.input.dip_owner,
            },
            focus: self.input.engine.read(owner),
            owner,
        }
    }
    fn rebuild(&mut self, page: Page) {
        self.input.cancel_press();
        self.surface = Self::blank(page);
        self.status = cstr(match page {
            Page::Home | Page::Applications => {
                "Hold OK to add or remove a favourite • launches disabled in this preview"
            }
            Page::Inputs => "Captured HDMI status • source switching disabled in this preview",
            _ => "View only • setting changes remain available in LG Settings",
        });
        match page {
            Page::Home | Page::Applications => {
                for app in &self.catalogue.applications {
                    if page == Page::Home && !self.prefs.favourites.contains(&app.id) {
                        continue;
                    }
                    let initials = app.title.chars().take(2).collect::<String>().to_uppercase();
                    let tex = self.textures.get(&app.id).copied().unwrap_or_default();
                    self.surface.cards.items.push(Card {
                        key: app.key.0,
                        id: app.id.clone(),
                        title: cstr(&app.title),
                        caption: cstr(if self.prefs.favourites.contains(&app.id) {
                            "Favourite"
                        } else {
                            "Application"
                        }),
                        initials: cstr(&initials),
                        texture: tex,
                    });
                }
            }
            Page::Inputs => {
                self.surface.grid.set_columns(4, INPUT_STYLE, 566.0);
                for (i, input) in self.snapshot.inputs.iter().enumerate() {
                    let caption = if input.active {
                        "Active input"
                    } else if input.signal {
                        "Signal present"
                    } else if input.plugged {
                        "Connected • no signal"
                    } else {
                        "No signal"
                    };
                    self.surface.cards.items.push(Card {
                        key: 500 + i as u32,
                        id: input.id.clone(),
                        title: cstr(&input.label),
                        caption: cstr(caption),
                        initials: cstr(&format!("HDMI {}", i + 1)),
                        texture: (0, 0.0, 0.0),
                    });
                }
            }
            Page::Controls => {
                self.surface.table_mode = true;
                let mut section = FormSection::new("Controls");
                for (i, (p, title)) in [
                    (Page::Picture, "Picture"),
                    (Page::Sound, "Sound"),
                    (Page::Network, "Network"),
                    (Page::Accessibility, "Accessibility"),
                    (Page::General, "General"),
                    (Page::PanelCare, "Panel care"),
                ]
                .into_iter()
                .enumerate()
                {
                    section = section.item_keyed(
                        title.to_owned(),
                        RowKey(i as u32 + 1),
                        RowKind::Nav(p),
                        Action::Inspect(title.to_owned()),
                        Row::new(title),
                    );
                }
                self.surface.table.set(Form::new().section(section), None);
            }
            _ => {
                self.surface.table_mode = true;
                let name = page
                    .title()
                    .unwrap_or("General")
                    .to_lowercase()
                    .replace(' ', "_");
                let settings = self
                    .snapshot
                    .settings
                    .get(&name)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                self.surface
                    .table
                    .set(controls_form(settings, &self.snapshot.context), None);
            }
        }
        let owner = InputOwner::Entry(entry(page));
        let old = self.input.engine.current(owner);
        let cx = Cx::<HomeHost> {
            views: (),
            tick: Tick::default(),
            measure: &LegacyMeasure,
            press: PressRead::default(),
            focus: self.input.engine.read(owner),
            owner,
        };
        self.input.engine.enter(
            owner,
            &self.surface,
            FocusTarget::ContainerGroup(GroupId(if self.surface.keys().is_empty() {
                0
            } else {
                1
            })),
            old,
            &cx,
        );
    }
    pub fn load_icons(&mut self, assets: &std::path::Path) {
        for (id, _, _) in self.textures.values() {
            plx_gfx::gfx::delete_tex(*id);
        }
        self.textures.clear();
        for app in self.catalogue.applications.iter().take(64) {
            let Some(path) = &app.icon else { continue };
            let path = std::path::Path::new(path);
            if path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(assets.join(path)) else {
                continue;
            };
            if bytes.len() > 1024 * 1024 {
                continue;
            }
            if let Some((w, h, pixels)) = plx_gfx::img::img_decode_owned(&bytes) {
                if w > 512 || h > 512 {
                    continue;
                }
                let id = plx_gfx::gfx::upload_rgba(0, w as i32, h as i32, pixels.as_ptr());
                self.textures
                    .insert(app.id.clone(), (id, w as f32, h as f32));
            }
        }
        self.rebuild(self.surface.page);
    }
    fn moved(&mut self, outcome: plx_ui::focus::Outcome<u32>) {
        if let plx_ui::focus::Outcome::Moved { from, to, by } = outcome {
            self.input.abandon_if_off(entry(self.surface.page), to);
            let cx = self.context(&LegacyMeasure);
            let mut buf = Vec::new();
            let mut present = Present::new();
            let mut fx = Effects::new(&mut buf, MachineId::Input, &mut present);
            // Build an independent projection so the grid receives the move without borrowing Input.
            let read = Cx::<HomeHost> {
                views: (),
                tick: cx.tick,
                measure: &LegacyMeasure,
                press: cx.press,
                focus: cx.focus.clone(),
                owner: cx.owner,
            };
            self.surface.grid.on(
                &ScreenEvent::FocusMoved { from, to, by },
                &read,
                &self.surface.cards,
                &mut fx,
            );
        }
    }
    pub fn direction(&mut self, dir: Dir) {
        self.input.hit.note_dpad();
        let owner = InputOwner::Entry(entry(self.surface.page));
        let cx = Cx::<HomeHost> {
            views: (),
            tick: Tick {
                ms: self.now,
                dt_us: 16_667,
            },
            measure: &LegacyMeasure,
            press: PressRead::default(),
            focus: self.input.engine.read(owner),
            owner,
        };
        let outcome = self
            .input
            .engine
            .move_dir(owner, &self.surface, &[], dir, &cx);
        self.moved(outcome);
    }
    pub fn cancel_press(&mut self) {
        self.input.cancel_press();
    }
    pub fn repeat(&mut self) {
        self.input.note_alive(self.now);
    }
    pub fn press(&mut self, down: bool, from: PressFrom) {
        if down {
            if let Some(key) = self
                .input
                .engine
                .current(InputOwner::Entry(entry(self.surface.page)))
            {
                let card = self
                    .surface
                    .group_of(&key.elem, &self.context(&LegacyMeasure))
                    == Some(GroupId(1))
                    && !self.surface.table_mode;
                self.input.arm(
                    PressArm {
                        key,
                        from,
                        holdable: card && self.surface.page != Page::Inputs,
                    },
                    MachineId::Input,
                    self.now,
                );
            }
        } else {
            self.input.release(self.now);
        }
    }
    pub fn back(&mut self) {
        if matches!(
            self.surface.page,
            Page::Picture
                | Page::Sound
                | Page::Network
                | Page::Accessibility
                | Page::General
                | Page::PanelCare
        ) {
            self.rebuild(Page::Controls)
        } else if self.surface.page != Page::Home {
            self.rebuild(Page::Home)
        } else {
            self.exit = true
        }
    }
    pub fn pointer(&mut self, x: f32, y: f32, click: bool) {
        let owner = InputOwner::Entry(entry(self.surface.page));
        let kind = if click {
            plx_ui::hit::PointerKind::Click
        } else {
            plx_ui::hit::PointerKind::Move
        };
        let resolution = self.input.hit.resolve(
            Some(entry(self.surface.page)),
            kind,
            x,
            y,
            self.input.engine.current(owner),
        );
        if let Some(key) = resolution.focus {
            let group = self
                .surface
                .group_of(&key.elem, &self.context(&LegacyMeasure));
            let outcome = self.input.engine.set(owner, key, group, By::Pointer);
            self.moved(outcome);
        }
        if click {
            if let Some((key, _)) = resolution.activate {
                self.activate(key, false);
            }
        }
    }
    fn activate(&mut self, key: FocusKey<u32>, hold: bool) {
        if key.entry != entry(self.surface.page) {
            return;
        }
        if (RAIL_BASE..RAIL_BASE + 4).contains(&key.elem) {
            self.rebuild(TABS[(key.elem - RAIL_BASE) as usize].0);
            return;
        }
        if self.surface.table_mode {
            if let Some(i) = key
                .elem
                .checked_sub(TABLE_BASE)
                .and_then(|k| self.surface.table.index_of_key(RowKey(k)))
            {
                match self.surface.table.activate(i) {
                    Some(plx_ui::form::Activation::Push(p)) => self.rebuild(p),
                    Some(plx_ui::form::Activation::Action(a)) => {
                        self.status = cstr("View only — use LG Settings for changes");
                        self.pending.push(a);
                    }
                    None => {}
                }
            }
            return;
        }
        let Some(i) = self.surface.cards.index_of(&key.elem) else {
            return;
        };
        let id = self.surface.cards.items[i].id.clone();
        if hold && matches!(self.surface.page, Page::Home | Page::Applications) {
            if !self.prefs.favourites.remove(&id) {
                self.prefs.favourites.insert(id);
            }
            self.preferences_changed = true;
            self.rebuild(self.surface.page);
        } else {
            self.status =
                cstr("Application/source request — connection is not enabled in this pilot");
            self.pending.push(if self.surface.page == Page::Inputs {
                Action::RequestInput(id)
            } else {
                Action::RequestApplication(id)
            });
        }
    }
    pub fn tick(&mut self, ms: u32, dt: f32) {
        self.now = ms;
        for event in self.input.tick(ms, dt) {
            match event {
                PressEvent::Commit(_, _, k) => self.activate(k, false),
                PressEvent::Hold(_, _, k) => self.activate(k, true),
            }
        }
        let owner = InputOwner::Entry(entry(self.surface.page));
        let cx = Cx::<HomeHost> {
            views: (),
            tick: Tick {
                ms,
                dt_us: (dt * 1e6) as u32,
            },
            measure: &LegacyMeasure,
            press: PressRead {
                scale: self.input.press.scale(),
                owner: self.input.dip_owner,
                ..Default::default()
            },
            focus: self.input.engine.read(owner),
            owner,
        };
        let mut buf = Vec::new();
        let mut present = Present::new();
        let mut fx = Effects::new(&mut buf, MachineId::Input, &mut present);
        self.surface.grid.on(
            &ScreenEvent::Tick(cx.tick),
            &cx,
            &self.surface.cards,
            &mut fx,
        );
        let current = cx.focus.current.map(|k| k.elem);
        self.surface.rail.table.list_focused =
            current.is_some_and(|k| (RAIL_BASE..RAIL_BASE + 4).contains(&k));
        self.surface.rail.table.sel = current
            .filter(|k| (RAIL_BASE..RAIL_BASE + 4).contains(k))
            .map(|k| (k - RAIL_BASE) as i32)
            .unwrap_or(
                TABS.iter()
                    .position(|(p, _)| *p == root_tab(self.surface.page))
                    .unwrap_or(0) as i32,
            );
        self.surface.table.table.list_focused = current.is_some_and(|k| k >= TABLE_BASE);
        if let Some(i) = current
            .and_then(|k| k.checked_sub(TABLE_BASE))
            .and_then(|k| self.surface.table.index_of_key(RowKey(k)))
        {
            self.surface.table.table.sel = i as i32;
        }
        self.surface.rail.table.update(dt, RAIL.h);
        self.surface.table.table.update(dt, CONTENT.h);
    }
    pub fn draw(&mut self) {
        let p = Painter::root();
        let ground = theme::SURFACE_APP;
        plx_gfx::gfx::frame_clear(ground[0], ground[1], ground[2]);
        Label::new(
            c"Native Home".as_ptr(),
            theme::size::DISPLAY,
            theme::TEXT_PRIMARY,
        )
        .bold()
        .draw(p, Rect::new(96.0, 60.0, 1320.0, 76.0));
        let subtitle = cstr(&format!(
            "{}  •  TV state captured {}",
            self.surface.page.title().unwrap_or("Home"),
            self.snapshot.captured_at
        ));
        Label::new(
            subtitle.as_ptr(),
            theme::size::CAPTION,
            theme::TEXT_SECONDARY,
        )
        .draw(p, Rect::new(96.0, 154.0, 1728.0, 44.0));
        p.rect(
            Rect::new(96.0, 214.0, 1728.0, 1.0),
            0.0,
            theme::HAIRLINE,
            theme::HAIRLINE,
            0.0,
        );
        self.surface.rail.table.draw_page(p, RAIL, &LegacyMeasure);
        let cx = self.context(&LegacyMeasure);
        let mut f = DrawFrame::<HomeHost>::new(&cx, p);
        if self.surface.table_mode {
            self.surface
                .table
                .table
                .draw_page(p, CONTENT, &LegacyMeasure);
        } else {
            let _clip = f.clip(p, CONTENT);
            self.surface
                .grid
                .draw(&mut f, p.clipped(CONTENT), &self.surface.cards);
        }
        for i in 0..4 {
            if let Some(r) = self.surface.rail.table.row_frame(RAIL, i) {
                f.stop(
                    p,
                    Stop {
                        key: FocusKey {
                            entry: entry(self.surface.page),
                            elem: RAIL_BASE + i as u32,
                        },
                        rect: r,
                        rest_rect: r,
                        clip: RAIL,
                        hover: Hover::Focus,
                        activate: Activate::Press,
                    },
                );
            }
        }
        if self.surface.table_mode {
            for i in 0..self.surface.table.table.n_rows() {
                if let Some(key) = self.surface.table.key_at(i as usize) {
                    if let Some(r) = self.surface.table.table.row_frame(CONTENT, i) {
                        f.stop(
                            p,
                            Stop {
                                key: FocusKey {
                                    entry: entry(self.surface.page),
                                    elem: TABLE_BASE + key.0,
                                },
                                rect: r,
                                rest_rect: r,
                                clip: CONTENT,
                                hover: Hover::Focus,
                                activate: Activate::Press,
                            },
                        );
                    }
                }
            }
        }
        let stops = f.into_stops();
        self.input.hit.fill(stops);
        self.input.hit.swap();
        Label::new(
            self.status.as_ptr(),
            theme::size::CAPTION,
            theme::TEXT_TERTIARY,
        )
        .draw(p, Rect::new(96.0, 985.0, 1728.0, 44.0));
    }
    pub fn focused_item(&self) -> Option<String> {
        let key = self
            .input
            .engine
            .current(InputOwner::Entry(entry(self.surface.page)))?;
        if (RAIL_BASE..RAIL_BASE + 4).contains(&key.elem) {
            Some(TABS[(key.elem - RAIL_BASE) as usize].1.to_owned())
        } else if self.surface.table_mode {
            self.surface
                .table
                .index_of_key(RowKey(key.elem.checked_sub(TABLE_BASE)?))
                .map(|i| format!("control-row-{i}"))
        } else {
            self.surface
                .cards
                .items
                .iter()
                .find(|c| c.key == key.elem)
                .map(|c| c.id.clone())
        }
    }
    pub fn should_exit(&self) -> bool {
        self.exit
    }
    pub fn page(&self) -> Page {
        self.surface.page
    }
    pub fn preferences(&self) -> &Preferences {
        &self.prefs
    }
    pub fn take_preferences_changed(&mut self) -> bool {
        std::mem::take(&mut self.preferences_changed)
    }
    pub fn take_requests(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.pending)
    }
    pub fn open(&mut self, page: Page) {
        self.rebuild(page)
    }
}
impl Drop for Shell {
    fn drop(&mut self) {
        for (id, _, _) in self.textures.values() {
            plx_gfx::gfx::delete_tex(*id);
        }
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
