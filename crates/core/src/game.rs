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
use crate::versus::{self, Cmd, Versus};
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
    /// M4: hosting or joining, the lobby, or a match that has ended
    Online,
}

impl Screen {
    pub fn name(self) -> &'static str {
        match self {
            Screen::Title => "title",
            Screen::Menu => "menu",
            Screen::Play => "play",
            Screen::Paused => "paused",
            Screen::Settings => "settings",
            Screen::Online => "online",
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
    /// M4: the online match or lobby, if any
    pub versus: Option<Versus>,
    /// the commands given to the page's transport, numbered from 1
    pub cmds: Vec<Cmd>,
    /// this build's name for the online hello, and this page's address for
    /// the invite link (the page says both at start)
    pub build: String,
    pub address: String,
    /// a room code the reader is typing toward, or that a link brought
    pub invited: String,
    /// the seed the page drew, for room codes
    seed: u32,
    /// the page's clock at the last frame (µs)
    pub now_us: i64,
    /// the last words of a match just left, still to send
    outbox_left: Vec<(u32, String)>,
    /// the field just copied to the clipboard, whose button says so
    pub copied: String,
}

impl Game {
    pub fn new(strings: &str, glossary: &str, look: &str, seed: u32) -> Result<Game, String> {
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
            versus: None,
            cmds: Vec::new(),
            build: "dev".into(),
            address: String::new(),
            invited: String::new(),
            seed,
            now_us: 0,
            outbox_left: Vec::new(),
            copied: String::new(),
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
        self.now_us = now_us;
        let ticks = self.clock.frame(now_us, bits);
        if self.in_match() {
            self.advance_match(&ticks, dx, dy, wheel);
            self.draw();
            return;
        }
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

    /// M4: a frame of an online match. The match runs on whatever screen
    /// shows (it cannot pause); only in play does the reader's input go in.
    /// The look speed is applied to the mouse here, before the record is
    /// made, so the world every browser runs turns each seat at 100%.
    fn advance_match(&mut self, ticks: &[u32], dx: i32, dy: i32, wheel: i32) {
        let playing = self.screen == Screen::Play;
        if playing {
            self.pending
                .add(dx * self.look_pct / 100, dy * self.look_pct / 100, wheel);
        } else {
            self.pending.clear();
        }
        let now = self.now_us;
        let Some(v) = self.versus.as_mut() else { return };
        v.check_time(now);
        for (k, b) in ticks.iter().enumerate() {
            let m = if k == 0 && playing {
                self.pending.take()
            } else {
                Mouse::default()
            };
            let bits = if playing { *b } else { 0 };
            if let Some(s) = v.session_mut() {
                s.local(Input::from_page(bits, m));
                s.step();
            }
        }
        self.alpha = self.clock.alpha();
        if v.ended() {
            self.show(Screen::Online);
        }
        self.follow_hud();
    }

    /// An online match is under way (not its lobby, and not ended).
    pub fn in_match(&self) -> bool {
        self.versus
            .as_ref()
            .is_some_and(|v| v.session().is_some() && !v.ended())
    }

    /// The world the view shows: the match's, or the room's.
    pub fn view_world(&self) -> &World {
        match self.versus.as_ref().and_then(|v| v.session()) {
            Some(s) => &s.world,
            None => &self.world,
        }
    }

    /// The seat the view is from.
    pub fn me(&self) -> usize {
        match self.versus.as_ref().and_then(|v| v.session()) {
            Some(s) => s.seat,
            None => 0,
        }
    }

    /// The HUD's text changes: the view model does too.
    fn follow_hud(&mut self) {
        // (and, online, the checksums the online check reads)
        let sums = self
            .versus
            .as_ref()
            .and_then(|v| v.session())
            .and_then(|s| s.made.back().copied());
        let now = serde_json::to_string(&crate::api::hud(self)).unwrap_or_default() + &format!("{sums:?}");
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
        self.view_world()
            .camera(self.me(), self.alpha, self.pending.peek(), self.aspect)
    }

    /// Builds the frame the page draws next.
    pub fn draw(&mut self) {
        self.world.fov = self.fov;
        if let Some(s) = self.versus.as_mut().and_then(|v| v.session_mut()) {
            s.world.fov = self.fov;
        }
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
        self.view_world().draw(
            self.me(),
            first_person,
            self.alpha,
            pending,
            &self.look,
            &mut moving,
        );
        crate::render::keep_visible(&mut moving.list, cam.eye);
        self.frame = Frame::build(&self.scene, &moving, &cam, self.aspect);
    }

    pub(crate) fn show(&mut self, s: Screen) {
        if self.screen != s {
            self.copied.clear();
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

    /// M4: a room to host or join: the transport is told to open it.
    fn open_room(&mut self, host: bool, code: &str) {
        self.versus = Some(Versus::new(host, code, &self.build));
        self.invited = code.to_string();
        self.command("open", host, &crate::online::room_name(code));
        self.show(Screen::Online);
        self.ui_version += 1;
    }

    fn command(&mut self, act: &str, host: bool, room: &str) {
        let id = self.cmds.len() as u32 + 1;
        self.cmds.push(Cmd {
            id,
            act: act.into(),
            host,
            mode: "room".into(),
            room: room.into(),
            code: String::new(),
        });
    }

    /// Leaves an online lobby or match, if one is open: the others are told
    /// and the transport closes.
    pub fn leave_online(&mut self) {
        if let Some(mut v) = self.versus.take() {
            v.leave();
            // the bye goes out before the close
            self.outbox_left = v.take_outbox();
            self.command("close", false, "");
            self.ui_version += 1;
        }
    }

    /// The transport says a browser arrived.
    pub fn online_connected(&mut self, peer: u32) {
        if let Some(v) = self.versus.as_mut() {
            v.connected(peer);
            self.ui_version += 1;
        }
    }

    /// The transport says a browser left.
    pub fn online_left(&mut self, peer: u32) {
        if let Some(v) = self.versus.as_mut() {
            v.left(peer);
            self.ui_version += 1;
            if v.ended() {
                self.show(Screen::Online);
            }
        }
    }

    /// Text from a browser.
    pub fn online_message(&mut self, peer: u32, text: &str) {
        let now = self.now_us;
        let Some(v) = self.versus.as_mut() else { return };
        let lobby = v.session().is_none();
        let began = v.message(peer, text, now);
        if lobby || v.ended() {
            self.ui_version += 1;
        }
        if began {
            self.clock.rest();
            self.show(Screen::Play);
        }
        if self.versus.as_ref().is_some_and(|v| v.ended()) {
            self.show(Screen::Online);
        }
    }

    /// What the transport should send, as JSON `[[peer, text], ...]`.
    pub fn online_outbox(&mut self) -> String {
        let mut out = std::mem::take(&mut self.outbox_left);
        if let Some(v) = self.versus.as_mut() {
            out.extend(v.take_outbox());
        }
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
    }

    /// A screen, setting or flag by name (`api::act` is the page's door).
    pub fn act(&mut self, name: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        match name {
            "title.begin" => self.show(Screen::Menu),
            "menu.practice" => self.practice(Bot::Moves),
            "menu.duel" => self.practice(Bot::Shoots),
            "play.resume" => self.show(Screen::Play),
            "play.restart" => {
                if self.versus.is_none() {
                    self.practice(self.bot)
                }
            }
            "screen.menu" => {
                self.leave_online();
                self.show(Screen::Menu)
            }
            // M4: online play
            "online.build" => self.build = v.to_string(),
            "online.address" => self.address = v.to_string(),
            "online.host" => {
                self.leave_online();
                let code = versus::make_code(self.seed ^ (self.now_us as u32));
                self.open_room(true, &code);
            }
            "online.join" => {
                self.leave_online();
                self.invited.clear();
                self.show(Screen::Online);
                self.ui_version += 1;
            }
            // a typed code, a pasted code or link, or the page opened from
            // an invite link (`?join=CODE`): straight into the room
            "online.room" | "online.link" => {
                let code = versus::read_code(v).ok_or_else(|| self.strings.get("online.bad_room"))?;
                self.leave_online();
                self.open_room(false, &code);
            }
            // the host switches a free seat's bot on or off (the seat's
            // index, 1 to 3)
            "online.bot" => {
                let seat: usize = v.parse().map_err(|_| format!("\"{value}\" is not a seat."))?;
                let changed = self.versus.as_mut().is_some_and(|x| x.toggle_bot(seat));
                if !changed {
                    return Err(self.strings.get("online.not_host"));
                }
                self.ui_version += 1;
            }
            "online.start" => {
                let started = self.versus.as_mut().is_some_and(|x| x.start());
                if !started {
                    return Err(self.strings.get("online.not_host"));
                }
                self.clock.rest();
                self.show(Screen::Play);
                self.ui_version += 1;
            }
            "online.error" => {
                let key = versus::TRANSPORT_ERRORS
                    .iter()
                    .copied()
                    .find(|k| *k == v)
                    .unwrap_or("online.no_relay");
                let Some(x) = self.versus.as_mut() else {
                    return Ok(());
                };
                // one pair that cannot link is carried by the host (CS-Q6:
                // A); only a guest that cannot reach the host has lost
                let fatal = key != "online.no_path" || !x.host && x.links().host_peer().is_none();
                if !fatal {
                    return Ok(());
                }
                x.error = Some(key);
                self.show(Screen::Online);
                self.ui_version += 1;
            }
            "page.leaving" => self.leave_online(),
            "online.copied" => {
                self.copied = v.to_string();
                self.ui_version += 1;
            }
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
                Screen::Online if self.versus.is_none() => self.show(Screen::Menu),
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
            // for tests and the journal's shots: the four skins standing in
            // a row 5 m in front of the spawn, each holding a different
            // weapon (seat 4 wears seat 1's skin, the Hidalgo's)
            "place.lineup" => {
                let w = &mut self.world;
                let spawn = w.players[0].spawn;
                w.players.truncate(1);
                let held = [
                    crate::weapons::Weapon::Rifle,
                    crate::weapons::Weapon::Toro,
                    crate::weapons::Weapon::Katana,
                    crate::weapons::Weapon::Shotgun,
                ];
                for (k, g) in held.iter().enumerate() {
                    let at = spawn.0.add(v3(fx::int(k as i32 * 2 - 3), 0, -fx::int(5)));
                    let mut p = crate::world::Player::at((at, fx::HALF_TURN));
                    p.arms.weapon = *g;
                    w.players.push(p);
                }
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
        self.view_world().players[self.me()].body.speed()
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
    // ---- M4: online ------------------------------------------------------

    #[test]
    fn hosting_shows_the_room_code_and_an_invite_link_and_tells_the_transport_to_open_the_room() {
        let mut g = game();
        g.act("online.address", "https://sgilson7.github.io/chumontillado/")
            .unwrap();
        g.act("online.host", "").unwrap();
        assert_eq!(g.screen, Screen::Online);
        let code = g.versus.as_ref().unwrap().code.clone();
        assert_eq!(crate::versus::read_code(&code), Some(code.clone()));
        let ui = crate::api::ui(&g);
        let fields: Vec<(String, String)> = ui.panels[0]
            .fields
            .iter()
            .map(|f| (f.id.clone(), f.value.clone()))
            .collect();
        assert_eq!(
            fields,
            vec![
                ("room-code".into(), code.clone()),
                (
                    "invite-link".into(),
                    format!("https://sgilson7.github.io/chumontillado/?join={code}")
                ),
            ]
        );
        let cmd = g.cmds.last().unwrap();
        assert_eq!(
            (cmd.act.as_str(), cmd.host, cmd.room.clone()),
            ("open", true, format!("chumontillado-p1-{code}"))
        );
        // the host's seats: itself, and three bots until guests come
        let seats: Vec<(String, bool)> = ui.panels[0]
            .actions
            .iter()
            .filter(|a| a.name == "online.bot")
            .map(|a| (a.label.clone(), a.enabled))
            .collect();
        assert_eq!(seats[0], ("Seat 1: you".to_string(), false));
        assert_eq!(seats[3], ("Seat 4: bot on".to_string(), true));
    }

    #[test]
    fn an_invite_link_joins_its_room_at_once_and_a_bad_code_is_refused_in_words() {
        let mut g = game();
        g.act("online.link", "kqxrt").unwrap();
        let v = g.versus.as_ref().unwrap();
        assert_eq!((v.host, v.code.as_str()), (false, "KQXRT"));
        assert_eq!(g.cmds.last().unwrap().room, "chumontillado-p1-KQXRT");
        assert_eq!(g.screen, Screen::Online);
        let mut g = game();
        assert_eq!(
            g.act("online.room", "KQXRO"),
            Err(g.strings.get("online.bad_room"))
        );
        assert!(g.versus.is_none());
    }

    #[test]
    fn leaving_tells_the_others_and_closes_the_transport() {
        let mut g = game();
        g.act("online.host", "").unwrap();
        g.online_connected(7);
        let _ = g.online_outbox();
        g.act("screen.menu", "").unwrap();
        assert!(g.versus.is_none());
        assert_eq!(g.cmds.last().unwrap().act, "close");
        let out = g.online_outbox();
        assert!(out.contains("bye") || out == "[]", "{out}");
        assert_eq!(g.screen, Screen::Menu);
    }

    #[test]
    fn a_pair_that_cannot_link_ends_nothing_for_the_host_but_a_guest_without_the_host_has_lost() {
        let mut g = game();
        g.act("online.host", "").unwrap();
        g.act("online.error", "online.no_path").unwrap();
        assert!(!g.versus.as_ref().unwrap().ended(), "the host carries on");
        let mut g = game();
        g.act("online.link", "KQXRT").unwrap();
        g.act("online.error", "online.no_path").unwrap();
        assert_eq!(g.versus.as_ref().unwrap().status(), "online.no_path");
        // and no relay at all ends a host's room too
        let mut g = game();
        g.act("online.host", "").unwrap();
        g.act("online.error", "online.no_relay").unwrap();
        assert!(g.versus.as_ref().unwrap().ended());
    }

    #[test]
    fn online_the_look_speed_scales_the_mouse_before_the_record_so_every_browser_turns_alike() {
        // a match of one person and three bots, started by its host
        let mut g = game();
        g.act("look.speed", "200").unwrap();
        g.act("online.host", "").unwrap();
        g.act("online.start", "").unwrap();
        assert_eq!(g.screen, Screen::Play);
        let s = g.versus.as_ref().unwrap().session().unwrap();
        assert_eq!(
            s.world.players[0].aim.pct, 100,
            "the world turns every seat at 100%"
        );
        // 1 000 hundredths of a pixel at 200% is the record's 2 000; a
        // record is used two ticks after it is made, so five frames
        let frames = |g: &mut Game, dx: i32| {
            for k in 0..5 {
                g.advance(k * 16_667, 0, if k == 0 { dx } else { 0 }, 0, 0);
            }
            g.view_world().players[0].aim.yaw
        };
        let yaw_once = frames(&mut g, 1_000);
        let mut h = game();
        h.act("online.host", "").unwrap();
        h.act("online.start", "").unwrap();
        assert_eq!(frames(&mut h, 2_000), yaw_once);
        assert_ne!(yaw_once, 0);
    }

    #[test]
    fn the_hosts_lobby_has_a_switch_for_each_free_seat_and_it_flips_the_seat_to_empty() {
        let mut g = game();
        g.act("online.host", "").unwrap();
        let switches = |g: &Game| -> Vec<String> {
            crate::api::ui(g).panels[0]
                .actions
                .iter()
                .filter(|a| a.name == "online.bot" && a.enabled)
                .map(|a| a.label.clone())
                .collect()
        };
        assert_eq!(
            switches(&g),
            vec!["Seat 2: bot on", "Seat 3: bot on", "Seat 4: bot on"]
        );
        g.act("online.bot", "2").unwrap();
        assert_eq!(switches(&g)[1], "Seat 3: bot off");
        assert!(g.act("online.bot", "0").is_err(), "the host's own seat");
        g.act("online.start", "").unwrap();
        let w = &g.versus.as_ref().unwrap().session().unwrap().world;
        assert!(w.players[2].out && !w.players[1].out);
    }

    #[test]
    fn the_pause_shows_your_shooting_by_speed() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        g.world.players[0].shooting.fired = [4, 0, 2, 0];
        g.world.players[0].shooting.hit = [3, 0, 1, 0];
        g.act("key.escape", "").unwrap();
        let ui = crate::api::ui(&g);
        let card = ui.panels.iter().find(|p| p.id == "shooting").expect("the card");
        let does: Vec<&str> = card.rows.iter().map(|r| r.does.as_str()).collect();
        assert_eq!(
            &does[..4],
            &["75% · 3 of 4", "No shots yet", "50% · 1 of 2", "No shots yet"]
        );
    }
}
