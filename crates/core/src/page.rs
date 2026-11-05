//! The reader-visible strings, from `data/strings.json`. The core checks the
//! file before anything is drawn: every key the game uses must be there and
//! hold text.
//!
//! The file is one object of key to text. A milestone that needs a string
//! adds its key to the file and to `REQUIRED` (adding entries only), and
//! reads it with `Strings::get`. Keys starting with `_` hold notes.

use std::collections::BTreeMap;

use serde::Serialize;

/// Every key the game reads, so a missing one is found at start, not mid-game.
pub const REQUIRED: &[&str] = &[
    "title",
    "loading",
    "no_webgl",
    "tagline",
    "begin",
    "menu_text",
    "practice",
    "settings",
    "back",
    "paused",
    "resume",
    "restart",
    "to_menu",
    "controls",
    "key_move",
    "control_move",
    "key_look",
    "control_look",
    "key_jump",
    "control_jump",
    "key_crouch",
    "control_crouch",
    "key_wall",
    "control_wall",
    "key_reset",
    "control_reset",
    "key_pause",
    "control_pause",
    "setting_fov",
    "setting_look",
    "degrees",
    "percent",
    "narrower",
    "wider",
    "slower",
    "faster",
    "hud_speed",
    "hud_walls",
    "hud_top",
    "hud_ground",
    "hud_air",
    "hud_sliding",
    "hud_slamming",
    "hint_play",
    "duel",
    "key_fire",
    "control_fire",
    "key_reload",
    "control_reload",
    "key_guns",
    "control_guns",
    "gun_pistol",
    "gun_shotgun",
    "gun_machine_pistol",
    "gun_rifle",
    "gun_pincel",
    "gun_drum",
    "gun_trabuco",
    "gun_toro",
    "gun_campana",
    "gun_collage",
    "melee_katana",
    "melee_navaja",
    "melee_guadana",
    "key_melee",
    "control_melee",
    "key_alt",
    "control_alt",
    "host_match",
    "join_match",
    "host_text",
    "join_text",
    "joining_text",
    "room_code",
    "invite_link",
    "copy_code",
    "copy_link",
    "copied",
    "join",
    "start_match",
    "leave",
    "seats",
    "seat_n",
    "seat_you",
    "seat_player",
    "seat_bot",
    "seat_empty",
    "seat_bot_on",
    "seat_bot_off",
    "seat_you_n",
    "seat_player_n",
    "bots_match",
    "setting_skin",
    "skin_prev",
    "skin_next",
    "skin_0",
    "skin_1",
    "skin_2",
    "skin_3",
    "lobby_skin",
    "you",
    "map_plaza",
    "map_catedral",
    "map_galeria",
    "map_mercado",
    "map_puerto",
    "map_acueducto",
    "hud_round",
    "hud_score",
    "hud_clock",
    "hud_round_won",
    "hud_round_you",
    "hud_round_none",
    "hud_match_won",
    "hud_match_you",
    "hud_watching",
    "online.knocking",
    "online.still_looking",
    "online.waiting_for",
    "online.dropped",
    "retry",
    "page_error",
    "context_lost",
    "setting_volume",
    "setting_filter",
    "filter_on",
    "filter_off",
    "quieter",
    "louder",
    "shooting",
    "band_walk",
    "band_run",
    "band_fast",
    "band_trick",
    "band_count",
    "band_none",
    "time_fast",
    "best_speed",
    "match_ended",
    "hud_seat",
    "online.waiting",
    "online.lobby",
    "online.playing",
    "online.reload",
    "online.garbled",
    "online.left",
    "online.host_left",
    "online.full",
    "online.desync",
    "online.timeout",
    "online.no_bundle",
    "online.no_relay",
    "online.no_path",
    "online.bad_room",
    "online.not_host",
    "hud_health",
    "hud_ammo",
    "hud_reloading",
    "hud_charge",
    "hud_dead",
];

/// The reader-visible strings the page needs. They live in data, not source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Strings(pub BTreeMap<String, String>);

/// Why a strings file cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringsError {
    NotJson(String),
    Missing(String),
    Empty(String),
}

impl StringsError {
    /// The message the page shows. Wording that depends on the rule stays
    /// beside the rule.
    pub fn message(&self) -> String {
        match self {
            StringsError::NotJson(detail) => format!("The strings file could not be read: {detail}"),
            StringsError::Missing(key) => format!("The strings file has no \"{key}\"."),
            StringsError::Empty(key) => format!("The strings file has an empty \"{key}\"."),
        }
    }
}

impl Strings {
    /// The text for a key; a key outside `REQUIRED` that is missing reads as
    /// itself, so the gap shows on screen rather than as a blank.
    pub fn get(&self, key: &str) -> String {
        self.0.get(key).cloned().unwrap_or_else(|| key.to_string())
    }
}

/// Parses and checks the strings file. A value that is only whitespace counts
/// as empty, because the page would draw nothing.
pub fn parse_strings(json: &str) -> Result<Strings, StringsError> {
    let all: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| StringsError::NotJson(e.to_string()))?;
    let mut out = BTreeMap::new();
    for (k, v) in all {
        if k.starts_with('_') {
            continue;
        }
        let Some(text) = v.as_str() else {
            return Err(StringsError::NotJson(format!("\"{k}\" is not text")));
        };
        if text.trim().is_empty() {
            return Err(StringsError::Empty(k));
        }
        out.insert(k, text.to_string());
    }
    if let Some(missing) = REQUIRED.iter().find(|k| !out.contains_key(**k)) {
        return Err(StringsError::Missing(missing.to_string()));
    }
    Ok(Strings(out))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn strings() -> Strings {
        parse_strings(include_str!("../../../data/strings.json")).expect("data/strings.json")
    }

    fn with(key: &str, value: serde_json::Value) -> String {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/strings.json")).unwrap();
        v[key] = value;
        v.to_string()
    }

    #[test]
    fn the_strings_file_holds_every_required_key() {
        let s = strings();
        for k in REQUIRED {
            assert!(!s.get(k).trim().is_empty(), "{k}");
        }
    }

    #[test]
    fn an_empty_title_is_rejected_by_name() {
        let e = parse_strings(&with("title", "  ".into())).unwrap_err();
        assert_eq!(e, StringsError::Empty("title".into()));
        assert_eq!(e.message(), "The strings file has an empty \"title\".");
    }

    #[test]
    fn a_missing_key_is_rejected_by_name() {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/strings.json")).unwrap();
        v.as_object_mut().unwrap().remove("resume");
        let e = parse_strings(&v.to_string()).unwrap_err();
        assert_eq!(e.message(), "The strings file has no \"resume\".");
    }

    #[test]
    fn a_value_that_is_not_text_and_text_that_is_not_json_are_rejected() {
        assert!(matches!(
            parse_strings(&with("practice", 3.into())),
            Err(StringsError::NotJson(_))
        ));
        assert!(matches!(parse_strings("title: T"), Err(StringsError::NotJson(_))));
    }

    #[test]
    fn notes_are_skipped_and_an_unknown_key_reads_as_itself() {
        let s = parse_strings(&with("_note", "for people".into())).unwrap();
        assert!(!s.0.contains_key("_note"));
        assert_eq!(s.get("no.such.key"), "no.such.key");
    }
}
