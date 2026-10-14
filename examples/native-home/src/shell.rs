//! The interactive application shell. Focus, hit resolution, press ownership and card motion
//! come from plx_ui. Only dashboard preferences are writable in this slice; TV actions are
//! typed requests checked by a separate, capability-gated executor.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;

use plx_machine::machine::{
    Canon, Chrome, Cx, Effects, EntryId, FocusKey, GroupId, Host, InputOwner, LogicalState,
    MachineId, PressArm, PressFrom, PressRead, ScreenArg, ScreenId, Tick,
};
use plx_machine::present::Present;
use plx_ui::cards::{CardSource, Grid, GridSpec, RowStyle, SectionFrame, Shelf, Tile, TileLabel};
use plx_ui::form::{Form, FormSection, FormTable, RowKey, RowKind};
use plx_ui::input::{InputMachine, PressEvent};
use plx_ui::label::{HAlign, Label};
use plx_ui::screen::{
    Activate, At, AxisMask, By, Dir, DrawFrame, EdgeRule, ElemKind, FocusTarget, Focusable,
    GroupKind, GroupSpec, Hover, Link, Placed, ScreenEvent, Seat, Step, Stop,
};
use plx_ui::table::Row;
use plx_ui::widgets::{Art, LegacyMeasure};
use plx_ui::{theme, Painter, Rect};
use serde::{Deserialize, Serialize};

use crate::live::{self, Freshness, Update};
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
const HOME_APP_STYLE: RowStyle = RowStyle {
    w: 270.0,
    h: 218.0,
    gap: 44.0,
    margin_x: 566.0,
    ..CARD_STYLE
};
const HOME_INPUT_STYLE: RowStyle = RowStyle {
    margin_x: 566.0,
    ..INPUT_STYLE
};
const HOME_FAVOURITES: SectionFrame = SectionFrame {
    y: 330.0,
    clip: Rect::new(526.0, 292.0, 1298.0, 320.0),
};
const HOME_INPUTS: SectionFrame = SectionFrame {
    y: 716.0,
    clip: Rect::new(526.0, 678.0, 1298.0, 282.0),
};
const TABS: [(Page, &str); 4] = [
    (Page::Home, "Home"),
    (Page::Applications, "Apps"),
    (Page::Inputs, "Inputs"),
    (Page::Controls, "Controls"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub id: String,
    pub label: String,
    pub plugged: bool,
    pub signal: bool,
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub captured_at: String,
    pub applications: Vec<Application>,
    pub launch_ids: BTreeSet<String>,
    pub inputs: Vec<Input>,
    pub context: Context,
    pub settings: BTreeMap<String, Vec<Setting>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        if input_key(&c.id).is_some() {
            // Every source keeps its status in the tile; a focused long name uses the
            // shared label marquee beneath it, like every other card.
            return TileLabel {
                title: Some(c.title.clone()),
                ..Default::default()
            };
        }
        TileLabel {
            title: Some(c.title.clone()),
            caption: Some(c.caption.clone()),
            ..Default::default()
        }
    }
    fn holdable(&self, i: usize) -> bool {
        input_key(&self.items[i].id).is_none()
    }
    fn overlay(
        &self,
        p: Painter,
        i: usize,
        tile: &Tile,
        _measure: &dyn plx_machine::machine::Measure,
    ) {
        let c = &self.items[i];
        if input_key(&c.id).is_some() {
            let inset = tile.rect.w * 0.07;
            let title = cstr(&plx_gfx::text::elide(
                c.title.to_str().unwrap_or(""),
                tile.rect.w - 2.0 * inset,
                theme::size::HEADLINE,
                1,
                false,
            ));
            Label::new(title.as_ptr(), theme::size::HEADLINE, theme::TEXT_PRIMARY)
                .bold()
                .draw(
                    p,
                    Rect::new(
                        tile.rect.x + inset,
                        tile.rect.y + inset,
                        tile.rect.w - 2.0 * inset,
                        tile.rect.h * 0.48,
                    ),
                );
            let (connection, signal) = c
                .caption
                .to_str()
                .unwrap_or("")
                .split_once('\n')
                .unwrap_or(("Unavailable", ""));
            for (text, top) in [(connection, 0.49), (signal, 0.70)] {
                Label::new(
                    cstr(text).as_ptr(),
                    theme::size::CAPTION,
                    theme::TEXT_SECONDARY,
                )
                .draw(
                    p,
                    Rect::new(
                        tile.rect.x + inset,
                        tile.rect.y + tile.rect.h * top,
                        tile.rect.w - 2.0 * inset,
                        tile.rect.h * 0.21,
                    ),
                );
            }
        } else if c.texture.0 == 0 {
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
    home_favourites: Shelf,
    home_inputs: Shelf,
    input_cards: Cards,
    rail: FormTable<String, Action, Page>,
    table: FormTable<String, Action, Page>,
    table_mode: bool,
}
fn input_key(id: &str) -> Option<u32> {
    match id {
        "HDMI_1" => Some(500),
        "HDMI_2" => Some(501),
        "HDMI_3" => Some(502),
        "HDMI_4" => Some(503),
        _ => None,
    }
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
            self.cards
                .items
                .iter()
                .chain(self.input_cards.items.iter())
                .map(|c| c.key)
                .collect()
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
            reachable: AxisMask::HORIZONTAL,
            edge: [EdgeRule::Geometric; 4],
            extent: RAIL,
            len: 4,
            elem: ElemKind::Control,
        });
        if self.page == Page::Home {
            out.push(self.home_favourites.group_spec(
                GroupId(1),
                self.cards.len(),
                HOME_FAVOURITES.clip,
            ));
            out.push(self.home_inputs.group_spec(
                GroupId(2),
                self.input_cards.len(),
                HOME_INPUTS.clip,
            ));
            return;
        }
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
        } else if self.input_cards.index_of(key).is_some() {
            Some(GroupId(2))
        } else {
            self.key_index(*key).map(|_| GroupId(1))
        }
    }
    fn neighbour(&self, key: FocusKey<u32>, dir: Dir, _cx: &Cx<'_, HomeHost>) -> Step<u32> {
        if self.page == Page::Home {
            if self.cards.index_of(&key.elem).is_some() {
                return self
                    .home_favourites
                    .neighbour::<HomeHost, _>(&self.cards, key, dir);
            }
            if self.input_cards.index_of(&key.elem).is_some() {
                return self
                    .home_inputs
                    .neighbour::<HomeHost, _>(&self.input_cards, key, dir);
            }
        }
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
        } else if self.page == Page::Home {
            self.home_favourites
                .place(cx, &self.cards, key, HOME_FAVOURITES, at)
                .or_else(|| {
                    self.home_inputs
                        .place(cx, &self.input_cards, key, HOME_INPUTS, at)
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
        } else if self.page == Page::Home {
            let seat = if group == GroupId(2) {
                self.home_inputs
                    .seat::<HomeHost, _>(&self.input_cards, from)
            } else {
                self.home_favourites.seat::<HomeHost, _>(&self.cards, from)
            };
            seat.map(|k| k.elem).unwrap_or(RAIL_BASE)
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
    live_state: Option<Freshness>,
    live_now: u32,
    live_updates: u32,
    app_launches_enabled: bool,
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
            live_state: None,
            live_now: 0,
            live_updates: 0,
            app_launches_enabled: false,
        };
        shell.rebuild(Page::Home);
        shell
    }
    fn blank(page: Page) -> Surface {
        let mut rail = FormSection::new("Luma Home");
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
            home_favourites: Shelf::new(entry(page), &HOME_APP_STYLE),
            home_inputs: Shelf::new(entry(page), &HOME_INPUT_STYLE),
            input_cards: Cards { items: Vec::new() },
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
                if self.app_launches_enabled {
                    "OK to open • hold OK to add or remove a favourite"
                } else {
                    "Hold OK to add or remove a favourite • launches disabled in this preview"
                }
            }
            Page::Inputs => "HDMI status • source switching disabled in this preview",
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
                self.surface.cards = self.input_cards();
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
        if page == Page::Home {
            self.surface.input_cards = self.input_cards();
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
        let outcome = self.input.engine.enter(
            owner,
            &self.surface,
            FocusTarget::ContainerGroup(GroupId(if self.surface.keys().is_empty() {
                0
            } else {
                if page == Page::Home && self.surface.cards.items.is_empty() {
                    2
                } else {
                    1
                }
            })),
            old,
            &cx,
        );
        self.moved(outcome);
    }
    fn input_cards(&self) -> Cards {
        Cards {
            items: self
                .snapshot
                .inputs
                .iter()
                .filter_map(|input| {
                    let key = input_key(&input.id)?;
                    let caption = format!(
                        "{}{}\n{}",
                        if input.active { "Active • " } else { "" },
                        if input.plugged {
                            "Connected"
                        } else {
                            "No cable"
                        },
                        if input.signal {
                            "Signal present"
                        } else {
                            "No signal"
                        }
                    );
                    Some(Card {
                        key,
                        id: input.id.clone(),
                        title: cstr(&input.label),
                        caption: cstr(&caption),
                        initials: cstr(&input.id.replace('_', " ")),
                        texture: (0, 0.0, 0.0),
                    })
                })
                .collect(),
        }
    }
    pub fn apply_live(&mut self, update: &Update, now: u32) {
        let before = self.snapshot.clone();
        let mut fresh = self.live_state.take().unwrap_or_default();
        live::merge(&mut self.snapshot, update, &mut fresh);
        self.live_state = Some(fresh);
        self.live_now = now;
        self.live_updates += 1;
        self.catalogue
            .refresh(&self.snapshot.applications, &self.snapshot.launch_ids);
        self.refresh_shown_data(&before);
    }
    pub fn age_live(&mut self, now: u32) {
        self.live_now = now;
        if let Some(fresh) = &self.live_state {
            let due = [("picture", fresh.picture), ("sound", fresh.sound)]
                .into_iter()
                .any(|(section, stamp)| {
                    !Freshness::current(stamp, now)
                        && self
                            .snapshot
                            .settings
                            .get(section)
                            .is_some_and(|rows| rows.iter().any(|r| r.available))
                });
            if due {
                let before = self.snapshot.clone();
                live::expire(&mut self.snapshot, fresh, now);
                self.refresh_shown_data(&before);
            }
        }
    }
    fn refresh_shown_data(&mut self, before: &Snapshot) {
        let page = self.surface.page;
        let changed = match page {
            Page::Home => {
                before.applications != self.snapshot.applications
                    || before.launch_ids != self.snapshot.launch_ids
                    || before.inputs != self.snapshot.inputs
            }
            Page::Applications => {
                before.applications != self.snapshot.applications
                    || before.launch_ids != self.snapshot.launch_ids
            }
            Page::Inputs => before.inputs != self.snapshot.inputs,
            Page::Controls => false,
            _ => {
                before.context != self.snapshot.context || before.settings != self.snapshot.settings
            }
        };
        if !changed {
            return;
        }
        // Keep the shared grid's motion/viewport on a data refresh; the engine owns the key.
        let grid = std::mem::replace(
            &mut self.surface.grid,
            Grid::new(
                entry(page),
                GridSpec::new(282.0, 260.0).columns(4, CARD_STYLE, 566.0),
            ),
        );
        let favourites = std::mem::replace(
            &mut self.surface.home_favourites,
            Shelf::new(entry(page), &HOME_APP_STYLE),
        );
        let inputs = std::mem::replace(
            &mut self.surface.home_inputs,
            Shelf::new(entry(page), &HOME_INPUT_STYLE),
        );
        self.rebuild(page);
        self.surface.grid = grid;
        self.surface.home_favourites = favourites;
        self.surface.home_inputs = inputs;
        let owner = InputOwner::Entry(entry(page));
        if let Some(to) = self.input.engine.current(owner) {
            self.moved(plx_ui::focus::Outcome::Moved {
                from: Some(to),
                to,
                by: By::Restore,
            });
        }
    }
    fn request_is_current(&self, input: bool) -> bool {
        let Some(fresh) = &self.live_state else {
            return true;
        };
        Freshness::current(
            if input { fresh.inputs } else { fresh.catalogue },
            self.live_now,
        )
    }
    pub fn enable_app_launches(&mut self, enabled: bool) {
        self.app_launches_enabled = enabled;
        self.rebuild(self.surface.page);
    }
    pub fn live_updates(&self) -> u32 {
        self.live_updates
    }
    fn reading_label(&self) -> String {
        let Some(fresh) = &self.live_state else {
            return format!("TV state captured {}", self.snapshot.captured_at);
        };
        if matches!(
            self.surface.page,
            Page::Network | Page::Accessibility | Page::General | Page::PanelCare
        ) {
            return "Managed by LG".into();
        }
        if self.surface.page == Page::Controls {
            return if Freshness::current(fresh.picture, self.live_now) {
                "Picture readings live • some readings unavailable".into()
            } else {
                "Readings unavailable • LG Settings available".into()
            };
        }
        let stamp = match self.surface.page {
            Page::Home | Page::Applications => fresh.catalogue,
            Page::Inputs => fresh.inputs,
            Page::Sound => fresh.sound,
            Page::Picture => fresh.picture,
            _ => fresh.picture,
        };
        if !Freshness::current(stamp, self.live_now) {
            return "Reading unavailable • last state kept".into();
        }
        let label = if fresh.simulated {
            "Simulated update"
        } else {
            "Live TV state"
        };
        if self.surface.page == Page::Picture {
            format!(
                "{} / {} / {} • {label}",
                self.snapshot.context.input,
                self.snapshot.context.dynamic_range.to_uppercase(),
                self.snapshot.context.picture_mode
            )
        } else {
            label.into()
        }
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
            if self.surface.page == Page::Home {
                self.surface.home_favourites.on(
                    &ScreenEvent::FocusMoved { from, to, by },
                    &read,
                    &self.surface.cards,
                    &mut fx,
                );
                self.surface.home_inputs.on(
                    &ScreenEvent::FocusMoved { from, to, by },
                    &read,
                    &self.surface.input_cards,
                    &mut fx,
                );
            } else {
                self.surface.grid.on(
                    &ScreenEvent::FocusMoved { from, to, by },
                    &read,
                    &self.surface.cards,
                    &mut fx,
                );
            }
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
        let links = if self.surface.page == Page::Home {
            vec![
                Link {
                    from: GroupId(1),
                    dir: Dir::Down,
                    to: GroupId(2),
                },
                Link {
                    from: GroupId(2),
                    dir: Dir::Up,
                    to: GroupId(1),
                },
            ]
        } else {
            Vec::new()
        };
        let outcome = self
            .input
            .engine
            .move_dir(owner, &self.surface, &links, dir, &cx);
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
        let Some(card) = self
            .surface
            .cards
            .items
            .iter()
            .chain(self.surface.input_cards.items.iter())
            .find(|c| c.key == key.elem)
        else {
            return;
        };
        let id = card.id.clone();
        let input = input_key(&id).is_some();
        if hold && !input && matches!(self.surface.page, Page::Home | Page::Applications) {
            if !self.prefs.favourites.remove(&id) {
                self.prefs.favourites.insert(id);
            }
            self.preferences_changed = true;
            self.rebuild(self.surface.page);
        } else {
            if !self.request_is_current(input) {
                self.status = cstr("Reading unavailable • refresh before opening this item");
                return;
            }
            self.status = cstr(if input {
                "Source switching is unavailable in this preview"
            } else if self.app_launches_enabled {
                "Opening application…"
            } else {
                "Application launching is unavailable in this preview"
            });
            self.pending.push(if input {
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
        if self.surface.page == Page::Home {
            self.surface.home_favourites.on(
                &ScreenEvent::Tick(cx.tick),
                &cx,
                &self.surface.cards,
                &mut fx,
            );
            self.surface.home_inputs.on(
                &ScreenEvent::Tick(cx.tick),
                &cx,
                &self.surface.input_cards,
                &mut fx,
            );
        } else {
            self.surface.grid.on(
                &ScreenEvent::Tick(cx.tick),
                &cx,
                &self.surface.cards,
                &mut fx,
            );
        }
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
        let ground = theme::SURFACE_OLED;
        plx_gfx::gfx::frame_clear(ground[0], ground[1], ground[2]);
        Label::new(
            cstr(self.surface.page.title().unwrap_or("Home")).as_ptr(),
            theme::size::DISPLAY,
            theme::TEXT_PRIMARY,
        )
        .bold()
        .draw(p, Rect::new(96.0, 60.0, 1320.0, 76.0));
        let subtitle = cstr(&self.reading_label());
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
        } else if self.surface.page == Page::Home {
            Label::new(
                c"Favourites".as_ptr(),
                theme::size::HEADLINE,
                theme::TEXT_PRIMARY,
            )
            .bold()
            .draw(p, Rect::new(566.0, 244.0, 1180.0, 48.0));
            let source_heading = if self
                .live_state
                .as_ref()
                .is_some_and(|fresh| !Freshness::current(fresh.inputs, self.live_now))
            {
                c"Inputs • status unavailable"
            } else {
                c"Inputs"
            };
            Label::new(
                source_heading.as_ptr(),
                theme::size::HEADLINE,
                theme::TEXT_PRIMARY,
            )
            .bold()
            .draw(p, Rect::new(566.0, 628.0, 1180.0, 48.0));
            if self.surface.cards.items.is_empty() {
                Label::new(
                    c"Hold OK on an app to add a favourite".as_ptr(),
                    theme::size::BODY,
                    theme::TEXT_SECONDARY,
                )
                .draw(p, Rect::new(566.0, 366.0, 1180.0, 100.0));
            }
            {
                let _clip = f.clip(p, HOME_FAVOURITES.clip);
                self.surface.home_favourites.draw(
                    &mut f,
                    p.clipped(HOME_FAVOURITES.clip),
                    &self.surface.cards,
                    HOME_FAVOURITES,
                );
            }
            {
                let _clip = f.clip(p, HOME_INPUTS.clip);
                self.surface.home_inputs.draw(
                    &mut f,
                    p.clipped(HOME_INPUTS.clip),
                    &self.surface.input_cards,
                    HOME_INPUTS,
                );
            }
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
    /// Used by the app picker/search and by identity-based verification, never a private cursor.
    pub fn focus_application(&mut self, id: &str) -> bool {
        if !matches!(self.surface.page, Page::Home | Page::Applications) {
            return false;
        }
        let Some(card) = self.surface.cards.items.iter().find(|c| c.id == id) else {
            return false;
        };
        let owner = InputOwner::Entry(entry(self.surface.page));
        let outcome = self.input.engine.set(
            owner,
            FocusKey {
                entry: entry(self.surface.page),
                elem: card.key,
            },
            Some(GroupId(1)),
            By::Restore,
        );
        self.moved(outcome);
        true
    }
    pub fn focus_input(&mut self, id: &str) -> bool {
        if !matches!(self.surface.page, Page::Home | Page::Inputs) {
            return false;
        }
        let Some(card) = self
            .surface
            .cards
            .items
            .iter()
            .chain(self.surface.input_cards.items.iter())
            .find(|c| c.id == id)
        else {
            return false;
        };
        let owner = InputOwner::Entry(entry(self.surface.page));
        let outcome = self.input.engine.set(
            owner,
            FocusKey {
                entry: entry(self.surface.page),
                elem: card.key,
            },
            Some(GroupId(if self.surface.page == Page::Home {
                2
            } else {
                1
            })),
            By::Restore,
        );
        self.moved(outcome);
        true
    }
    pub fn focus_lg_settings(&mut self) -> bool {
        if !self.surface.table_mode || self.surface.table.index_of_key(RowKey(1000)).is_none() {
            return false;
        }
        let owner = InputOwner::Entry(entry(self.surface.page));
        let key = FocusKey {
            entry: entry(self.surface.page),
            elem: TABLE_BASE + 1000,
        };
        let outcome = self
            .input
            .engine
            .set(owner, key, Some(GroupId(1)), By::Restore);
        self.moved(outcome);
        true
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
                .chain(self.surface.input_cards.items.iter())
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
    pub fn storage_notice(&mut self, message: &str) {
        self.status = cstr(message);
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
