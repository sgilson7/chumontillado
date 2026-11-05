//! The door into the core. Every function takes strings and returns a JSON
//! string: `{"ok": ...}` on success or `{"error": "..."}` with a message the
//! page can show as it is. The shim forwards these calls and nothing else.
//!
//! The core describes the screen as a view model (`Ui`): the screen's name,
//! its panels and their actions, the HUD's lines and the glossary's
//! definitions. The page builds the DOM from it without knowing what a
//! panel means. Settings, screens and flags all come back through one
//! function, `act`, with a dotted name. (Builder's template and Chumworld
//! Arena's api.rs, cut to M1's screens.)

use serde::Serialize;

use crate::game::{Game, Screen};
use crate::prim::{self, MeshId};
use crate::{input, look, render, world};

pub fn ok<T: Serialize>(value: &T) -> String {
    serde_json::json!({ "ok": value }).to_string()
}

fn error(message: String) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// One button: the action and value the page sends back through `act`.
/// `engage`: pressing it should take the pointer for play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    pub name: String,
    pub value: String,
    pub label: String,
    pub enabled: bool,
    pub engage: bool,
    /// M4: the field whose text this button sends as its value, if any
    #[serde(skip_serializing_if = "String::is_empty")]
    pub from: String,
    /// M4: the field whose text this button copies, if any (the page's
    /// clipboard; nothing goes through `act`)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub copies: String,
    /// M4: a small button beside others (the copy buttons, the seat switches)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub small: bool,
}

/// M4: a box of text: a code or link to copy (`fixed`), or a box to type
/// a code into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub value: String,
    pub fixed: bool,
}

/// M4: online play as the page's transport needs it: the commands to run,
/// and the status key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnlineView {
    pub cmds: Vec<crate::versus::Cmd>,
    pub status: String,
    /// this browser's checksums, `tick:sum` newest last, for the online
    /// check to compare across browsers
    pub sums: String,
}

/// A setting's row: its name, its value in words, and the buttons that
/// change it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Setting {
    pub id: String,
    pub label: String,
    pub value: String,
    pub actions: Vec<Action>,
}

/// A row of the controls table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub key: String,
    pub does: String,
}

/// One block of the screen. Empty fields are left out of the JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Panel {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub heading: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<Setting>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub rows_heading: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Row>,
}

/// A line over the play view: its id, its text, and whether it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hud {
    pub id: String,
    pub text: String,
    pub shown: bool,
    /// M5: where on the screen it sits, hundredths of a percent across and
    /// down (the markers through walls); none for the fixed lines
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<[i32; 2]>,
}

/// A term and its definition, for the page's hover hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hint {
    pub term: String,
    pub definition: String,
}

/// Everything the page draws besides the 3D view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ui {
    pub title: String,
    pub screen: String,
    pub panels: Vec<Panel>,
    pub hud: Vec<Hud>,
    /// the page should hold the pointer
    pub capture: bool,
    pub hints: Vec<Hint>,
    /// shown where WebGL 2 is missing
    pub no_webgl: String,
    /// M4
    pub online: OnlineView,
}

fn action(g: &Game, name: &str, value: &str, label_key: &str, engage: bool) -> Action {
    Action {
        name: name.into(),
        value: value.into(),
        label: g.strings.get(label_key),
        enabled: true,
        engage,
        from: String::new(),
        copies: String::new(),
        small: false,
    }
}

fn field(g: &Game, id: &str, label: &str, value: &str, fixed: bool) -> Field {
    Field {
        id: id.into(),
        label: g.strings.get(label),
        value: value.into(),
        fixed,
    }
}

/// CS-Q22: the shooting-by-speed card for seat `me`: hits of shots fired
/// in each band of speed, time at run speed or more, and the best speed.
fn shooting_panel(g: &Game) -> Panel {
    let s = &g.strings;
    let p = &g.view_world().players[g.me()];
    let sh = &p.shooting;
    let mut out = panel("shooting");
    out.heading = s.get("shooting");
    let pct = |a: i32, b: i32| if b > 0 { a * 100 / b } else { 0 };
    out.rows = ["band_walk", "band_run", "band_fast", "band_trick"]
        .iter()
        .enumerate()
        .map(|(k, key)| Row {
            key: s.get(key),
            does: if sh.fired[k] == 0 {
                s.get("band_none")
            } else {
                s.get("band_count")
                    .replace("{pct}", &pct(sh.hit[k], sh.fired[k]).to_string())
                    .replace("{hit}", &sh.hit[k].to_string())
                    .replace("{fired}", &sh.fired[k].to_string())
            },
        })
        .collect();
    out.rows.push(Row {
        key: s.get("time_fast"),
        does: s
            .get("percent")
            .replace("{n}", &pct(sh.fast_ticks, sh.ticks).to_string()),
    });
    out.rows.push(Row {
        key: s.get("best_speed"),
        does: s
            .get("hud_speed")
            .replace("{speed}", &world::speed_text(p.top_speed)),
    });
    out
}

/// M4: the online screen: hosting (the room code, the invite link, the
/// seats and Start), joining (a box for the code), a guest's lobby, or a
/// match that has ended and why.
fn online_panel(g: &Game) -> Panel {
    let s = &g.strings;
    let mut p = panel("online");
    let leave = action(g, "screen.menu", "", "leave", false);
    let Some(v) = g.versus.as_ref() else {
        // Join: a box for the code, or the whole link
        p.heading = s.get("join_match");
        p.text = s.get("join_text");
        p.fields = vec![field(g, "room-in", "room_code", &g.invited, false)];
        p.actions = vec![
            Action {
                from: "room-in".into(),
                ..action(g, "online.room", "", "join", false)
            },
            action(g, "screen.menu", "", "back", false),
        ];
        return p;
    };
    if v.ended() {
        p.heading = s.get("match_ended");
        p.text = s.get(v.status());
        p.actions = vec![action(g, "screen.menu", "", "to_menu", false)];
        return p;
    }
    p.heading = s.get(if v.host { "host_match" } else { "join_match" });
    use crate::versus::Notice;
    let notice = v.notice(g.now_us);
    let fill = |key: &str, secs: i64| {
        s.get(key)
            .replace("{code}", &v.code)
            .replace("{n}", &secs.to_string())
    };
    p.text = match (v.host, v.seat(), notice) {
        (true, _, Some(Notice::Knocking)) => format!("{} {}", s.get("host_text"), s.get("online.knocking")),
        (true, _, _) => s.get("host_text"),
        (false, None, Some(Notice::StillLooking { secs })) => fill("online.still_looking", secs),
        (false, None, Some(Notice::Looking { secs })) => fill("joining_text", secs),
        (false, None, _) => fill("joining_text", 0),
        (false, Some(_), _) => s.get("online.lobby"),
    };
    p.fields = vec![field(g, "room-code", "room_code", &v.code, true)];
    // a copy button says Copied once its text is on the clipboard
    let copy = |id: &str, label: &str| Action {
        copies: id.into(),
        small: true,
        ..action(g, "", "", if g.copied == id { "copied" } else { label }, false)
    };
    let mut copies = vec![copy("room-code", "copy_code")];
    if !g.address.is_empty() {
        let link = crate::versus::invite_link(&g.address, &v.code);
        p.fields.push(field(g, "invite-link", "invite_link", &link, true));
        copies.push(copy("invite-link", "copy_link"));
    }
    p.actions = copies;
    // R10.12: a guest still looking can look again
    if matches!(notice, Some(Notice::StillLooking { .. })) {
        p.actions.push(action(g, "online.retry", "", "retry", false));
    }
    if v.host {
        // the host's seats, two to a row: its own and each guest's say who
        // holds them; each free one is a switch, a bot or empty (T10)
        for k in 0..crate::online::SEATS {
            let n = (k + 1).to_string();
            let held = k == 0 || crate::online::holds(v.seats(), k);
            let label = if k == 0 {
                "seat_you_n"
            } else if held {
                "seat_player_n"
            } else if crate::online::holds(v.empty(), k) {
                "seat_bot_off"
            } else {
                "seat_bot_on"
            };
            p.actions.push(Action {
                label: s.get(label).replace("{n}", &n),
                small: true,
                enabled: !held,
                ..action(g, "online.bot", &k.to_string(), "", false)
            });
        }
    }
    // M5: your skin, here as in the settings
    p.actions.push(Action {
        label: s.get("lobby_skin").replace("{name}", &skin_name(g, g.skin)),
        small: true,
        ..action(g, "skin.step", "1", "", false)
    });
    if v.host {
        p.actions.push(action(g, "online.start", "", "start_match", true));
    }
    p.actions.push(leave);
    // a guest's seats, once it knows them (the host's are its switches)
    if !v.host && v.seat().is_some() {
        p.rows_heading = s.get("seats");
        p.rows = (0..crate::online::SEATS)
            .map(|k| Row {
                key: s.get("seat_n").replace("{n}", &(k + 1).to_string()),
                does: s.get(if Some(k) == v.seat() {
                    "seat_you"
                } else if crate::online::holds(v.seats(), k) {
                    "seat_player"
                } else if crate::online::holds(v.empty(), k) {
                    "seat_empty"
                } else {
                    "seat_bot"
                }),
            })
            .collect();
    }
    p
}

fn panel(id: &str) -> Panel {
    Panel {
        id: id.into(),
        heading: String::new(),
        text: String::new(),
        settings: Vec::new(),
        actions: Vec::new(),
        rows_heading: String::new(),
        rows: Vec::new(),
        fields: Vec::new(),
    }
}

/// The controls, as the pause shows them.
pub const CONTROLS: &[(&str, &str)] = &[
    ("key_move", "control_move"),
    ("key_look", "control_look"),
    ("key_fire", "control_fire"),
    ("key_reload", "control_reload"),
    ("key_guns", "control_guns"),
    ("key_melee", "control_melee"),
    ("key_alt", "control_alt"),
    ("key_jump", "control_jump"),
    ("key_crouch", "control_crouch"),
    ("key_wall", "control_wall"),
    ("key_reset", "control_reset"),
    ("key_pause", "control_pause"),
];

fn controls(g: &Game) -> Vec<Row> {
    CONTROLS
        .iter()
        .map(|(k, d)| Row {
            key: g.strings.get(k),
            does: g.strings.get(d),
        })
        .collect()
}

pub fn settings(g: &Game) -> Vec<Setting> {
    let step = |name: &str, minus: &str, plus: &str| {
        vec![
            action(g, name, "-1", minus, false),
            action(g, name, "1", plus, false),
        ]
    };
    vec![
        Setting {
            id: "fov".into(),
            label: g.strings.get("setting_fov"),
            value: g.strings.get("degrees").replace("{n}", &g.fov.to_string()),
            actions: step("fov.step", "narrower", "wider"),
        },
        Setting {
            id: "skin".into(),
            label: g.strings.get("setting_skin"),
            value: skin_name(g, g.skin),
            actions: step("skin.step", "skin_prev", "skin_next"),
        },
        Setting {
            id: "volume".into(),
            label: g.strings.get("setting_volume"),
            value: g.strings.get("percent").replace("{n}", &g.volume_pct.to_string()),
            actions: step("volume.step", "quieter", "louder"),
        },
        Setting {
            id: "look".into(),
            label: g.strings.get("setting_look"),
            value: g.strings.get("percent").replace("{n}", &g.look_pct.to_string()),
            actions: step("look.step", "slower", "faster"),
        },
    ]
}

/// M5: a skin's name.
pub fn skin_name(g: &Game, skin: u8) -> String {
    g.strings.get(&format!("skin_{}", skin % 4))
}

/// M5: a seat as the HUD names it: "You", or its skin's name.
fn who(g: &Game, seat: usize) -> String {
    if seat == g.me() {
        g.strings.get("you")
    } else {
        skin_name(g, g.view_world().players[seat].skin)
    }
}

/// M5: the round's lines over play: the round and map, the scores, the
/// count, the clock, who took the round or the match, and whom you watch
/// while down; and from 75 s, where each other seat is, through walls.
fn round_lines(g: &Game, playing: bool) -> Vec<Hud> {
    use crate::rounds::Phase;
    let w = g.view_world();
    let Some(r) = w.rounds else { return Vec::new() };
    let s = &g.strings;
    let me = g.me();
    let line = |id: &str, text: String, shown: bool| Hud {
        id: id.into(),
        text,
        shown: shown && playing,
        at: None,
    };
    let scores = (0..w.players.len())
        .filter(|&k| !w.players[k].out)
        .map(|k| {
            s.get("hud_score")
                .replace("{who}", &who(g, k))
                .replace("{n}", &r.scores[k].to_string())
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let secs = r.ticks / 60;
    let ended = match (r.phase, r.winner) {
        (Phase::Over, Some(k)) if k == me => s.get("hud_round_you"),
        (Phase::Over, Some(k)) => s.get("hud_round_won").replace("{who}", &who(g, k)),
        (Phase::Over, None) => s.get("hud_round_none"),
        (Phase::Done, Some(k)) if k == me => s.get("hud_match_you"),
        (Phase::Done, Some(k)) => s.get("hud_match_won").replace("{who}", &who(g, k)),
        _ => String::new(),
    };
    let mut out = vec![
        line(
            "round",
            s.get("hud_round")
                .replace("{n}", &r.round.to_string())
                .replace("{map}", &s.get(w.map.name)),
            true,
        ),
        line("scores", scores, true),
        line("count", r.count_left().to_string(), r.phase == Phase::Count),
        line(
            "clock",
            s.get("hud_clock")
                .replace("{m}", &(secs / 60).to_string())
                .replace("{ss}", &format!("{:02}", secs % 60)),
            r.phase == Phase::Live,
        ),
        line("ended", ended.clone(), !ended.is_empty()),
        line(
            "watching",
            s.get("hud_watching").replace("{who}", &who(g, g.eyes())),
            r.phase == Phase::Live && g.eyes() != me,
        ),
    ];
    // from 75 s: each other seat still standing, where it is on the screen
    // (CS-Q18: C), whatever stands between
    let cam = g.camera();
    let m = cam.matrix(crate::render::Layer::Far, g.aspect);
    for k in 0..4 {
        let p = w.players.get(k);
        let spot = p
            .filter(|p| r.xray() && k != g.eyes() && p.alive())
            .map(|p| m.project(p.eye(g.alpha).sub(cam.eye)))
            .filter(|c| c[3] > 0)
            .map(|c| {
                let x = 5_000 + (c[0] as i64 * 5_000 / c[3] as i64) as i32;
                let y = 5_000 - (c[1] as i64 * 5_000 / c[3] as i64) as i32;
                [x, y]
            })
            .filter(|[x, y]| (0..=10_000).contains(x) && (0..=10_000).contains(y));
        out.push(Hud {
            id: format!("seen-{k}"),
            text: String::new(),
            shown: playing && spot.is_some(),
            at: spot,
        });
    }
    out
}

/// The Pincel's charge, percent of full.
fn charge_pct(a: &crate::arms::Arms) -> i32 {
    let full = a.weapon.stats().charge.max(1);
    a.charge * 100 / full
}

/// The lines over the play view: speed, wall jumps left, what the body is
/// doing, the best speed, the hint, and (M2) health, the weapon, its rounds,
/// a reload, the Pincel's charge, being down, and the hit and kill marks.
pub fn hud(g: &Game) -> Vec<Hud> {
    let playing = g.screen == Screen::Play;
    let p = &g.view_world().players[g.me()];
    let b = &p.body;
    // the katana in hand allows a fourth (M3)
    let left = (p.tuning().wall_jumps - b.wall_jumps).max(0);
    let doing = if b.sliding {
        "hud_sliding"
    } else if b.slam_from.is_some() {
        "hud_slamming"
    } else if !b.on_ground {
        "hud_air"
    } else {
        "hud_ground"
    };
    let line = |id: &str, text: String, shown: bool| Hud {
        id: id.into(),
        text,
        shown,
        at: None,
    };
    vec![
        line(
            "speed",
            g.strings
                .get("hud_speed")
                .replace("{speed}", &world::speed_text(b.speed())),
            playing,
        ),
        line(
            "walls",
            g.strings.get("hud_walls").replace("{n}", &left.to_string()),
            playing,
        ),
        line("doing", g.strings.get(doing), playing),
        line(
            "top",
            g.strings
                .get("hud_top")
                .replace("{speed}", &world::speed_text(p.top_speed)),
            playing,
        ),
        line(
            "hint",
            g.strings.get("hint_play"),
            playing && g.versus.is_none() && g.world.tick < 600,
        ),
        // R10.12: in a match, whose connection holds it up, or who left
        line(
            "connection",
            match g.versus.as_ref().and_then(|v| v.notice(g.now_us)) {
                Some(crate::versus::Notice::WaitingFor(k)) => {
                    g.strings.get("online.waiting_for").replace("{who}", &who(g, k))
                }
                Some(crate::versus::Notice::Dropped(k)) => {
                    g.strings.get("online.dropped").replace("{who}", &who(g, k))
                }
                _ => String::new(),
            },
            playing
                && matches!(
                    g.versus.as_ref().and_then(|v| v.notice(g.now_us)),
                    Some(crate::versus::Notice::WaitingFor(_) | crate::versus::Notice::Dropped(_))
                ),
        ),
        // M4: online, which seat is yours, for the match's first seconds
        line(
            "online",
            g.strings
                .get("hud_seat")
                .replace("{n}", &(g.me() + 1).to_string()),
            playing && g.in_match() && g.view_world().tick < 600,
        ),
        // M2: health, the gun and its rounds, reloading, the Pincel's charge
        line(
            "health",
            g.strings.get("hud_health").replace("{n}", &p.health.to_string()),
            playing,
        ),
        line("gun", g.strings.get(p.arms.weapon.stats().name), playing),
        line(
            "ammo",
            g.strings
                .get("hud_ammo")
                .replace("{n}", &p.arms.rounds[p.arms.weapon as usize].to_string())
                .replace("{of}", &p.arms.weapon.stats().magazine.to_string()),
            // a melee weapon has no rounds
            playing && p.arms.weapon.stats().magazine > 0,
        ),
        line(
            "reload",
            g.strings.get("hud_reloading"),
            playing && p.arms.reloading > 0,
        ),
        line(
            "charge",
            g.strings
                .get("hud_charge")
                .replace("{n}", &charge_pct(&p.arms).to_string()),
            playing && p.arms.charge > 0,
        ),
        line(
            "dead",
            g.strings.get("hud_dead"),
            playing && !p.alive() && g.view_world().rounds.is_none(),
        ),
        // the hit mark and the kill mark round the crosshair: drawn, not read
        line("mark", String::new(), playing && p.hit_mark > 0),
        line("kill", String::new(), playing && p.kill_mark > 0),
    ]
    .into_iter()
    .chain(round_lines(g, playing))
    .collect()
}

pub fn ui(g: &Game) -> Ui {
    let mut panels = Vec::new();
    match g.screen {
        Screen::Title => {
            let mut p = panel("title");
            p.heading = g.strings.get("title");
            p.text = g.strings.get("tagline");
            p.actions = vec![action(g, "title.begin", "", "begin", false)];
            panels.push(p);
        }
        Screen::Menu => {
            let mut p = panel("menu");
            p.heading = g.strings.get("title");
            p.text = g.strings.get("menu_text");
            p.actions = vec![
                action(g, "menu.practice", "", "practice", true),
                action(g, "menu.duel", "", "duel", true),
                action(g, "menu.bots", "", "bots_match", true),
                action(g, "online.host", "", "host_match", false),
                action(g, "online.join", "", "join_match", false),
                action(g, "screen.settings", "", "settings", false),
            ];
            panels.push(p);
        }
        Screen::Play => {}
        Screen::Paused => {
            let mut p = panel("paused");
            p.heading = g.strings.get("paused");
            p.actions = if g.versus.is_some() {
                // online: the match runs on behind the pause, and cannot be
                // started again
                vec![
                    action(g, "play.resume", "", "resume", true),
                    action(g, "screen.settings", "", "settings", false),
                    action(g, "screen.menu", "", "leave", false),
                ]
            } else {
                vec![
                    action(g, "play.resume", "", "resume", true),
                    action(g, "play.restart", "", "restart", true),
                    action(g, "screen.settings", "", "settings", false),
                    action(g, "screen.menu", "", "to_menu", false),
                ]
            };
            p.rows_heading = g.strings.get("controls");
            p.rows = controls(g);
            panels.push(p);
            // CS-Q22: A, the shooting card beside the pause
            panels.push(shooting_panel(g));
        }
        Screen::Settings => {
            let mut p = panel("settings");
            p.heading = g.strings.get("settings");
            p.settings = settings(g);
            p.actions = vec![action(g, "settings.back", "", "back", false)];
            panels.push(p);
        }
        Screen::Online => {
            panels.push(online_panel(g));
            // CS-Q22: A, and after an online match
            if g.versus
                .as_ref()
                .is_some_and(|v| v.ended() && v.session().is_some())
            {
                panels.push(shooting_panel(g));
            }
        }
    }
    Ui {
        title: g.strings.get("title"),
        screen: g.screen.name().into(),
        panels,
        hud: hud(g),
        capture: g.capture(),
        hints: g
            .glossary
            .iter()
            .map(|e| Hint {
                term: e.term.clone(),
                definition: e.definition.clone(),
            })
            .collect(),
        no_webgl: g.strings.get("no_webgl"),
        online: OnlineView {
            cmds: g.cmds.clone(),
            status: g.versus.as_ref().map(|v| v.status()).unwrap_or("").into(),
            sums: g
                .versus
                .as_ref()
                .and_then(|v| v.session())
                .map(|s| {
                    s.made
                        .iter()
                        .map(|(t, sum)| format!("{t}:{sum:x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default(),
        },
    }
}

/// A screen, setting or flag: the new view model, or the error.
pub fn act(g: &mut Game, name: &str, value: &str) -> String {
    match g.act(name, value) {
        Ok(()) => ok(&ui(g)),
        Err(e) => error(e),
    }
}

/// Sizes and layout the page needs to upload and draw: every stride, offset,
/// bit, pass state and uniform name, so the page holds no layout of its own.
pub fn numbers() -> String {
    let b = render::BYTES_PER_VALUE;
    let vec4 = 4 * b;
    serde_json::json!({
        "actions": input::ACTIONS.iter().map(|(n, bit)| (n.to_string(), serde_json::json!(bit))).collect::<serde_json::Map<_, _>>(),
        "vertex": {
            "stride": prim::VERTEX_VALUES * b,
            "attributes": [["aPos", 3, 0], ["aNormal", 3, 3 * b], ["aColor", 3, 6 * b]],
        },
        "instance": {
            "stride": render::INSTANCE_VALUES * b,
            "attributes": [["iA", 4, 0], ["iB", 4, vec4], ["iC", 4, 2 * vec4], ["iD", 4, 3 * vec4]],
        },
        "draw_values": render::DRAW_VALUES,
        "buffers": { "static": render::STATIC_BUFFER, "dynamic": render::DYNAMIC_BUFFER },
        "passes": render::pass_states(),
        "uniforms": look::UNIFORMS.iter().chain(crate::sky::UNIFORMS).map(|(n, call, len)| serde_json::json!([n, call, len, call.contains("Matrix")])).collect::<Vec<_>>(),
        "view": { "layers": [[0, 16], [16, 32], [32, 48]], "eye": [48, 51], "origin": [51, 54] },
    })
    .to_string()
}

/// Every mesh to upload, by number, with its vertex count.
pub fn meshes(g: &Game) -> String {
    let list: Vec<serde_json::Value> = (0..g.meshes.count() as u32)
        .map(|id| {
            let n = g.meshes.vertices(MeshId(id)).len() / prim::VERTEX_VALUES;
            serde_json::json!({ "id": id, "vertices": n })
        })
        .collect();
    serde_json::Value::Array(list).to_string()
}

pub fn mesh(g: &Game, id: u32) -> Vec<i32> {
    g.meshes.vertices(MeshId(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        Game::new(
            include_str!("../../../data/strings.json"),
            include_str!("../../../GLOSSARY.md"),
            include_str!("../../../data/look.json"),
            1,
        )
        .unwrap()
    }

    #[test]
    fn the_hud_reads_the_speed_and_the_wall_jumps_left_only_in_play() {
        let mut g = game();
        assert!(hud(&g).iter().all(|h| !h.shown));
        g.act("menu.practice", "").unwrap();
        g.world.players[0].body.velocity = crate::geom::v3(crate::fx::int(17), 0, crate::fx::int(3));
        g.world.players[0].body.wall_jumps = 1;
        let h = hud(&g);
        // √(17² + 3²) = 17.26
        assert_eq!(h[0].text, "17.3 m/s");
        assert_eq!(h[1].text, "Wall jumps: 2");
        assert!(h[0].shown && h[1].shown);
    }

    #[test]
    fn the_hud_names_the_gun_and_counts_its_rounds() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        g.world.players[0].arms.rounds[0] = 3;
        let h = hud(&g);
        let line = |id: &str| h.iter().find(|l| l.id == id).unwrap().clone();
        assert_eq!(line("gun").text, "Service pistol");
        assert_eq!(line("ammo").text, "3 / 8");
        assert!(!line("reload").shown && !line("mark").shown);
        g.world.players[0].hit_mark = 4;
        assert!(hud(&g).iter().find(|l| l.id == "mark").unwrap().shown);
    }

    #[test]
    fn with_a_katana_the_hud_hides_the_rounds_and_counts_four_wall_jumps() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        g.world.players[0].arms.weapon = crate::weapons::Weapon::Katana;
        let h = hud(&g);
        let line = |id: &str| h.iter().find(|l| l.id == id).unwrap().clone();
        assert_eq!(line("gun").text, "Katana");
        assert!(!line("ammo").shown, "a melee weapon has no rounds");
        assert_eq!(line("walls").text, "Wall jumps: 4");
    }

    #[test]
    fn act_answers_with_the_view_model_or_the_error() {
        let mut g = game();
        let v: serde_json::Value = serde_json::from_str(&act(&mut g, "title.begin", "")).unwrap();
        assert_eq!(v["ok"]["screen"], "menu");
        let e: serde_json::Value = serde_json::from_str(&act(&mut g, "nonsense", "")).unwrap();
        assert_eq!(e["error"], "Unknown action \"nonsense\".");
    }

    #[test]
    fn the_pause_lists_every_control_and_lets_go_of_the_pointer() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        g.act("key.escape", "").unwrap();
        let u = ui(&g);
        assert_eq!(u.panels[0].rows.len(), CONTROLS.len());
        assert!(!u.capture);
    }
}
