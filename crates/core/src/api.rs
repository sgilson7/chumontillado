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
}

fn action(g: &Game, name: &str, value: &str, label_key: &str, engage: bool) -> Action {
    Action {
        name: name.into(),
        value: value.into(),
        label: g.strings.get(label_key),
        enabled: true,
        engage,
    }
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
    }
}

/// The controls, as the pause shows them.
pub const CONTROLS: &[(&str, &str)] = &[
    ("key_move", "control_move"),
    ("key_look", "control_look"),
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

fn settings(g: &Game) -> Vec<Setting> {
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
            id: "look".into(),
            label: g.strings.get("setting_look"),
            value: g.strings.get("percent").replace("{n}", &g.look_pct.to_string()),
            actions: step("look.step", "slower", "faster"),
        },
    ]
}

/// The lines over the play view: speed, wall jumps left, what the body is
/// doing, the best speed, and the hint.
pub fn hud(g: &Game) -> Vec<Hud> {
    let playing = g.screen == Screen::Play;
    let p = &g.world.players[0];
    let b = &p.body;
    let left = crate::movement::TUNING.wall_jumps - b.wall_jumps;
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
        line("hint", g.strings.get("hint_play"), playing && g.world.tick < 600),
    ]
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
                action(g, "screen.settings", "", "settings", false),
            ];
            panels.push(p);
        }
        Screen::Play => {}
        Screen::Paused => {
            let mut p = panel("paused");
            p.heading = g.strings.get("paused");
            p.actions = vec![
                action(g, "play.resume", "", "resume", true),
                action(g, "play.restart", "", "restart", true),
                action(g, "screen.settings", "", "settings", false),
                action(g, "screen.menu", "", "to_menu", false),
            ];
            p.rows_heading = g.strings.get("controls");
            p.rows = controls(g);
            panels.push(p);
        }
        Screen::Settings => {
            let mut p = panel("settings");
            p.heading = g.strings.get("settings");
            p.settings = settings(g);
            p.actions = vec![action(g, "settings.back", "", "back", false)];
            panels.push(p);
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
        "view": { "layers": [[0, 16], [16, 32], [32, 48]], "eye": [48, 51] },
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
