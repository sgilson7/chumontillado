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
/// M6: the volume a player starts at, and the step the setting moves by.
pub const VOLUME_START: i32 = 70;
pub const VOLUME_STEP: i32 = 10;
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
    /// M5: the skin this player wears (a setting), by `skins::SKINS` number
    pub skin: u8,
    /// a match against bots is under way (not the practice room)
    pub matched: bool,
    /// the map the static scene was built from
    scene_map: &'static str,
    /// R10.12: the notice about the links last shown
    notice_seen: Option<crate::versus::Notice>,
    /// M6: the sounds to play, gathered since the page last took them, and
    /// T15: the painted (Kuwahara) filter over the scene, a setting, off
    /// to start
    pub filter: bool,
    /// T18: the view shakes when hit, a setting, on to start
    pub shake: bool,
    /// the volume, percent (a setting)
    pub sounds: Vec<crate::sound::Play>,
    pub volume_pct: i32,
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
            skin: 0,
            matched: false,
            scene_map: "practice",
            notice_seen: None,
            sounds: Vec::new(),
            volume_pct: VOLUME_START,
            filter: false,
            shake: true,
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
        // R10.12: a new word about the links shows at once
        let notice = self.versus.as_ref().and_then(|v| v.notice(now_us));
        if notice != self.notice_seen {
            self.notice_seen = notice;
            self.ui_version += 1;
        }
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
                self.hear();
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
                // each sound once, a tick run again giving only what it
                // did not make before (`Rollback::take_heard`)
                let heard = s.take_heard();
                let (eyes, world) = (s.seat, &s.world);
                let p = &world.players[world.watching_or(eyes)];
                let plays = crate::sound::for_listener(&heard, eyes, p.eye_now(), p.aim.yaw, self.volume_pct);
                self.sounds.extend(plays);
            }
        }
        self.alpha = self.clock.alpha();
        if v.ended() {
            self.show(Screen::Online);
        }
        self.follow_hud();
    }

    /// M6: the last tick's sounds in the room or match against bots, as the
    /// seat whose eyes the view is from hears them.
    fn hear(&mut self) {
        let eyes = self.eyes();
        let w = &self.world;
        let Some(p) = w.players.get(eyes) else { return };
        let plays = crate::sound::for_listener(&w.heard, self.me(), p.eye_now(), p.aim.yaw, self.volume_pct);
        self.sounds.extend(plays);
    }

    /// M6: the sounds gathered since the page last took them, as JSON.
    pub fn take_sounds(&mut self) -> String {
        serde_json::to_string(&std::mem::take(&mut self.sounds)).unwrap_or_else(|_| "[]".into())
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

    /// M5: the settings, opened from the menu, show your skin beside the
    /// panel, turning slowly, so you see what you choose.
    fn previewing(&self) -> bool {
        self.screen == Screen::Settings && self.settings_from == Screen::Menu
    }

    pub fn camera(&self) -> render::Camera {
        if self.previewing() {
            // in the practice room by its spawn, looking north at the
            // figure, which stands off to the left of the panel
            let eye = v3(fx::int(8), fx::ONE * 13 / 10, fx::int(13));
            let aim = Aim {
                yaw: 0,
                pitch: -fx::deg(4),
                rest: (0, 0),
                pct: 100,
            };
            return camera::eye_view(eye, aim, camera::vertical_fov(self.fov, self.aspect));
        }
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
        self.view_world().camera(
            self.eyes(),
            self.alpha,
            self.pending.peek(),
            self.aspect,
            self.shake,
        )
    }

    /// The seat whose eyes the view is from: yours, or in a match while you
    /// are down, the next seat still standing (M5).
    pub fn eyes(&self) -> usize {
        let w = self.view_world();
        let me = self.me();
        if w.rounds.is_some() && !w.players[me].alive() {
            w.watching(me)
        } else {
            me
        }
    }

    /// Builds the frame the page draws next.
    pub fn draw(&mut self) {
        self.world.fov = self.fov;
        if let Some(s) = self.versus.as_mut().and_then(|v| v.session_mut()) {
            s.world.fov = self.fov;
        }
        // M5: a new round on a new map: its static scene
        let shown = self.view_world().map.name;
        let rebuilt = shown != self.scene_map;
        if rebuilt {
            self.scene_map = shown;
            self.groups = Self::map_groups(self.view_world(), &self.look);
        }
        let cam = self.camera();
        let origin = render::origin_for(cam.eye);
        if rebuilt || origin != self.scene.origin || self.scene.version == 0 {
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
            self.eyes(),
            first_person,
            self.alpha,
            pending,
            &self.look,
            &mut moving,
        );
        if self.previewing() {
            let turn = (self.title_ticks as i64 * fx::deg(1) as i64 / 2) as i32;
            let figure = crate::skins::Figure {
                skin: crate::skins::SKINS[self.skin as usize % crate::skins::COUNT],
                feet: v3(fx::ONE * 55 / 10, 0, fx::ONE * 95 / 10),
                yaw: fx::wrap(fx::HALF_TURN.wrapping_add(turn)),
                pitch: 0,
                ducked: false,
                flashed: false,
                speed: 0,
                on_ground: true,
                moment: 0,
                arms: None,
            };
            crate::skins::draw(&mut moving, &figure, &self.look);
        }
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

    /// M5: a match on your own against three bots, on the three maps in
    /// turn; it runs as an online match does, without the links.
    fn bots_match(&mut self) {
        self.leave_online();
        let mut skins = [self.skin, 0, 0, 0];
        // the bots wear the skins you do not
        let mut free = (0..crate::skins::COUNT as u8).filter(|k| *k != self.skin);
        for s in skins.iter_mut().skip(1) {
            *s = free.next().unwrap_or(0);
        }
        self.world = World::online(0b1, 0, skins);
        self.matched = true;
        self.clock.rest();
        self.show(Screen::Play);
    }

    /// The practice room from its start, its bot moving only or shooting.
    fn practice(&mut self, bot: Bot) {
        self.matched = false;
        self.bot = bot;
        self.world = World::room(bot);
        self.world.players[0].skin = self.skin;
        if let Some(b) = self.world.players.get_mut(1) {
            b.skin = (self.skin + 1) % crate::skins::COUNT as u8;
        }
        for p in &mut self.world.players {
            p.aim.pct = self.look_pct;
        }
        self.clock.rest();
        self.show(Screen::Play);
    }

    /// M4: a room to host or join: the transport is told to open it.
    fn open_room(&mut self, host: bool, code: &str) {
        let mut v = Versus::new(host, code, &self.build, self.skin);
        v.opened_us = Some(self.now_us);
        self.versus = Some(v);
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
            v.connected(peer, self.now_us);
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
            "menu.bots" => self.bots_match(),
            "play.resume" => self.show(Screen::Play),
            "play.restart" => {
                if self.versus.is_none() {
                    if self.matched {
                        self.bots_match()
                    } else {
                        self.practice(self.bot)
                    }
                }
            }
            // M5: your skin, a setting: the six in turn (T16); in a lobby the
            // others are told
            "skin.step" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.skin = (self.skin as i32 + step).rem_euclid(crate::skins::COUNT as i32) as u8;
                if let Some(x) = self.versus.as_mut() {
                    x.choose_skin(self.skin);
                }
                self.ui_version += 1;
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
            // R10.12: a guest that cannot find the room looks again
            "online.retry" => {
                let code = self.versus.as_ref().map(|x| x.code.clone()).unwrap_or_default();
                if code.is_empty() {
                    return Err(self.strings.get("online.bad_room"));
                }
                self.leave_online();
                self.open_room(false, &code);
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
            // T15: −1 turns the filter off, 1 on
            "filter.set" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.filter = step > 0;
                self.ui_version += 1;
            }
            // T18: −1 stills the view when hit, 1 lets it shake
            "shake.set" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.shake = step > 0;
                self.ui_version += 1;
            }
            "volume.step" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.volume_pct = (self.volume_pct + step * VOLUME_STEP).clamp(0, 100);
                self.ui_version += 1;
            }
            "fov.step" => {
                let step: i32 = v.parse().map_err(|_| format!("\"{value}\" is not a step."))?;
                self.fov = (self.fov + step * 5).clamp(camera::FOV_MIN, camera::FOV_MAX);
                self.ui_version += 1;
                self.draw();
            }
            // for tests and the journal's shots: in a match against bots, a
            // round on map k (0 the Plaza, 1 the Catedral, 2 the Galería),
            // past its count
            "place.map" => {
                let k: usize = v.parse().map_err(|_| format!("\"{value}\" is not a map."))?;
                if let Some(mut r) = self.world.rounds {
                    r.map = k % crate::maps::COUNT;
                    r.phase = crate::rounds::Phase::Live;
                    r.ticks = 0;
                    self.world.rounds = Some(r);
                    self.world.start_round(r.map);
                }
                self.draw();
            }
            // for tests and the journal's shots: the six skins standing in
            // a row 5 m in front of the spawn, each in its own skin holding a
            // different weapon (T16)
            "place.lineup" => {
                let w = &mut self.world;
                let spawn = w.players[0].spawn;
                w.players.truncate(1);
                let held = [
                    crate::weapons::Weapon::Rifle,
                    crate::weapons::Weapon::Toro,
                    crate::weapons::Weapon::Katana,
                    crate::weapons::Weapon::Shotgun,
                    crate::weapons::Weapon::Punal,
                    crate::weapons::Weapon::Pincel,
                ];
                for (k, g) in held.iter().enumerate() {
                    // 1.6 m apart, so the six fit between the room's slope and wall
                    let at = spawn
                        .0
                        .add(v3(fx::ONE * (k as i32 * 16 - 28) / 10, 0, -fx::int(5)));
                    let mut p = crate::world::Player::at((at, fx::HALF_TURN));
                    p.arms.weapon = *g;
                    p.skin = k as u8;
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
            // T18: for pictures, a hit of `dmg` on the first seat from (x, z)
            // cm at eye height
            "place.hurt" => {
                let n: Vec<i32> = v
                    .split_whitespace()
                    .map(|t| t.parse())
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("\"{value}\" is not three whole numbers."))?;
                let [dmg, x, z] = n[..] else {
                    return Err(format!("\"{value}\" is not three whole numbers."));
                };
                let p = &mut self.world.players[0];
                let from = v3(x * fx::ONE / 100, p.eye_now().y, z * fx::ONE / 100);
                p.hurt.struck(p.health, dmg, Some(from));
                p.health = (p.health - dmg).max(1);
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
    fn a_jump_in_the_room_reaches_the_page_as_a_sound_once() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        for k in 0..30 {
            g.advance(k * 16_667, 0, 0, 0, 0);
        }
        g.take_sounds();
        g.advance(30 * 16_667, crate::input::JUMP, 0, 0, 0);
        let plays: Vec<serde_json::Value> = serde_json::from_str(&g.take_sounds()).unwrap();
        let jumps: Vec<_> = plays.iter().filter(|p| p["voice"] == "jump").collect();
        assert_eq!(jumps.len(), 1, "{plays:?}");
        // the player's own jump, at the starting volume, from the middle
        assert_eq!(jumps[0]["gain"], VOLUME_START * 10);
        assert_eq!(jumps[0]["pan"], 0);
        assert_eq!(g.take_sounds(), "[]", "taken once");
    }

    #[test]
    fn a_hit_shows_on_the_health_bar_the_red_edge_and_the_arc_and_shake_is_a_setting() {
        let mut g = game();
        g.act("menu.practice", "").unwrap();
        let bar = hud_line(&g, "health");
        assert_eq!(
            (bar.fill, bar.trail, bar.tone),
            (Some(10_000), Some(10_000), None)
        );
        assert!(!hud_line(&g, "hurt").shown && !hud_line(&g, "bearing").shown);
        // a shot of 30 from straight behind
        let p = &mut g.world.players[0];
        let behind = p
            .eye_now()
            .add(crate::geom::facing(p.aim.yaw, 0).scale(-fx::int(10)));
        p.hurt.struck(100, 30, Some(behind));
        p.health = 70;
        let bar = hud_line(&g, "health");
        assert_eq!(
            (bar.fill, bar.trail, bar.tone),
            (Some(7_000), Some(10_000), Some("struck"))
        );
        let edge = hud_line(&g, "hurt");
        assert!(edge.shown && edge.fill.unwrap() > crate::hurt::EDGE_LEAST);
        let arc = hud_line(&g, "bearing");
        assert!(arc.shown);
        assert!((arc.turn.unwrap() - 18_000).abs() < 50, "{:?}", arc.turn);
        // low health pulses once the jolt is over
        g.world.players[0].health = 20;
        g.world.players[0].hurt.ticks = crate::hurt::HURT - crate::hurt::STRUCK;
        assert_eq!(hud_line(&g, "health").tone, Some("low"));
        // the shake starts on, and its setting turns it off
        assert!(g.shake);
        g.act("shake.set", "-1").unwrap();
        assert!(!g.shake);
        let row = crate::api::settings(&g)
            .into_iter()
            .find(|s| s.id == "shake")
            .unwrap();
        assert_eq!((row.label.as_str(), row.value.as_str()), ("Screen shake", "Off"));
    }

    #[test]
    fn the_painted_filter_starts_off_and_its_setting_turns_it_on_and_off() {
        let mut g = game();
        assert!(!g.filter, "off until the player turns it on");
        let row = |g: &Game| {
            crate::api::settings(g)
                .into_iter()
                .find(|s| s.id == "filter")
                .expect("a filter row")
                .value
        };
        assert_eq!(row(&g), "Off");
        g.act("filter.set", "1").unwrap();
        assert!(g.filter);
        assert_eq!(row(&g), "On");
        g.act("filter.set", "-1").unwrap();
        assert!(!g.filter);
    }

    #[test]
    fn the_volume_steps_by_ten_from_none_to_full_and_none_is_silent() {
        let mut g = game();
        assert_eq!(g.volume_pct, 70);
        for _ in 0..5 {
            g.act("volume.step", "1").unwrap();
        }
        assert_eq!(g.volume_pct, 100);
        for _ in 0..12 {
            g.act("volume.step", "-1").unwrap();
        }
        assert_eq!(g.volume_pct, 0);
        g.act("menu.practice", "").unwrap();
        for k in 0..30 {
            g.advance(k * 16_667, crate::input::JUMP, 0, 0, 0);
        }
        assert_eq!(g.take_sounds(), "[]");
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
        g.act("place.hurt", "40 0 0").unwrap();
        assert_eq!(g.world.players[0].health, 60);
        assert!(g.act("place.hurt", "40").is_err());
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

    // ---- M5 ----------------------------------------------------------------

    fn hud_line(g: &Game, id: &str) -> crate::api::Hud {
        crate::api::hud(g).into_iter().find(|h| h.id == id).expect(id)
    }

    #[test]
    fn play_against_bots_is_a_match_in_your_skin_the_bots_in_the_other_three() {
        let mut g = game();
        g.act("skin.step", "2").unwrap();
        assert_eq!(g.skin, 2);
        g.act("menu.bots", "").unwrap();
        let w = &g.world;
        assert!(w.rounds.is_some() && g.matched);
        let skins: Vec<u8> = w.players.iter().map(|p| p.skin).collect();
        assert_eq!(skins, vec![2, 0, 1, 3]);
        assert!(w.players[1..].iter().all(|p| p.pilot.is_some()));
        // the count shows 3 at the start; the round's line names the Plaza
        g.act("title.begin", "").ok();
        g.show(Screen::Play);
        assert_eq!(hud_line(&g, "count").text, "3");
        assert!(hud_line(&g, "count").shown);
        assert_eq!(hud_line(&g, "round").text, "Round 1 · the Plaza");
    }

    #[test]
    fn the_skin_setting_steps_round_the_six_and_shows_its_name() {
        let mut g = game();
        let name = |g: &Game| {
            crate::api::settings(g)
                .into_iter()
                .find(|r| r.id == "skin")
                .unwrap()
                .value
        };
        // back from the Hidalgo is the last of the six (T16)
        g.act("skin.step", "-1").unwrap();
        assert_eq!(g.skin, 5);
        assert_eq!(name(&g), "The Torero");
        g.act("skin.step", "-1").unwrap();
        assert_eq!(name(&g), "The Friar");
        g.act("skin.step", "-1").unwrap();
        assert_eq!(name(&g), "El Sordo");
        g.act("skin.step", "3").unwrap();
        assert_eq!(g.skin, 0);
    }

    #[test]
    fn from_seventy_five_seconds_a_marker_shows_where_each_standing_seat_is() {
        let mut g = game();
        g.act("menu.bots", "").unwrap();
        // seat 2 (a bot) stands 10 m straight ahead of you
        let me = g.world.players[0].body.origin;
        let ahead = crate::geom::facing(g.world.players[0].aim.yaw, 0).scale(fx::int(10));
        g.world.players[1].body.origin = me.add(ahead);
        g.world.players[1].prev = me.add(ahead);
        let mut r = g.world.rounds.unwrap();
        r.phase = crate::rounds::Phase::Live;
        r.ticks = crate::rounds::XRAY_TICKS - 1;
        g.world.rounds = Some(r);
        assert!(!hud_line(&g, "seen-1").shown, "not before 75 s");
        r.ticks = crate::rounds::XRAY_TICKS;
        g.world.rounds = Some(r);
        let m = hud_line(&g, "seen-1");
        assert!(m.shown);
        // straight ahead at eye height: the middle of the screen across,
        // and its middle up and down to within a few percent
        let [x, y] = m.at.unwrap();
        assert!((x - 5_000).abs() < 300 && (y - 5_000).abs() < 500, "{x} {y}");
        assert!(!hud_line(&g, "seen-0").shown, "not yourself");
    }

    #[test]
    fn while_you_are_down_in_a_match_the_view_is_from_the_next_seat_standing() {
        let mut g = game();
        g.act("menu.bots", "").unwrap();
        assert_eq!(g.eyes(), 0);
        g.world.players[0].dead = crate::world::DOWN_FOR_ROUND;
        assert_eq!(g.eyes(), 1);
        let mut r = g.world.rounds.unwrap();
        r.phase = crate::rounds::Phase::Live;
        g.world.rounds = Some(r);
        assert_eq!(hud_line(&g, "watching").text, "Down. Watching The Infanta.");
    }

    #[test]
    fn a_guests_skin_reaches_the_host_and_the_match_on_both_browsers() {
        let mut host = crate::versus::Versus::new(true, "KQXRT", "b", 1);
        let mut guest = crate::versus::Versus::new(false, "KQXRT", "b", 3);
        host.connected(1, 0);
        guest.connected(2, 0);
        let trade = |h: &mut crate::versus::Versus, gu: &mut crate::versus::Versus| {
            for _ in 0..3 {
                for (_, t) in gu.take_outbox() {
                    h.message(1, &t, 0);
                }
                for (_, t) in h.take_outbox() {
                    gu.message(2, &t, 0);
                }
            }
        };
        trade(&mut host, &mut guest);
        assert_eq!(host.links().skins[..2], [1, 3]);
        // the guest changes its mind in the lobby, to the last skin (T16:
        // the host once held a guest's choice to the first four)
        guest.choose_skin(5);
        trade(&mut host, &mut guest);
        assert_eq!(host.links().skins[1], 5);
        host.start();
        trade(&mut host, &mut guest);
        // the bots wear the first two skins no person does: 0 and 2
        let skins = |v: &crate::versus::Versus| -> Vec<u8> {
            v.session()
                .unwrap()
                .world
                .players
                .iter()
                .map(|p| p.skin)
                .collect()
        };
        assert_eq!(skins(&host), vec![1, 5, 0, 2]);
        assert_eq!(skins(&guest), skins(&host));
    }

    #[test]
    fn a_guest_still_looking_is_told_why_and_can_try_again_on_the_same_room() {
        let mut g = game();
        g.act("online.link", "KQXRT").unwrap();
        g.advance(25_000_000, 0, 0, 0, 0);
        let ui = crate::api::ui(&g);
        assert!(ui.panels[0]
            .text
            .starts_with("Still no answer from room KQXRT after 25 s."));
        assert!(ui.panels[0].actions.iter().any(|a| a.name == "online.retry"));
        let opens = g.cmds.iter().filter(|c| c.act == "open").count();
        g.act("online.retry", "").unwrap();
        let last: Vec<&str> = g.cmds.iter().rev().take(2).map(|c| c.act.as_str()).collect();
        assert_eq!(last, vec!["open", "close"]);
        assert_eq!(g.cmds.iter().filter(|c| c.act == "open").count(), opens + 1);
        assert_eq!(g.cmds.last().unwrap().room, "chumontillado-p1-KQXRT");
        // and the count starts again
        assert_eq!(
            g.versus.as_ref().unwrap().notice(g.now_us),
            Some(crate::versus::Notice::Looking { secs: 0 })
        );
    }
}
