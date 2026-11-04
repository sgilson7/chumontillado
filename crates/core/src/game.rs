//! The game: which screen is showing, the reader's settings, the clock,
//! and the frame the page draws next.
//!
//! M1 has a title, a menu, the practice room, a pause and the settings. The
//! world runs at a fixed 60 ticks a second (`ticks::Clock`); the page's
//! frames come as often as the display allows, and each draws the world
//! between its last two ticks with the mouse movement not yet given to a
//! tick already turned (`world::World::camera`), as Chumworld Arena does.

use crate::camera::{self, Aim};
use crate::fx::{self, Fx};
use crate::geom::v3;
use crate::input::{Input, Mouse, Pending};
use crate::look::{rgb, Look};
use crate::map::Surface;
use crate::page::{self, Strings};
use crate::prim::MeshTable;
use crate::render::{self, Frame, Group, Scene, Shapes};
use crate::ticks::Clock;
use crate::world::{Bot, World};

/// Software drawing (a CPU in place of a graphics card) draws a share of the
/// pixels, percent.
const SOFTWARE_SCALE_PCT: i32 = 50;
/// The most pixels a frame draws (a 4K screen is drawn at about 1440p).
const MAX_PIXELS: i64 = 2560 * 1440;
/// The title's slow turn round the room, angle units a tick.
const TITLE_DRIFT: i32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    Menu,
    Play,
    Paused,
    Settings,
}

impl Screen {
    pub fn name(self) -> &'static str {
        match self {
            Screen::Title => "title",
            Screen::Menu => "menu",
            Screen::Play => "play",
            Screen::Paused => "paused",
            Screen::Settings => "settings",
        }
    }
}

pub struct Game {
    pub strings: Strings,
    pub glossary: Vec<crate::glossary::Entry>,
    pub look: Look,
    pub meshes: MeshTable,
    pub world: World,
    /// the room's bot: moving only (Practice room) or shooting (Duel)
    pub bot: Bot,
    pub screen: Screen,
    /// the screen the settings return to
    pub settings_from: Screen,
    pub clock: Clock,
    pub pending: Pending,
    pub alpha: Fx,
    pub title_ticks: u64,
    /// the reader's look speed, percent, and field of view, degrees
    pub look_pct: i32,
    pub fov: i32,
    pub software: bool,
    pub css: (i32, i32),
    pub dpr_pct: i32,
    pub pixels: (i32, i32),
    pub aspect: Fx,
    groups: Vec<Group>,
    pub scene: Scene,
    pub frame: Frame,
    /// changes whenever the view model does
    pub ui_version: u32,
    /// the HUD's last text, so the view model changes only when it does
    hud_seen: String,
}

impl Game {
    pub fn new(strings: &str, glossary: &str, look: &str, _seed: u32) -> Result<Game, String> {
        let strings = page::parse_strings(strings).map_err(|e| e.message())?;
        let look = Look::parse(look)?;
        let world = World::practice();
        let groups = Self::map_groups(&world, &look);
        let mut g = Game {
            strings,
            glossary: crate::glossary::load(glossary),
            look,
            meshes: MeshTable::default(),
            world,
            bot: Bot::Moves,
            screen: Screen::Title,
            settings_from: Screen::Menu,
            clock: Clock::default(),
            pending: Pending::default(),
            alpha: 0,
            title_ticks: 0,
            look_pct: 100,
            fov: camera::FOV_START,
            software: false,
            css: (1280, 720),
            dpr_pct: 100,
            pixels: (1280, 720),
            aspect: fx::ratio(16, 9),
            groups,
            scene: Scene::default(),
            frame: Frame::default(),
            ui_version: 1,
            hud_seen: String::new(),
        };
        g.draw();
        Ok(g)
    }

    fn map_groups(world: &World, look: &Look) -> Vec<Group> {
        let p = &look.palette;
        world.map.groups(
            |s| {
                rgb(match s {
                    Surface::Floor => p.floor,
                    Surface::Wall => p.wall,
                    Surface::Ramp => p.ramp,
                    Surface::Slope => p.slope,
                    Surface::Block => p.block,
                    Surface::Trim => p.trim,
                })
            },
            look.ink(),
        )
    }

    /// The CSS size and the device pixel ratio (percent) in; the drawing
    /// buffer's size out.
    pub fn resize(&mut self, css_w: i32, css_h: i32, dpr_pct: i32) -> [i32; 2] {
        self.css = (css_w.max(1), css_h.max(1));
        self.dpr_pct = dpr_pct.clamp(25, 400);
        let scale = if self.software {
            self.dpr_pct * SOFTWARE_SCALE_PCT / 100
        } else {
            self.dpr_pct
        };
        let mut w = self.css.0 as i64 * scale as i64 / 100;
        let mut h = self.css.1 as i64 * scale as i64 / 100;
        if w * h > MAX_PIXELS {
            let k = fx::isqrt_u64((MAX_PIXELS * 1_000_000 / (w * h)) as u64) as i64;
            w = w * k / 1000;
            h = h * k / 1000;
        }
        self.pixels = (w.max(1) as i32, h.max(1) as i32);
        self.aspect = fx::ratio(self.css.0, self.css.1);
        self.draw();
        [self.pixels.0, self.pixels.1]
    }

    /// One display frame: the page's clock in microseconds, the held action
    /// bits, and the mouse and wheel movement in hundredths of a CSS pixel.
    pub fn advance(&mut self, now_us: i64, bits: u32, dx: i32, dy: i32, wheel: i32) {
        let ticks = self.clock.frame(now_us, bits);
        if self.screen == Screen::Play {
            self.pending.add(dx, dy, wheel);
            for (k, b) in ticks.iter().enumerate() {
                let m = if k == 0 {
                    self.pending.take()
                } else {
                    Mouse::default()
                };
                self.world.step(&[Input::from_page(*b, m)]);
            }
            self.alpha = self.clock.alpha();
            self.follow_hud();
        } else if self.screen == Screen::Paused
            || self.settings_from == Screen::Paused && self.screen == Screen::Settings
        {
            // paused: the world holds still and time passing is forgotten
            self.pending.clear();
            self.clock.rest();
        } else {
            self.pending.clear();
            self.title_ticks += ticks.len() as u64;
            self.alpha = self.clock.alpha();
        }
        self.draw();
    }

    /// The HUD's text changes: the view model does too.
    fn follow_hud(&mut self) {
        let now = serde_json::to_string(&crate::api::hud(self)).unwrap_or_default();
        if now != self.hud_seen {
            self.hud_seen = now;
            self.ui_version += 1;
        }
    }

    pub fn camera(&self) -> render::Camera {
        if matches!(self.screen, Screen::Title | Screen::Menu)
            || self.screen == Screen::Settings && self.settings_from == Screen::Menu
        {
            // the title and the menu look slowly round the room from above
            // its north-east corner
            let drift = (self.title_ticks as i64 * TITLE_DRIFT as i64
                + fx::mul(self.alpha, TITLE_DRIFT) as i64) as i32;
            let aim = Aim {
                yaw: fx::wrap(fx::deg(150).wrapping_add(drift)),
                pitch: -fx::deg(24),
                rest: (0, 0),
                pct: 100,
            };
            let fwd = crate::geom::facing(aim.yaw, aim.pitch);
            let eye = v3(0, fx::int(16), 0).sub(fwd.scale(fx::int(30)));
            return camera::eye_view(eye, aim, camera::vertical_fov(self.fov, self.aspect));
        }
        self.world.camera(0, self.alpha, self.pending.peek(), self.aspect)
    }

    /// Builds the frame the page draws next.
    pub fn draw(&mut self) {
        self.world.fov = self.fov;
        let cam = self.camera();
        let origin = render::origin_for(cam.eye);
        if origin != self.scene.origin || self.scene.version == 0 {
            self.scene = Scene::build(&self.groups, origin, self.scene.version + 1);
        }
        let mut moving = Shapes::default();
        moving.sky(cam.eye);
        let first_person = matches!(self.screen, Screen::Play | Screen::Paused)
            || self.screen == Screen::Settings && self.settings_from == Screen::Paused;
        let pending = if first_person {
            self.pending.peek()
        } else {
            Mouse::default()
        };
        self.world
            .draw(0, first_person, self.alpha, pending, &self.look, &mut moving);
        crate::render::keep_visible(&mut moving.list, cam.eye);
        self.frame = Frame::build(&self.scene, &moving, &cam, self.aspect);
    }

    pub(crate) fn show(&mut self, s: Screen) {
        if self.screen != s {
            self.screen = s;
            self.pending.clear();
            self.ui_version += 1;
            self.draw();
        }
    }

    /// The practice room from its start, its bot moving only or shooting.
    fn practice(&mut self, bot: Bot) {
        self.bot = bot;
        self.world = World::room(bot);
        for p in &mut self.world.players {
            p.aim.pct = self.look_pct;
        }
        self.clock.rest();
        self.show(Screen::Play);
    }

    /// A screen, setting or flag by name (`api::act` is the page's door).
    pub fn act(&mut self, name: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        match name {
            "title.begin" => self.show(Screen::Menu),
            "menu.practice" => self.practice(Bot::Moves),
            "menu.duel" => self.practice(Bot::Shoots),
            "play.resume" => self.show(Screen::Play),
            "play.restart" => self.practice(self.bot),
            "screen.menu" => self.show(Screen::Menu),
            "screen.settings" => {
                if self.screen != Screen::Settings {
                    self.settings_from = self.screen;
                }
                self.show(Screen::Settings);
            }
            "settings.back" => self.show(self.settings_from),
            // Esc, a lost pointer or a hidden tab pause the room; Esc again
            // goes back
            "key.escape" => match self.screen {
                Screen::Play => self.show(Screen::Paused),
                Screen::Paused => self.show(Screen::Play),
                Screen::Settings => self.show(self.settings_from),
                _ => {}
            },
            "pointer.lost" | "page.hidden" => {
                if self.screen == Screen::Play {
                    self.show(Screen::Paused);
                }
            }
            "look.speed" => {
                let pct: i32 = v
                    .parse()
                    .map_err(|_| format!("The look speed \"{value}\" is not a whole percent."))?;
                self.look_pct = pct.clamp(camera::LOOK_PCT_MIN, camera::LOOK_PCT_MAX);
                for p in &mut self.world.players {
                    p.aim.pct = self.look_pct;
                }
                self.ui_version += 1;
            }
            "look.step" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                let pct = (self.look_pct + step * 10).clamp(camera::LOOK_PCT_MIN, camera::LOOK_PCT_MAX);
                return self.act("look.speed", &pct.to_string());
            }
            "fov.step" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.fov = (self.fov + step * 5).clamp(camera::FOV_MIN, camera::FOV_MAX);
                self.ui_version += 1;
                self.draw();
            }
            // for tests and the journal's shots: stand the player at x y z
            // (centimetres) facing a heading (degrees), still
            "place.at" => {
                let n: Vec<i32> = v
                    .split_whitespace()
                    .map(|t| t.parse())
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("\"{value}\" is not four whole numbers."))?;
                let [x, y, z, yaw] = n[..] else {
                    return Err(format!("\"{value}\" is not four whole numbers."));
                };
                let p = &mut self.world.players[0];
                p.body =
                    crate::movement::Body::at(v3(x * fx::ONE / 100, y * fx::ONE / 100, z * fx::ONE / 100));
                p.prev = p.body.origin;
                p.aim.yaw = fx::deg(yaw);
                p.aim.pitch = 0;
                self.draw();
            }
            "display.software" => {
                self.software = v == "true";
                let (w, h, d) = (self.css.0, self.css.1, self.dpr_pct);
                self.resize(w, h, d);
            }
            _ => return Err(format!("Unknown action \"{name}\".")),
        }
        Ok(())
    }

    /// The speed shown on the HUD (m/s, Q16.16).
    pub fn speed(&self) -> Fx {
        self.world.players[0].body.speed()
    }

    /// Whether the page should hold the pointer.
    pub fn capture(&self) -> bool {
        self.screen == Screen::Play
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn game() -> Game {
        Game::new(
            include_str!("../../../data/strings.json"),
            include_str!("../../../GLOSSARY.md"),
            include_str!("../../../data/look.json"),
            1,
        )
        .expect("the shipped data")
    }

    #[test]
    fn the_title_leads_to_the_menu_and_the_menu_to_the_room() {
        let mut g = game();
        assert_eq!(g.screen, Screen::Title);
        g.act("title.begin", "").unwrap();
        assert_eq!(g.screen, Screen::Menu);
        g.act("menu.practice", "").unwrap();
        assert_eq!(g.screen, Screen::Play);
        assert!(g.capture());
    }

    #[test]
    fn escape_pauses_the_room_and_the_world_holds_still_while_paused() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        // hold forward for a second
        let fwd = crate::input::FORWARD;
        for k in 0..60 {
            g.advance(k * 16_667, fwd, 0, 0, 0);
        }
        g.act("key.escape", "").unwrap();
        assert_eq!(g.screen, Screen::Paused);
        let at = g.world.players[0].body.origin;
        for k in 60..240 {
            g.advance(k * 16_667, fwd, 0, 0, 0);
        }
        assert_eq!(g.world.players[0].body.origin, at);
        g.act("key.escape", "").unwrap();
        assert_eq!(g.screen, Screen::Play);
    }

    #[test]
    fn the_field_of_view_steps_by_five_inside_ninety_to_a_hundred_and_twenty() {
        let mut g = game();
        assert_eq!(g.fov, 105);
        g.act("fov.step", "1").unwrap();
        assert_eq!(g.fov, 110);
        for _ in 0..10 {
            g.act("fov.step", "1").unwrap();
        }
        assert_eq!(g.fov, 120);
        for _ in 0..10 {
            g.act("fov.step", "-1").unwrap();
        }
        assert_eq!(g.fov, 90);
    }

    #[test]
    fn place_at_stands_the_player_where_it_is_told_in_centimetres() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        g.act("place.at", "-1800 1000 2500 90").unwrap();
        let p = &g.world.players[0];
        assert_eq!(p.body.origin, v3(-fx::int(18), fx::int(10), fx::int(25)));
        assert_eq!(p.aim.yaw, fx::deg(90));
        assert!(g.act("place.at", "1 2 3").is_err());
    }

    #[test]
    fn an_unknown_action_is_refused_by_name() {
        let mut g = game();
        assert_eq!(g.act("dance", ""), Err("Unknown action \"dance\".".into()));
    }

    #[test]
    fn the_mouse_turns_the_view_on_the_frame_it_moves_before_any_tick_takes_it() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        // the clock's first frame runs one tick
        g.advance(0, 0, 0, 0, 0);
        let (before, ticks) = (g.camera().fwd, g.world.tick);
        // a frame too soon for a tick: the movement waits in `pending`
        g.advance(1_000, 0, 5_000, 0, 0);
        assert_eq!(g.world.tick, ticks);
        assert_ne!(g.camera().fwd, before, "the view turned before the tick");
    }
}
