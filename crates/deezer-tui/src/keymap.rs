use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// All remappable user actions. Serialized as stable string identifiers in the
/// keybindings file so the file stays human-editable and forward-compatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Application / global
    AppQuit,
    AppDetach,
    AppBackground,
    HelpToggle,
    SettingsToggle,
    InfoToggle,
    EqualizerToggle,
    LyricsToggle,

    // Search / filter focus
    SearchFocus,

    // Navigation
    NavUp,
    NavDown,
    NavLeft,
    NavRight,
    NavPageUp,
    NavPageDown,
    NavHome,
    NavEnd,
    Select,
    Back,
    NextTab,
    PrevTab,
    NextCategory,
    PrevCategory,

    // Playback / player
    PlaybackToggle,
    PlaybackNext,
    PlaybackPrev,
    PlaybackSeekForward,
    PlaybackSeekBackward,
    VolumeUp,
    VolumeDown,
    ShuffleToggle,
    RepeatCycle,
    FlowStart,
    ShuffleFavorites,

    // Item / track actions
    ContextMenu,
    PlayingContextMenu,
    OpenAlbum,
    OpenArtist,
    WaitingList,
    LikeToggle,

    // Overlays / pickers / modals
    SortCycle,
    DownloadOffline,
    FilterStart,
    FollowLyrics,
    TranslateLyrics,
    EqPresetNext,
    EqPresetPrev,
    EqReset,
    EqToggle,
    ConfirmYes,
    ConfirmNo,
    ConfirmToggle,
    DeleteItem,

    // Login
    LoginWeb,
    LoginArlMode,
}

impl Action {
    /// Ordered list of every action, used to guarantee a complete file.
    pub fn all() -> &'static [Action] {
        use Action::*;
        &[
            AppQuit,
            AppDetach,
            AppBackground,
            HelpToggle,
            SettingsToggle,
            InfoToggle,
            EqualizerToggle,
            LyricsToggle,
            SearchFocus,
            NavUp,
            NavDown,
            NavLeft,
            NavRight,
            NavPageUp,
            NavPageDown,
            NavHome,
            NavEnd,
            Select,
            Back,
            NextTab,
            PrevTab,
            NextCategory,
            PrevCategory,
            PlaybackToggle,
            PlaybackNext,
            PlaybackPrev,
            PlaybackSeekForward,
            PlaybackSeekBackward,
            VolumeUp,
            VolumeDown,
            ShuffleToggle,
            RepeatCycle,
            FlowStart,
            ShuffleFavorites,
            ContextMenu,
            PlayingContextMenu,
            OpenAlbum,
            OpenArtist,
            WaitingList,
            LikeToggle,
            SortCycle,
            DownloadOffline,
            FilterStart,
            FollowLyrics,
            TranslateLyrics,
            EqPresetNext,
            EqPresetPrev,
            EqReset,
            EqToggle,
            ConfirmYes,
            ConfirmNo,
            ConfirmToggle,
            DeleteItem,
            LoginWeb,
            LoginArlMode,
        ]
    }

    /// Human-readable action name for the keybindings UI/help.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        use Action::*;
        match self {
            AppQuit => "Quit application",
            AppDetach => "Detach (keep daemon)",
            AppBackground => "Send to background",
            HelpToggle => "Toggle help",
            SettingsToggle => "Toggle settings",
            InfoToggle => "Toggle info",
            EqualizerToggle => "Toggle equalizer",
            LyricsToggle => "Toggle lyrics",
            SearchFocus => "Focus search/filter",
            NavUp => "Navigate up",
            NavDown => "Navigate down",
            NavLeft => "Navigate left / previous category",
            NavRight => "Navigate right / next category",
            NavPageUp => "Page up",
            NavPageDown => "Page down",
            NavHome => "Jump to start",
            NavEnd => "Jump to end",
            Select => "Select / confirm",
            Back => "Back / close",
            NextTab => "Next tab",
            PrevTab => "Previous tab",
            NextCategory => "Next category",
            PrevCategory => "Previous category",
            PlaybackToggle => "Play / pause",
            PlaybackNext => "Next track",
            PlaybackPrev => "Previous track",
            PlaybackSeekForward => "Seek forward",
            PlaybackSeekBackward => "Seek backward",
            VolumeUp => "Volume up",
            VolumeDown => "Volume down",
            ShuffleToggle => "Toggle shuffle",
            RepeatCycle => "Cycle repeat",
            FlowStart => "Start Flow",
            ShuffleFavorites => "Shuffle favorites",
            ContextMenu => "Open context menu",
            PlayingContextMenu => "Menu for playing track",
            OpenAlbum => "Open album",
            OpenArtist => "Open artist",
            WaitingList => "Open waiting list",
            LikeToggle => "Toggle like/favorite",
            SortCycle => "Cycle sort order",
            DownloadOffline => "Download for offline",
            FilterStart => "Start filter",
            FollowLyrics => "Follow lyrics",
            TranslateLyrics => "Toggle lyrics translation",
            EqPresetNext => "Next EQ preset",
            EqPresetPrev => "Previous EQ preset",
            EqReset => "Reset EQ band/preamp",
            EqToggle => "Toggle EQ on/off",
            ConfirmYes => "Confirm: yes",
            ConfirmNo => "Confirm: no",
            ConfirmToggle => "Toggle confirmation choice",
            DeleteItem => "Delete/remove item",
            LoginWeb => "Web login",
            LoginArlMode => "ARL input mode",
        }
    }
}

/// A single key binding (key code + modifiers), representable as a readable string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    pub fn ctrl(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    pub fn key(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }

    pub fn ctrl_key(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::CONTROL)
    }

    pub fn matches(&self, event: KeyEvent) -> bool {
        // Normalize: for Char, compare the char; ignore KeyEventKind/state.
        // Modifier comparison: consider only CONTROL/ALT/SHIFT/SUPER bits relevant to binding.
        let ev_mods = event.modifiers
            & (KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SHIFT
                | KeyModifiers::SUPER);
        let bind_mods = self.modifiers
            & (KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SHIFT
                | KeyModifiers::SUPER);
        if ev_mods != bind_mods {
            return false;
        }
        match (&self.code, event.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => *a == b,
            (a, b) => *a == b,
        }
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }
        if self.modifiers.contains(KeyModifiers::SUPER) {
            parts.push("Super");
        }
        let key = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        };
        if parts.is_empty() {
            write!(f, "{key}")
        } else {
            write!(f, "{}+{key}", parts.join("+"))
        }
    }
}

impl std::str::FromStr for KeyBinding {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty key binding".into());
        }
        let mut modifiers = KeyModifiers::NONE;
        let mut key_part = s;
        // Parse modifiers prefix repeatedly (e.g. Ctrl+Shift+Left)
        loop {
            let lower = key_part.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("ctrl+") {
                modifiers |= KeyModifiers::CONTROL;
                key_part = &key_part[5..];
                let _ = rest;
                continue;
            }
            if let Some(rest) = lower.strip_prefix("alt+") {
                modifiers |= KeyModifiers::ALT;
                key_part = &key_part[4..];
                let _ = rest;
                continue;
            }
            if let Some(rest) = lower.strip_prefix("shift+") {
                modifiers |= KeyModifiers::SHIFT;
                key_part = &key_part[6..];
                let _ = rest;
                continue;
            }
            if let Some(rest) = lower.strip_prefix("super+") {
                modifiers |= KeyModifiers::SUPER;
                key_part = &key_part[6..];
                let _ = rest;
                continue;
            }
            break;
        }
        let code = match key_part.to_ascii_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            k if k.starts_with('f') && k.len() > 1 => {
                let n: u8 = k[1..].parse().map_err(|_| format!("invalid key: {s}"))?;
                KeyCode::F(n)
            }
            _ => {
                let chars: Vec<char> = key_part.chars().collect();
                if chars.len() == 1 {
                    KeyCode::Char(chars[0])
                } else {
                    return Err(format!("invalid key: {s}"));
                }
            }
        };
        Ok(KeyBinding { code, modifiers })
    }
}

impl Serialize for KeyBinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Complete keymap: every action mapped to an explicit list of bindings.
/// Stored on disk as a full file (no delta from defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(flatten)]
    pub bindings: BTreeMap<Action, Vec<KeyBinding>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Keymap {
    pub fn path() -> Option<PathBuf> {
        deezer_core::Config::dir().map(|d| d.join("keybindings.json"))
    }

    /// Full default bindings. This is the complete, explicit map written to disk.
    pub fn defaults() -> Self {
        use Action::*;
        let mut b: BTreeMap<Action, Vec<KeyBinding>> = BTreeMap::new();

        b.insert(AppQuit, vec![KeyBinding::ctrl('q')]);
        b.insert(
            AppDetach,
            vec![
                KeyBinding::ctrl('c'),
                KeyBinding::ctrl('z'),
                KeyBinding::ctrl('Z'),
            ],
        );
        b.insert(
            AppBackground,
            vec![KeyBinding::ctrl('z'), KeyBinding::ctrl('Z')],
        );
        b.insert(HelpToggle, vec![KeyBinding::char('?')]);
        b.insert(SettingsToggle, vec![KeyBinding::ctrl('o')]);
        b.insert(InfoToggle, vec![KeyBinding::char('i')]);
        b.insert(EqualizerToggle, vec![KeyBinding::char('e')]);
        b.insert(LyricsToggle, vec![KeyBinding::char('m')]);

        b.insert(
            SearchFocus,
            vec![KeyBinding::char('/'), KeyBinding::ctrl('f')],
        );

        b.insert(NavUp, vec![KeyBinding::key(KeyCode::Up)]);
        b.insert(NavDown, vec![KeyBinding::key(KeyCode::Down)]);
        b.insert(NavLeft, vec![KeyBinding::key(KeyCode::Left)]);
        b.insert(NavRight, vec![KeyBinding::key(KeyCode::Right)]);
        b.insert(NavPageUp, vec![KeyBinding::key(KeyCode::PageUp)]);
        b.insert(NavPageDown, vec![KeyBinding::key(KeyCode::PageDown)]);
        b.insert(NavHome, vec![KeyBinding::key(KeyCode::Home)]);
        b.insert(NavEnd, vec![KeyBinding::key(KeyCode::End)]);
        b.insert(Select, vec![KeyBinding::key(KeyCode::Enter)]);
        b.insert(Back, vec![KeyBinding::key(KeyCode::Esc)]);
        b.insert(NextTab, vec![KeyBinding::key(KeyCode::Tab)]);
        b.insert(PrevTab, vec![KeyBinding::key(KeyCode::BackTab)]);
        // Category navigation aliases (also covered by NavLeft/NavRight in many views)
        b.insert(NextCategory, vec![KeyBinding::key(KeyCode::Right)]);
        b.insert(PrevCategory, vec![KeyBinding::key(KeyCode::Left)]);

        b.insert(PlaybackToggle, vec![KeyBinding::key(KeyCode::Char(' '))]);
        b.insert(PlaybackNext, vec![KeyBinding::char('n')]);
        b.insert(PlaybackPrev, vec![KeyBinding::char('b')]);
        b.insert(
            PlaybackSeekForward,
            vec![KeyBinding::ctrl_key(KeyCode::Right)],
        );
        b.insert(
            PlaybackSeekBackward,
            vec![KeyBinding::ctrl_key(KeyCode::Left)],
        );
        b.insert(VolumeUp, vec![KeyBinding::char('+'), KeyBinding::char('=')]);
        b.insert(VolumeDown, vec![KeyBinding::char('-')]);
        b.insert(ShuffleToggle, vec![KeyBinding::char('s')]);
        b.insert(RepeatCycle, vec![KeyBinding::char('r')]);
        b.insert(FlowStart, vec![KeyBinding::char('f')]);
        b.insert(ShuffleFavorites, vec![KeyBinding::char('g')]);

        b.insert(ContextMenu, vec![KeyBinding::char('x')]);
        b.insert(
            PlayingContextMenu,
            vec![KeyBinding::new(KeyCode::Char(' '), KeyModifiers::CONTROL)],
        );
        b.insert(OpenAlbum, vec![KeyBinding::char('a')]);
        b.insert(OpenArtist, vec![KeyBinding::char('t')]);
        b.insert(WaitingList, vec![KeyBinding::char('w')]);
        b.insert(
            LikeToggle,
            vec![
                KeyBinding::char('L'),
                KeyBinding::char('l'),
                KeyBinding::ctrl('l'),
            ],
        );

        b.insert(SortCycle, vec![KeyBinding::char('o')]);
        b.insert(DownloadOffline, vec![KeyBinding::char('d')]);
        b.insert(FilterStart, vec![KeyBinding::char('/')]);
        b.insert(FollowLyrics, vec![KeyBinding::char('f')]);
        b.insert(TranslateLyrics, vec![KeyBinding::char('t')]);
        b.insert(EqPresetNext, vec![KeyBinding::char('p')]);
        b.insert(EqPresetPrev, vec![KeyBinding::char('P')]);
        b.insert(EqReset, vec![KeyBinding::char('0')]);
        b.insert(
            EqToggle,
            vec![
                KeyBinding::key(KeyCode::Char(' ')),
                KeyBinding::key(KeyCode::Enter),
            ],
        );
        b.insert(
            ConfirmYes,
            vec![KeyBinding::char('y'), KeyBinding::char('Y')],
        );
        b.insert(
            ConfirmNo,
            vec![KeyBinding::char('n'), KeyBinding::char('N')],
        );
        b.insert(
            ConfirmToggle,
            vec![
                KeyBinding::key(KeyCode::Tab),
                KeyBinding::key(KeyCode::Left),
                KeyBinding::key(KeyCode::Right),
                KeyBinding::char('h'),
                KeyBinding::char('l'),
            ],
        );
        b.insert(
            DeleteItem,
            vec![KeyBinding::key(KeyCode::Delete), KeyBinding::char('d')],
        );

        b.insert(LoginWeb, vec![KeyBinding::key(KeyCode::Enter)]);
        b.insert(LoginArlMode, vec![KeyBinding::char('w')]);

        // Ensure completeness over all actions (in case any was missed above).
        for action in Action::all() {
            b.entry(*action).or_default();
        }

        Self { bindings: b }
    }

    /// Defaults adjusted for vim navigation mode.
    pub fn defaults_with_vim(vim_keys: bool) -> Self {
        let mut km = Self::defaults();
        km.apply_vim_keys(vim_keys);
        km
    }

    /// Apply/remove vim-style navigation bindings in-place.
    /// Edits nav actions and resolves the `l` conflict with LikeToggle.
    pub fn apply_vim_keys(&mut self, vim_keys: bool) {
        use Action::*;
        let defaults = Self::defaults();
        for action in [NavUp, NavDown, NavLeft, NavRight, LikeToggle] {
            if let Some(def) = defaults.bindings.get(&action) {
                self.bindings.insert(action, def.clone());
            }
        }
        if vim_keys {
            self.bindings
                .entry(NavUp)
                .or_default()
                .push(KeyBinding::char('k'));
            self.bindings
                .entry(NavDown)
                .or_default()
                .push(KeyBinding::char('j'));
            self.bindings
                .entry(NavLeft)
                .or_default()
                .push(KeyBinding::char('h'));
            self.bindings
                .entry(NavRight)
                .or_default()
                .push(KeyBinding::char('l'));
            // Avoid conflict: plain 'l' becomes nav-right in vim mode.
            if let Some(v) = self.bindings.get_mut(&LikeToggle) {
                v.retain(|b| !(b.code == KeyCode::Char('l') && b.modifiers.is_empty()));
            }
        }
    }

    /// Load the keymap from disk. Always results in a complete, explicit map on disk:
    /// - missing file -> write full defaults
    /// - existing file missing actions -> fill with defaults and save
    /// - parse error -> fall back to defaults in memory (keep the user's file as-is)
    pub fn load(vim_keys: bool) -> Self {
        if cfg!(test) {
            return Self::defaults_with_vim(vim_keys);
        }
        let Some(path) = Self::path() else {
            return Self::defaults_with_vim(vim_keys);
        };
        if !path.exists() {
            let km = Self::defaults_with_vim(vim_keys);
            let _ = km.save_to(&path);
            return km;
        }
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Keymap>(&content) {
                Ok(mut km) => {
                    let mut changed = false;
                    let defaults = Self::defaults_with_vim(vim_keys);
                    for action in Action::all() {
                        if !km.bindings.contains_key(action) {
                            if let Some(def) = defaults.bindings.get(action) {
                                km.bindings.insert(*action, def.clone());
                            } else {
                                km.bindings.insert(*action, Vec::new());
                            }
                            changed = true;
                        }
                    }
                    if changed {
                        let _ = km.save_to(&path);
                    }
                    km
                }
                Err(_) => Self::defaults_with_vim(vim_keys),
            },
            Err(_) => Self::defaults_with_vim(vim_keys),
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<(), std::io::Error> {
        if cfg!(test) {
            return Ok(());
        }
        let path = Self::path()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        fs::write(&tmp, format!("{content}\n"))
            .and_then(|()| fs::rename(&tmp, path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&tmp);
            })
    }

    pub fn matches(&self, action: Action, event: KeyEvent) -> bool {
        self.bindings
            .get(&action)
            .map(|v| v.iter().any(|b| b.matches(event)))
            .unwrap_or(false)
    }

    /// Returns the first binding label for display, or empty string if unbound.
    #[allow(dead_code)]
    pub fn label(&self, action: Action) -> String {
        self.bindings
            .get(&action)
            .and_then(|v| v.first())
            .map(|b| b.to_string())
            .unwrap_or_default()
    }

    /// Returns all binding labels joined for display.
    #[allow(dead_code)]
    pub fn labels(&self, action: Action) -> String {
        self.bindings
            .get(&action)
            .map(|v| {
                v.iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }
}
