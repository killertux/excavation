//! Menu screens as pure, testable state machines (no macroquad).
//!
//! Each screen owns a selection index and an `update(MenuInput) -> MenuAction`
//! that the `App` fills with edge-triggered key presses and then executes. This
//! keeps all menu logic free of GPU/input polling so it is fully unit-testable.

/// Edge-triggered menu input booleans, filled by the app from key presses.
#[derive(Debug, Clone, Copy, Default)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub escape: bool,
}

/// An action a menu screen requests; the `App` interprets and executes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// Nothing happened; keep the frame as is.
    None,
    /// Start a brand-new run (clears the existing save).
    NewGame,
    /// Resume the saved run.
    Continue,
    OpenLevelSelect,
    OpenSettings,
    /// Leave the current screen (return to wherever it was opened from).
    Back,
    /// Start playing the given (0-based) level.
    StartLevel(usize),
    ToggleFullscreen,
    VolumeUpMusic,
    VolumeDownMusic,
    VolumeUpSfx,
    VolumeDownSfx,
    Resume,
    RestartLevel,
    Save,
    SaveAndQuitToMenu,
    Quit,
}

/// Where the Settings screen was opened from, so "Back" returns correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSource {
    Main,
    Pause,
}

/// The main-menu entries. `Continue` is only visible when a save exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    Play,
    Continue,
    LevelSelect,
    Settings,
    Quit,
}

/// The main menu: Play / [Continue] / Level Select / Settings / Quit.
#[derive(Debug, Clone, Copy)]
pub struct MainMenu {
    pub selection: usize,
    pub has_save: bool,
}

impl MainMenu {
    pub fn new(has_save: bool) -> Self {
        MainMenu { selection: 0, has_save }
    }

    /// The visible menu entries in order (Continue hidden when no save).
    pub fn items(&self) -> Vec<MainMenuItem> {
        let mut v = vec![MainMenuItem::Play];
        if self.has_save {
            v.push(MainMenuItem::Continue);
        }
        v.push(MainMenuItem::LevelSelect);
        v.push(MainMenuItem::Settings);
        v.push(MainMenuItem::Quit);
        v
    }

    fn item_at(&self, i: usize) -> Option<MainMenuItem> {
        self.items().get(i).copied()
    }

    pub fn update(&mut self, input: &MenuInput) -> MenuAction {
        let n = self.items().len();
        if n == 0 {
            return MenuAction::None;
        }
        if self.selection >= n {
            self.selection = n - 1;
        }
        if input.up {
            self.selection = (self.selection + n - 1) % n;
        }
        if input.down {
            self.selection = (self.selection + 1) % n;
        }
        if input.enter {
            return match self.item_at(self.selection) {
                Some(MainMenuItem::Play) => MenuAction::NewGame,
                Some(MainMenuItem::Continue) => MenuAction::Continue,
                Some(MainMenuItem::LevelSelect) => MenuAction::OpenLevelSelect,
                Some(MainMenuItem::Settings) => MenuAction::OpenSettings,
                Some(MainMenuItem::Quit) | None => MenuAction::Quit,
            };
        }
        MenuAction::None
    }
}

/// The level-select screen: `level_count` levels, `unlocked` of them selectable.
#[derive(Debug, Clone, Copy)]
pub struct LevelSelect {
    pub selection: usize,
    pub level_count: usize,
    pub unlocked: usize,
}

impl LevelSelect {
    pub fn new(level_count: usize, unlocked: usize) -> Self {
        LevelSelect { selection: 0, level_count, unlocked }
    }

    /// Whether a 0-based level is locked (its 1-based index exceeds `unlocked`).
    pub fn is_locked(&self, index: usize) -> bool {
        index + 1 > self.unlocked
    }

    pub fn update(&mut self, input: &MenuInput) -> MenuAction {
        let n = self.level_count;
        if n == 0 {
            return MenuAction::Back;
        }
        if self.selection >= n {
            self.selection = n - 1;
        }
        if input.up {
            self.selection = (self.selection + n - 1) % n;
        }
        if input.down {
            self.selection = (self.selection + 1) % n;
        }
        if input.enter {
            // Locked levels are unselectable: enter does nothing on them.
            if !self.is_locked(self.selection) {
                return MenuAction::StartLevel(self.selection);
            }
        }
        if input.escape {
            return MenuAction::Back;
        }
        MenuAction::None
    }
}

/// The settings screen: Music / SFX (left-right), Fullscreen (enter), Back.
#[derive(Debug, Clone, Copy)]
pub struct SettingsScreen {
    pub selection: usize,
    pub source: MenuSource,
}

impl SettingsScreen {
    pub fn new(source: MenuSource) -> Self {
        SettingsScreen { selection: 0, source }
    }

    pub fn update(&mut self, input: &MenuInput) -> MenuAction {
        let n = 4; // Music, SFX, Fullscreen, Back
        if self.selection >= n {
            self.selection = n - 1;
        }
        if input.up {
            self.selection = (self.selection + n - 1) % n;
        }
        if input.down {
            self.selection = (self.selection + 1) % n;
        }
        if input.left {
            return match self.selection {
                0 => MenuAction::VolumeDownMusic,
                1 => MenuAction::VolumeDownSfx,
                _ => MenuAction::None,
            };
        }
        if input.right {
            return match self.selection {
                0 => MenuAction::VolumeUpMusic,
                1 => MenuAction::VolumeUpSfx,
                _ => MenuAction::None,
            };
        }
        if input.enter {
            return match self.selection {
                2 => MenuAction::ToggleFullscreen,
                3 => MenuAction::Back,
                _ => MenuAction::None,
            };
        }
        if input.escape {
            return MenuAction::Back;
        }
        MenuAction::None
    }
}

/// The pause menu: Resume / Restart Level / Save / Settings / Quit to Menu.
#[derive(Debug, Clone, Copy)]
pub struct Pause {
    pub selection: usize,
}

impl Pause {
    pub fn new() -> Self {
        Pause { selection: 0 }
    }

    pub fn update(&mut self, input: &MenuInput) -> MenuAction {
        let n = 5;
        if self.selection >= n {
            self.selection = n - 1;
        }
        if input.up {
            self.selection = (self.selection + n - 1) % n;
        }
        if input.down {
            self.selection = (self.selection + 1) % n;
        }
        if input.enter {
            return match self.selection {
                0 => MenuAction::Resume,
                1 => MenuAction::RestartLevel,
                2 => MenuAction::Save,
                3 => MenuAction::OpenSettings,
                4 => MenuAction::SaveAndQuitToMenu,
                _ => MenuAction::None,
            };
        }
        if input.escape {
            return MenuAction::Resume;
        }
        MenuAction::None
    }
}

/// Whether a menu action should route to the story intro. A brand-new run
/// (`NewGame`) shows the intro; `Continue` and level-select `StartLevel` skip it
/// so a returning/replaying player goes straight to the action.
pub fn shows_intro(action: MenuAction) -> bool {
    matches!(action, MenuAction::NewGame)
}

/// The currently-active menu screen (meaningful only while in a menu state).
#[derive(Debug, Clone, Copy)]
pub enum Menu {
    Main(MainMenu),
    LevelSelect(LevelSelect),
    Settings(SettingsScreen),
    Pause(Pause),
}

impl Menu {
    /// Feed `input` to the active screen and return the action it requests.
    pub fn update(&mut self, input: &MenuInput) -> MenuAction {
        match self {
            Menu::Main(m) => m.update(input),
            Menu::LevelSelect(m) => m.update(input),
            Menu::Settings(m) => m.update(input),
            Menu::Pause(m) => m.update(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(f: impl FnOnce(&mut MenuInput)) -> MenuInput {
        let mut i = MenuInput::default();
        f(&mut i);
        i
    }

    #[test]
    fn main_menu_hides_continue_without_a_save() {
        let m = MainMenu::new(false);
        let items = m.items();
        assert_eq!(items, vec![MainMenuItem::Play, MainMenuItem::LevelSelect, MainMenuItem::Settings, MainMenuItem::Quit]);
    }

    #[test]
    fn main_menu_shows_continue_with_a_save() {
        let m = MainMenu::new(true);
        assert_eq!(
            m.items(),
            vec![MainMenuItem::Play, MainMenuItem::Continue, MainMenuItem::LevelSelect, MainMenuItem::Settings, MainMenuItem::Quit]
        );
    }

    #[test]
    fn main_menu_selection_wraps_and_selects() {
        let mut m = MainMenu::new(true);
        // Start at Play (0); down wraps to Quit (4).
        assert_eq!(m.update(&key(|i| i.down = true)), MenuAction::None);
        assert_eq!(m.selection, 1);
        // Continue is at index 1; select it.
        assert_eq!(m.update(&key(|i| i.enter = true)), MenuAction::Continue);
        // Down through to Quit and select.
        for _ in 0..3 {
            m.update(&key(|i| i.down = true));
        }
        assert_eq!(m.selection, 4);
        assert_eq!(m.update(&key(|i| i.enter = true)), MenuAction::Quit);
    }

    #[test]
    fn main_menu_up_wraps_to_last_item() {
        let mut m = MainMenu::new(false);
        assert_eq!(m.update(&key(|i| i.up = true)), MenuAction::None);
        assert_eq!(m.selection, 3, "up from Play wraps to Quit (4 items, no save)");
    }

    #[test]
    fn main_menu_play_starts_new_game() {
        let mut m = MainMenu::new(false);
        assert_eq!(m.update(&key(|i| i.enter = true)), MenuAction::NewGame);
    }

    #[test]
    fn level_select_locked_levels_are_unselectable() {
        let mut ls = LevelSelect::new(3, 1); // only level 1 unlocked
        assert!(ls.is_locked(1));
        assert!(ls.is_locked(2));
        // Navigate down once to level 2 (index 1, locked); enter does nothing.
        ls.update(&key(|i| i.down = true));
        assert_eq!(ls.selection, 1);
        assert_eq!(ls.update(&key(|i| i.enter = true)), MenuAction::None, "locked level cannot be selected");
    }

    #[test]
    fn level_select_start_on_unlocked_level() {
        let mut ls = LevelSelect::new(3, 2);
        assert!(!ls.is_locked(0));
        assert_eq!(ls.update(&key(|i| i.enter = true)), MenuAction::StartLevel(0));
        // Move to level 2 (index 1) and select it.
        ls.update(&key(|i| i.down = true));
        assert_eq!(ls.update(&key(|i| i.enter = true)), MenuAction::StartLevel(1));
    }

    #[test]
    fn level_select_escape_returns_back() {
        let mut ls = LevelSelect::new(3, 3);
        assert_eq!(ls.update(&key(|i| i.escape = true)), MenuAction::Back);
    }

    #[test]
    fn settings_left_right_adjust_sliders() {
        let mut s = SettingsScreen::new(MenuSource::Main);
        // Row 0 (music): right -> up, left -> down.
        assert_eq!(s.update(&key(|i| i.right = true)), MenuAction::VolumeUpMusic);
        assert_eq!(s.update(&key(|i| i.left = true)), MenuAction::VolumeDownMusic);
        // Row 1 (sfx): move down then adjust.
        s.update(&key(|i| i.down = true));
        assert_eq!(s.update(&key(|i| i.right = true)), MenuAction::VolumeUpSfx);
        assert_eq!(s.update(&key(|i| i.left = true)), MenuAction::VolumeDownSfx);
    }

    #[test]
    fn settings_enter_toggles_fullscreen_and_back() {
        let mut s = SettingsScreen::new(MenuSource::Main);
        s.update(&key(|i| i.down = true));
        s.update(&key(|i| i.down = true)); // row 2 = fullscreen
        assert_eq!(s.update(&key(|i| i.enter = true)), MenuAction::ToggleFullscreen);
        s.update(&key(|i| i.down = true)); // row 3 = back
        assert_eq!(s.update(&key(|i| i.enter = true)), MenuAction::Back);
    }

    #[test]
    fn settings_escape_returns_back_anywhere() {
        let mut s = SettingsScreen::new(MenuSource::Pause);
        assert_eq!(s.update(&key(|i| i.escape = true)), MenuAction::Back);
    }

    #[test]
    fn pause_navigation_and_actions() {
        let mut p = Pause::new();
        // Up wraps from Resume (0) to Quit to Menu (4).
        assert_eq!(p.update(&key(|i| i.up = true)), MenuAction::None);
        assert_eq!(p.selection, 4);
        assert_eq!(p.update(&key(|i| i.enter = true)), MenuAction::SaveAndQuitToMenu);
        // Reset selection; move to Save (2).
        p = Pause::new();
        p.update(&key(|i| i.down = true));
        p.update(&key(|i| i.down = true));
        assert_eq!(p.update(&key(|i| i.enter = true)), MenuAction::Save);
        // Escape always resumes.
        assert_eq!(p.update(&key(|i| i.escape = true)), MenuAction::Resume);
    }

    #[test]
    fn menu_dispatches_to_active_screen() {
        let mut menu = Menu::Main(MainMenu::new(true));
        assert_eq!(menu.update(&key(|i| i.enter = true)), MenuAction::NewGame);
        let mut menu = Menu::Pause(Pause::new());
        assert_eq!(menu.update(&key(|i| i.escape = true)), MenuAction::Resume);
    }

    #[test]
    fn shows_intro_only_for_a_fresh_run() {
        // The story intro is shown only on a brand-new run; Continue and level
        // select (StartLevel) bypass it, as do every other menu action.
        assert!(shows_intro(MenuAction::NewGame));
        assert!(!shows_intro(MenuAction::Continue));
        assert!(!shows_intro(MenuAction::StartLevel(2)));
        assert!(!shows_intro(MenuAction::OpenLevelSelect));
        assert!(!shows_intro(MenuAction::OpenSettings));
        assert!(!shows_intro(MenuAction::Resume));
        assert!(!shows_intro(MenuAction::RestartLevel));
        assert!(!shows_intro(MenuAction::None));
    }
}
