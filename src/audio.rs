//! Audio: background-music loops and one-shot/looping sound effects, wired to
//! the `Settings` volume controls.
//!
//! The [`Music`] and [`Sfx`] enums are **plain data** (no macroquad types), so
//! the pure game modules (`Level`/`Run`) can emit `Sfx` values without needing
//! a render/audio dependency in the *logic* — `App` is the only place that has
//! an [`Audio`] instance and turns those values into actual playback.
//!
//! All file paths are relative to the current directory on desktop and to the
//! server root on web (the `build-web.sh` script stages `assets/` alongside the
//! wasm), so `macroquad::audio::load_sound` resolves them on both targets.

use macroquad::audio::{self, PlaySoundParams, Sound};

/// Background-music tracks. Indexed by `as usize` into [`Audio::music`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Music {
    /// Menu / level-select / settings / pause / outcome screens.
    Menu,
    /// Active gameplay (no beast has a clear path to the player).
    Level,
    /// Tense variant while a beast has a clear path to the player.
    Chase,
}

impl Music {
    /// The asset path for this music loop.
    pub fn path(self) -> &'static str {
        match self {
            Music::Menu => "assets/audio/music/menu_loop.wav",
            Music::Level => "assets/audio/music/level_loop.wav",
            Music::Chase => "assets/audio/music/chase_loop.wav",
        }
    }
}

/// One-shot (occasionally looping) sound effects. `GemPickup` is the M8 story
/// beat: it plays once on the intro screen when a brand-new run begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// Continuous loop while the player mines (suppressed during Super Pick).
    Dig,
    /// Continuous loop while a beast digs.
    BeastDig,
    /// One-shot when a rock breaks (player or beast).
    RockBreak,
    /// One-shot when the player collects a gold pickup.
    GoldPickup,
    /// One-shot when Super Pick activates.
    SuperPick,
    /// One-shot when Sticky Smell activates.
    StickySmell,
    /// One-shot when a beast becomes "near" the player (edge, cooldown-gated).
    BeastGrowl,
    /// One-shot tick while the player walks.
    Footstep,
    /// One-shot when the player is caught.
    Caught,
    /// One-shot when a level completes (reused for victory).
    LevelComplete,
    /// One-shot on game over.
    GameOver,
    /// One-shot on a successful shop purchase.
    Purchase,
    /// One-shot story beat: the gem is found (played on the M8 intro screen).
    GemPickup,
    /// One-shot on menu navigation/activation.
    UiClick,
}

impl Sfx {
    /// Every sfx in declaration (index) order, used to load them at boot.
    pub const ALL: [Sfx; 14] = [
        Sfx::Dig,
        Sfx::BeastDig,
        Sfx::RockBreak,
        Sfx::GoldPickup,
        Sfx::SuperPick,
        Sfx::StickySmell,
        Sfx::BeastGrowl,
        Sfx::Footstep,
        Sfx::Caught,
        Sfx::LevelComplete,
        Sfx::GameOver,
        Sfx::Purchase,
        Sfx::GemPickup,
        Sfx::UiClick,
    ];

    /// The asset path for this sound effect.
    pub fn path(self) -> &'static str {
        match self {
            Sfx::Dig => "assets/audio/sfx/sfx_dig.wav",
            Sfx::BeastDig => "assets/audio/sfx/sfx_beast_dig.wav",
            Sfx::RockBreak => "assets/audio/sfx/sfx_rock_break.wav",
            Sfx::GoldPickup => "assets/audio/sfx/sfx_gold_pickup.wav",
            Sfx::SuperPick => "assets/audio/sfx/sfx_super_pick.wav",
            Sfx::StickySmell => "assets/audio/sfx/sfx_sticky_smell.wav",
            Sfx::BeastGrowl => "assets/audio/sfx/sfx_beast_growl.wav",
            Sfx::Footstep => "assets/audio/sfx/sfx_footstep.wav",
            Sfx::Caught => "assets/audio/sfx/sfx_caught.wav",
            Sfx::LevelComplete => "assets/audio/sfx/sfx_level_complete.wav",
            Sfx::GameOver => "assets/audio/sfx/sfx_game_over.wav",
            Sfx::Purchase => "assets/audio/sfx/sfx_purchase.wav",
            Sfx::GemPickup => "assets/audio/sfx/sfx_gem_pickup.wav",
            Sfx::UiClick => "assets/audio/sfx/sfx_ui_click.wav",
        }
    }
}

/// Loaded sounds + playback state. Owned by `App`; the game logic only ever
/// produces [`Sfx`]/[`Music`] values and hands them here.
pub struct Audio {
    /// Music loops, indexed by `Music as usize`.
    music: Vec<Sound>,
    /// Sound effects, indexed by `Sfx as usize`.
    sfx: Vec<Sound>,
    music_volume: f32,
    sfx_volume: f32,
    /// The loop currently playing, if any.
    current_music: Option<Music>,
    dig_playing: bool,
    beast_dig_playing: bool,
}

impl Audio {
    /// Load every music loop and sound effect.
    pub async fn load() -> Audio {
        let mut music = Vec::with_capacity(3);
        for m in [Music::Menu, Music::Level, Music::Chase] {
            music.push(audio::load_sound(m.path()).await.expect("music loop should load"));
        }

        let mut sfx = Vec::with_capacity(Sfx::ALL.len());
        for s in Sfx::ALL {
            sfx.push(audio::load_sound(s.path()).await.expect("sfx should load"));
        }

        Audio { music, sfx, music_volume: 1.0, sfx_volume: 1.0, current_music: None, dig_playing: false, beast_dig_playing: false }
    }

    /// Start the music loop `m` at the current music volume, stopping any other
    /// loop. No-op if `m` is already playing.
    pub fn play_music(&mut self, m: Music) {
        if self.current_music == Some(m) {
            return;
        }
        if let Some(cur) = self.current_music {
            audio::stop_sound(&self.music[cur as usize]);
        }
        audio::play_sound(&self.music[m as usize], PlaySoundParams { looped: true, volume: self.music_volume });
        self.current_music = Some(m);
    }

    /// Play a one-shot effect at the current sfx volume.
    pub fn play(&mut self, s: Sfx) {
        audio::play_sound(&self.sfx[s as usize], PlaySoundParams { looped: false, volume: self.sfx_volume });
    }

    /// Begin a continuous (looped) effect — only valid for the dig loops; other
    /// effects (or one already running) are ignored so it never restarts every
    /// frame.
    pub fn start_loop(&mut self, s: Sfx) {
        let already = match s {
            Sfx::Dig => self.dig_playing,
            Sfx::BeastDig => self.beast_dig_playing,
            _ => return,
        };
        if already {
            return;
        }
        audio::play_sound(&self.sfx[s as usize], PlaySoundParams { looped: true, volume: self.sfx_volume });
        match s {
            Sfx::Dig => self.dig_playing = true,
            Sfx::BeastDig => self.beast_dig_playing = true,
            _ => {}
        }
    }

    /// Stop a continuous effect started with [`Audio::start_loop`]. No-op if it
    /// isn't running.
    pub fn stop_loop(&mut self, s: Sfx) {
        let playing = match s {
            Sfx::Dig => self.dig_playing,
            Sfx::BeastDig => self.beast_dig_playing,
            _ => return,
        };
        if !playing {
            return;
        }
        audio::stop_sound(&self.sfx[s as usize]);
        match s {
            Sfx::Dig => self.dig_playing = false,
            Sfx::BeastDig => self.beast_dig_playing = false,
            _ => {}
        }
    }

    /// Apply a new music volume live (also re-volumes the current loop).
    pub fn set_music_volume(&mut self, v: f32) {
        self.music_volume = v.clamp(0.0, 1.0);
        if let Some(cur) = self.current_music {
            audio::set_sound_volume(&self.music[cur as usize], self.music_volume);
        }
    }

    /// Store a new sfx volume; applied to future plays/loops.
    pub fn set_sfx_volume(&mut self, v: f32) {
        self.sfx_volume = v.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_paths_are_complete_and_distinct() {
        let mut seen = std::collections::HashSet::new();
        for m in [Music::Menu, Music::Level, Music::Chase] {
            let p = m.path();
            assert!(p.ends_with(".wav"), "music path is a wav: {p}");
            assert!(seen.insert(p), "music path unique: {p}");
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn sfx_paths_cover_every_variant_and_are_valid_wavs() {
        let mut seen = std::collections::HashSet::new();
        for s in Sfx::ALL {
            let p = s.path();
            assert!(p.starts_with("assets/audio/sfx/"), "sfx in the sfx dir: {p}");
            assert!(p.ends_with(".wav"), "sfx is a wav: {p}");
            assert!(seen.insert(p), "sfx path unique: {p}");
        }
        // Every variant is represented exactly once in ALL (which is also the
        // index order used to load/play).
        assert_eq!(seen.len(), Sfx::ALL.len());
    }

    #[test]
    fn all_variants_are_in_all() {
        let all: Vec<Sfx> = Sfx::ALL.to_vec();
        for s in [
            Sfx::Dig,
            Sfx::BeastDig,
            Sfx::RockBreak,
            Sfx::GoldPickup,
            Sfx::SuperPick,
            Sfx::StickySmell,
            Sfx::BeastGrowl,
            Sfx::Footstep,
            Sfx::Caught,
            Sfx::LevelComplete,
            Sfx::GameOver,
            Sfx::Purchase,
            Sfx::GemPickup,
            Sfx::UiClick,
        ] {
            assert!(all.contains(&s), "all must list {s:?}");
        }
    }

    #[test]
    fn discriminants_are_stable_for_indexing() {
        // `Audio` indexes the sfx vector by `s as usize`; these discriminants
        // must match the order in `Sfx::ALL` so a variant plays its own file.
        assert_eq!(Sfx::Dig as usize, 0);
        assert_eq!(Sfx::UiClick as usize, Sfx::ALL.len() - 1);
        assert_eq!(Music::Menu as usize, 0);
        assert_eq!(Music::Chase as usize, 2);
    }
}
