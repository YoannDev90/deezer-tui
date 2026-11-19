use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use deezer_core::api::models::{
    AlbumDetail, ArtistDetail, ArtistSubTab, AudioQuality, DisplayItem, GenreDetail, Lyrics,
    PlaylistData, PlaylistDetail, TrackData,
};
use deezer_core::offline::OfflineTrack;
use deezer_core::player::eq::EqSettings;
use deezer_core::player::state::{PlaybackStatus, RepeatMode};

/// Commands sent from the TUI client to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Request a full state snapshot.
    GetSnapshot,
    /// Login with ARL token.
    Login { arl: String },
    /// Search for tracks.
    Search { query: String },
    /// Play track at index from search results.
    PlayFromSearch { index: usize },
    /// Play track at index from favorites.
    PlayFromFavorites { index: usize },
    /// Toggle play/pause.
    TogglePause,
    /// Next track in queue.
    NextTrack,
    /// Previous track in queue.
    PrevTrack,
    /// Set volume (0.0 - 1.0).
    SetVolume { volume: f32 },
    /// Seek forward by a number of seconds in the current track.
    SeekForward { secs: u64 },
    /// Seek backward by a number of seconds in the current track.
    SeekBackward { secs: u64 },
    /// Seek to an absolute position (seconds) in the current track.
    SeekAbsolute { secs: u64 },
    /// Stop playback entirely (clears current track, resets position).
    Stop,
    /// Toggle shuffle mode.
    ToggleShuffle,
    /// Cycle repeat mode (Off -> Queue -> Track -> Off).
    CycleRepeat,
    /// Load favorites from Deezer.
    LoadFavorites,
    /// Navigate list selection up.
    SelectUp,
    /// Navigate list selection down.
    SelectDown,
    /// Switch to next tab.
    NextTab,
    /// Switch to previous tab.
    PrevTab,
    /// Switch to next category within current tab.
    NextCategory,
    /// Switch to previous category within current tab.
    PrevCategory,
    /// Switch directly to a tab (mouse click on the tab bar).
    SetTab { tab: ActiveTab },
    /// Switch directly to a category of the current tab, by index into its
    /// `ALL` list (mouse click on the category menu).
    SetCategory { index: usize },
    /// Set the current tab's list selection directly (mouse click on a row).
    SelectIndex { index: usize },
    /// Play favorites in shuffle mode.
    ShuffleFavorites,
    /// Add a track to favorites.
    AddFavorite { track_id: String },
    /// Remove a track from favorites.
    RemoveFavorite { track_id: String },
    /// Add an artist to favorites.
    AddFavoriteArtist { artist_id: String },
    /// Remove an artist from favorites.
    RemoveFavoriteArtist { artist_id: String },
    /// Add an album to favorites.
    AddFavoriteAlbum { album_id: String },
    /// Remove an album from favorites.
    RemoveFavoriteAlbum { album_id: String },
    /// Request the user's playlists (for playlist picker).
    RequestPlaylists,
    /// Add a track to a playlist.
    AddToPlaylist {
        playlist_id: String,
        track_id: String,
    },
    /// Remove a track from a playlist (requires user to own the playlist).
    RemoveFromPlaylist {
        playlist_id: String,
        track_id: String,
    },
    /// Create a new playlist and add a track to it.
    CreatePlaylistAndAdd { title: String, track_id: String },
    /// Rename an existing playlist.
    RenamePlaylist {
        playlist_id: String,
        new_title: String,
    },
    /// Delete a playlist (owner only).
    DeletePlaylist { playlist_id: String },
    /// Mark a track as disliked (don't recommend).
    DislikeTrack { track_id: String },
    /// Insert a track to play next in the queue.
    PlayNext { track: TrackData },
    /// Append a track to the end of the queue.
    AddToQueue { track: TrackData },
    /// Start a mix inspired by a track.
    StartMix { track_id: String },
    /// Start Deezer Flow (personalized radio).
    StartFlow,
    /// Jump to and play a specific track in the queue by index.
    PlayFromQueue { index: usize },
    /// Remove a track from the queue by index.
    RemoveFromQueue { index: usize },
    /// Load album detail (tracks, metadata).
    GetAlbumDetail { album_id: String },
    /// Play a track from the album detail view.
    PlayFromAlbum { index: usize },
    /// Load artist detail (top tracks, albums).
    GetArtistDetail { artist_id: String },
    /// Play a track from the artist detail top tracks.
    PlayFromArtist { index: usize },
    /// Open an album from the artist detail albums list.
    OpenArtistAlbum { index: usize },
    /// Load playlist detail (tracks, metadata).
    GetPlaylistDetail { playlist_id: String },
    /// Play a track from the playlist detail view. `order` is the playlist's
    /// display order (track indices) when the user sorted it, so the queue
    /// follows what is on screen; empty means the playlist's own order.
    PlayFromPlaylist {
        index: usize,
        #[serde(default)]
        order: Vec<usize>,
    },
    /// Load a podcast show's episode list.
    GetShowDetail { show_id: String },
    /// Logout — clear ARL, return to login screen.
    Logout,
    /// Load radio stations from Deezer.
    LoadRadios,
    /// Play the selected radio station's tracks.
    PlayFromRadio { index: usize },
    /// Load the mood channels list from Deezer.
    LoadMoods,
    /// Play the selected mood (resolves tracks via the API).
    PlayFromMood { index: usize },
    /// Load music genres/categories from Deezer.
    LoadGenres,
    /// Load the detail (chart + radios) of a specific genre.
    GetGenreDetail { genre_id: u64, name: String },
    /// Play a track from the genre detail Tracks sub-tab.
    PlayFromGenreTrack { index: usize },
    /// Play a radio from the genre detail Radios sub-tab.
    PlayFromGenreRadio { index: usize },
    /// Download a track for offline mode.
    DownloadOffline { track: TrackData },
    /// Download an entire album for offline mode.
    DownloadAlbumOffline { album_id: String },
    /// Download an entire playlist for offline mode.
    DownloadPlaylistOffline { playlist_id: String },
    /// Remove a track from offline storage.
    RemoveOfflineTrack { track_id: String },
    /// Remove an album from offline storage.
    RemoveOfflineAlbum { album_id: String },
    /// Remove a playlist from offline storage.
    RemoveOfflinePlaylist { playlist_id: String },
    /// Play a track from offline storage.
    PlayFromOffline { index: usize },
    /// Play a track from an offline album (queue = album tracks).
    PlayOfflineAlbum {
        album_id: String,
        track_index: usize,
    },
    /// Play a track from an offline playlist (queue = playlist tracks).
    PlayOfflinePlaylist {
        playlist_id: String,
        track_index: usize,
    },
    /// Push a navigation overlay onto the daemon-side stack.
    PushNavOverlay(NavOverlay),
    /// Pop the top navigation overlay from the daemon-side stack.
    PopNavOverlay,
    /// Clear the entire navigation overlay stack (e.g. when entering a new top-level detail).
    ClearNavOverlayStack,
    /// Change the preferred audio quality (persisted to config). Takes effect on next track.
    SetQuality { quality: AudioQuality },
    /// Replace the equalizer settings (applied live, persisted to config).
    SetEqualizer { settings: EqSettings },
    /// Fetch the lyrics of the currently playing track.
    RequestLyrics,
    /// Graceful shutdown — daemon exits.
    Shutdown,
}

/// Navigation overlays that are persisted in the daemon across client reconnections.
/// Only content-detail views that correspond to loaded daemon data belong here.
/// Pure UI overlays (Help, Settings, WaitingList, etc.) stay client-side only.
// Variant names are wire format: renaming one would break IPC with older
// daemons/clients, so the shared `Detail` postfix stays.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavOverlay {
    ArtistDetail,
    AlbumDetail { from_artist: bool },
    PlaylistDetail,
    GenreDetail,
}

/// Sub-categories within the GenreDetail overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GenreDetailSubTab {
    #[default]
    Tracks,
    Albums,
    Artists,
    Playlists,
    Radios,
}

impl GenreDetailSubTab {
    pub const ALL: [Self; 5] = [
        Self::Tracks,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
        Self::Radios,
    ];

    pub fn next(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(&self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| c == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

/// Messages sent from the daemon to the TUI client.
// The snapshot variant is ~1.6 KB, but a message is built once, serialized
// immediately and never cloned in a loop — boxing it would only add an
// allocation on the hot path for no gain.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Full state snapshot.
    Snapshot(DaemonSnapshot),
    /// Only the fields that move several times a second while a track plays.
    /// Sent instead of a full snapshot when nothing else changed, so playback
    /// costs ~60 bytes per tick instead of ~111 KB.
    Tick(PlaybackTick),
    /// Error message.
    Error(String),
}

/// The volatile slice of the snapshot: everything that changes on every tick
/// during playback. The daemon sends this alone when the rest is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlaybackTick {
    #[serde(default)]
    pub status: PlaybackStatus,
    #[serde(default)]
    pub position_secs: u64,
    #[serde(default)]
    pub position_ms: u64,
    #[serde(default = "default_volume")]
    pub volume: f32,
}

/// Screen the daemon is on (login vs main).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Screen {
    #[default]
    Login,
    Main,
}

/// Active tab in main view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ActiveTab {
    #[default]
    Search,
    Favorites,
    Explore,
    Downloads,
}

/// Sub-categories within the Explore tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExploreCategory {
    #[default]
    Moods,
    Categories,
    Radios,
}

impl ExploreCategory {
    pub const ALL: [Self; 3] = [Self::Moods, Self::Categories, Self::Radios];
}

/// A music genre/category for display.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenreItem {
    pub id: u64,
    pub name: String,
}

/// Search category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchCategory {
    /// Mixed results of every type (top result first).
    #[default]
    All,
    Track,
    Artist,
    Album,
    Playlist,
    Podcast,
    Episode,
    Profile,
}

impl SearchCategory {
    pub const ALL: [Self; 8] = [
        Self::All,
        Self::Track,
        Self::Artist,
        Self::Album,
        Self::Playlist,
        Self::Podcast,
        Self::Episode,
        Self::Profile,
    ];

    /// API section key used in deezer.pageSearch response.
    pub fn api_key(&self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Track => "TRACK",
            Self::Artist => "ARTIST",
            Self::Album => "ALBUM",
            Self::Playlist => "PLAYLIST",
            Self::Podcast => "SHOW",
            Self::Episode => "EPISODE",
            Self::Profile => "USER",
        }
    }
}

/// Favorites category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FavoritesCategory {
    #[default]
    RecentlyPlayed,
    Tracks,
    Artists,
    Albums,
    Playlists,
    Following,
}

impl FavoritesCategory {
    pub const ALL: [Self; 6] = [
        Self::RecentlyPlayed,
        Self::Tracks,
        Self::Artists,
        Self::Albums,
        Self::Playlists,
        Self::Following,
    ];
}

/// Offline content category filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OfflineCategory {
    #[default]
    Tracks,
    Albums,
    Playlists,
}

impl OfflineCategory {
    pub const ALL: [Self; 3] = [Self::Tracks, Self::Albums, Self::Playlists];
}

/// A radio station item for display.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RadioItem {
    pub id: u64,
    pub title: String,
}

/// A mood channel item for display in the Explore > Moods tab.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoodEntry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub radio_id: Option<u64>,
}

/// Complete state snapshot sent from daemon to client.
/// All fields use `#[serde(default)]` for forward/backward compatibility
/// when daemon and client are running different binary versions.
/// Lists of tabs other than `active_tab` are emptied before it goes on the
/// wire — see [`DaemonSnapshot::prune_inactive_tabs`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonSnapshot {
    #[serde(default)]
    pub screen: Screen,
    #[serde(default)]
    pub active_tab: ActiveTab,

    // Player state
    #[serde(default)]
    pub status: PlaybackStatus,
    #[serde(default)]
    pub current_track: Option<TrackData>,
    #[serde(default)]
    pub quality: AudioQuality,
    #[serde(default)]
    pub position_secs: u64,
    /// Millisecond-accurate position, for time-synced lyrics. Older daemons
    /// omit it, in which case the client derives it from `position_secs`.
    #[serde(default)]
    pub position_ms: u64,
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default)]
    pub repeat: RepeatMode,
    #[serde(default)]
    pub equalizer: EqSettings,

    /// Lyrics of the currently playing track, when the user has requested them
    /// (or when the daemon prefetched them). `None` means "not fetched yet".
    #[serde(default)]
    pub lyrics: Option<Lyrics>,
    /// Track ID the `lyrics` above belong to, so the client can tell a stale
    /// payload from the one it asked for.
    #[serde(default)]
    pub lyrics_track_id: Option<String>,
    /// True while a lyrics fetch is in flight.
    #[serde(default)]
    pub lyrics_loading: bool,

    // Queue
    #[serde(default)]
    pub queue: Vec<TrackData>,
    #[serde(default)]
    pub queue_index: usize,

    // Search
    //
    // No `search_results` here: `search_display` already carries every track
    // (`DisplayItem.track` embeds the full `TrackData`), and playback resolves
    // the daemon-side copy by index. Sending both duplicated the whole result
    // list on the wire.
    #[serde(default)]
    pub search_selected: usize,
    #[serde(default)]
    pub search_loading: bool,
    #[serde(default)]
    pub search_category: SearchCategory,
    #[serde(default)]
    pub search_display: Vec<DisplayItem>,

    // Favorites
    //
    // Same story as search: `favorites_display` carries the tracks (when the
    // category is a track list), so a separate `favorites: Vec<TrackData>` was
    // the same list serialized twice. The daemon keeps its own copy for
    // `PlayFromFavorites` / `ShuffleFavorites`.
    #[serde(default)]
    pub favorites_selected: usize,
    #[serde(default)]
    pub favorites_loading: bool,
    #[serde(default)]
    pub favorites_category: FavoritesCategory,
    #[serde(default)]
    pub favorites_display: Vec<DisplayItem>,
    /// IDs of tracks in the user's favorites (Loved Tracks).
    #[serde(default)]
    pub favorite_track_ids: Vec<String>,
    /// IDs of artists in the user's favorites.
    #[serde(default)]
    pub favorite_artist_ids: Vec<String>,
    /// IDs of albums in the user's favorites.
    #[serde(default)]
    pub favorite_album_ids: Vec<String>,

    // Offline
    #[serde(default)]
    pub offline_category: OfflineCategory,
    #[serde(default)]
    pub offline_tracks: Vec<OfflineTrack>,
    #[serde(default)]
    pub offline_albums: Vec<AlbumDetail>,
    #[serde(default)]
    pub offline_playlists: Vec<PlaylistDetail>,
    #[serde(default)]
    pub offline_selected: usize,
    #[serde(default)]
    pub offline_loading: bool,
    /// IDs of tracks available offline (for UI indicators in other tabs).
    #[serde(default)]
    pub offline_track_ids: Vec<String>,

    // Explore tab
    #[serde(default)]
    pub explore_category: ExploreCategory,

    // Radios
    #[serde(default)]
    pub radios: Vec<RadioItem>,
    #[serde(default)]
    pub radios_selected: usize,
    #[serde(default)]
    pub radios_loading: bool,

    // Moods
    #[serde(default)]
    pub moods: Vec<MoodEntry>,
    #[serde(default)]
    pub moods_selected: usize,
    #[serde(default)]
    pub moods_loading: bool,

    // Genres / Categories
    #[serde(default)]
    pub genres: Vec<GenreItem>,
    #[serde(default)]
    pub genres_selected: usize,
    #[serde(default)]
    pub genres_loading: bool,

    // Playlists (for playlist picker in popup menu)
    #[serde(default)]
    pub playlists: Vec<PlaylistData>,

    // Album detail
    #[serde(default)]
    pub album_detail: Option<AlbumDetail>,
    #[serde(default)]
    pub album_detail_selected: usize,
    #[serde(default)]
    pub album_detail_loading: bool,

    // Artist detail
    #[serde(default)]
    pub artist_detail: Option<ArtistDetail>,
    #[serde(default)]
    pub artist_detail_selected: usize,
    #[serde(default)]
    pub artist_detail_loading: bool,
    #[serde(default)]
    pub artist_detail_sub_tab: ArtistSubTab,

    // Playlist detail
    #[serde(default)]
    pub playlist_detail: Option<PlaylistDetail>,
    #[serde(default)]
    pub playlist_detail_selected: usize,
    #[serde(default)]
    pub playlist_detail_loading: bool,

    // Genre detail
    #[serde(default)]
    pub genre_detail: Option<GenreDetail>,
    #[serde(default)]
    pub genre_detail_loading: bool,

    // Navigation overlay stack (persisted across reconnections)
    #[serde(default)]
    pub nav_overlay: Option<NavOverlay>,
    #[serde(default)]
    pub nav_overlay_stack: Vec<NavOverlay>,

    // UI hints
    #[serde(default)]
    pub status_msg: Option<String>,
    #[serde(default)]
    pub login_error: Option<String>,
    #[serde(default)]
    pub login_loading: bool,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub is_offline: bool,
}

impl DaemonSnapshot {
    /// Drop the lists the active tab doesn't render, so a snapshot carries
    /// one tab's data instead of every tab's at once (radios alone are ~300
    /// entries, search results embed full tracks).
    ///
    /// This only shapes the wire: the daemon keeps its own copies for every
    /// command, and the lists come back in full on the next snapshot after a
    /// tab switch. Fields needed outside their tab are never touched —
    /// `favorite_*_ids` (hearts), `offline_track_ids` (download badges) and
    /// `playlists` (the picker popup opens from any tab) — and neither are
    /// the selection/loading flags, so switching back restores the tab as it
    /// was left.
    pub fn prune_inactive_tabs(&mut self) {
        if self.active_tab != ActiveTab::Search {
            self.search_display.clear();
        }
        if self.active_tab != ActiveTab::Favorites {
            self.favorites_display.clear();
        }
        match self.active_tab {
            ActiveTab::Explore => {
                // Only the sub-category on screen is rendered (ui/explore.rs
                // matches on it); switching category reloads the other lists.
                match self.explore_category {
                    ExploreCategory::Moods => {
                        self.radios.clear();
                        self.genres.clear();
                    }
                    ExploreCategory::Categories => {
                        self.radios.clear();
                        self.moods.clear();
                    }
                    ExploreCategory::Radios => {
                        self.moods.clear();
                        self.genres.clear();
                    }
                }
            }
            _ => {
                self.radios.clear();
                self.moods.clear();
                self.genres.clear();
            }
        }
        if self.active_tab != ActiveTab::Downloads {
            self.offline_tracks.clear();
            self.offline_albums.clear();
            self.offline_playlists.clear();
        }
    }
}

fn default_volume() -> f32 {
    0.8
}

impl Default for DaemonSnapshot {
    fn default() -> Self {
        Self {
            screen: Screen::Login,
            active_tab: ActiveTab::Search,
            status: PlaybackStatus::Stopped,
            current_track: None,
            quality: AudioQuality::Mp3_128,
            position_secs: 0,
            position_ms: 0,
            duration_secs: 0,
            volume: 0.8,
            shuffle: false,
            repeat: RepeatMode::Off,
            equalizer: EqSettings::default(),
            lyrics: None,
            lyrics_track_id: None,
            lyrics_loading: false,
            queue: Vec::new(),
            queue_index: 0,
            search_selected: 0,
            search_loading: false,
            search_category: SearchCategory::default(),
            search_display: Vec::new(),
            favorites_selected: 0,
            favorites_loading: false,
            favorites_category: FavoritesCategory::default(),
            favorites_display: Vec::new(),
            favorite_track_ids: Vec::new(),
            favorite_artist_ids: Vec::new(),
            favorite_album_ids: Vec::new(),
            offline_category: OfflineCategory::default(),
            offline_tracks: Vec::new(),
            offline_albums: Vec::new(),
            offline_playlists: Vec::new(),
            offline_selected: 0,
            offline_loading: false,
            offline_track_ids: Vec::new(),
            explore_category: ExploreCategory::default(),
            radios: Vec::new(),
            radios_selected: 0,
            radios_loading: false,
            moods: Vec::new(),
            moods_selected: 0,
            moods_loading: false,
            genres: Vec::new(),
            genres_selected: 0,
            genres_loading: false,
            playlists: Vec::new(),
            album_detail: None,
            album_detail_selected: 0,
            album_detail_loading: false,
            artist_detail: None,
            artist_detail_selected: 0,
            artist_detail_loading: false,
            artist_detail_sub_tab: ArtistSubTab::default(),
            playlist_detail: None,
            playlist_detail_selected: 0,
            playlist_detail_loading: false,
            genre_detail: None,
            genre_detail_loading: false,
            nav_overlay: None,
            nav_overlay_stack: Vec::new(),
            status_msg: None,
            login_error: None,
            login_loading: false,
            user_name: None,
            is_offline: false,
        }
    }
}

/// Get the Unix socket path for daemon IPC.
pub fn socket_path() -> PathBuf {
    deezer_core::Config::dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("daemon.sock")
}

/// Get the PID file path for the daemon process.
pub fn pid_path() -> PathBuf {
    deezer_core::Config::dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("daemon.pid")
}

/// Send a line-delimited JSON message over a Unix stream.
pub async fn send_line<T: Serialize>(stream: &mut UnixStream, msg: &T) -> std::io::Result<()> {
    let mut json = serde_json::to_string(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    json.push('\n');
    stream.write_all(json.as_bytes()).await?;
    stream.flush().await
}

/// Read a line-delimited JSON message from a buffered reader.
pub async fn read_line<T: for<'de> Deserialize<'de>, R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> std::io::Result<Option<T>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None); // EOF — peer disconnected
    }
    let msg = serde_json::from_str(&line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every snapshot field is `#[serde(default)]`: an empty payload must
    /// load — that is what lets a new client talk to an older daemon.
    #[test]
    fn snapshot_defaults_when_every_field_is_missing() {
        let snap: DaemonSnapshot = serde_json::from_value(json!({})).expect("empty snapshot");
        assert_eq!(snap.screen, Screen::default());
        assert_eq!(snap.active_tab, ActiveTab::default());
        assert_eq!(snap.status, PlaybackStatus::Stopped);
        assert_eq!(snap.position_ms, 0);
        assert!(snap.lyrics.is_none());
        assert!(snap.queue.is_empty());
        assert!(snap.nav_overlay.is_none());
    }

    /// A partial payload sets the fields it carries and defaults the rest —
    /// the shape an older daemon's messages take on a newer client.
    #[test]
    fn snapshot_takes_known_fields_and_defaults_the_others() {
        let snap: DaemonSnapshot = serde_json::from_value(json!({
            "status": "Playing",
            "position_secs": 42,
            "position_ms": 42_500,
            "nav_overlay": { "AlbumDetail": { "from_artist": true } },
            "nav_overlay_stack": ["PlaylistDetail"]
        }))
        .expect("partial snapshot");
        assert_eq!(snap.status, PlaybackStatus::Playing);
        assert_eq!(snap.position_secs, 42);
        assert_eq!(snap.position_ms, 42_500);
        assert_eq!(
            snap.nav_overlay,
            Some(NavOverlay::AlbumDetail { from_artist: true })
        );
        assert_eq!(snap.nav_overlay_stack, vec![NavOverlay::PlaylistDetail]);
        // Untouched fields keep their defaults rather than garbage.
        assert_eq!(snap.screen, Screen::default());
        assert!(snap.current_track.is_none());
    }

    /// A newer daemon sends fields an older client has never heard of: they
    /// must be ignored instead of failing the parse.
    #[test]
    fn snapshot_ignores_unknown_fields() {
        let snap: DaemonSnapshot = serde_json::from_value(json!({
            "field_from_the_future": { "anything": true },
            "lyrics": { "synced": [{ "at_ms": 500, "text": "x" }] }
        }))
        .expect("payload with unknown fields");
        // …including a `LyricLine` predating the `translated` field.
        assert_eq!(
            snap.lyrics
                .as_ref()
                .and_then(|l| l.synced.first())
                .map(|l| l.translated.as_deref()),
            Some(None)
        );
    }

    /// serialize → deserialize → serialize must be stable and preserve the
    /// values: this is the wire format every daemon/client pair agrees on.
    #[test]
    fn snapshot_round_trips_without_losing_or_inventing_fields() {
        let snap: DaemonSnapshot = serde_json::from_value(json!({
            "screen": "Main",
            "status": "Paused",
            "position_ms": 12_345,
            "volume": 0.75,
            "lyrics": {
                "synced": [{ "at_ms": 1000, "text": "la la", "translated": "lo lo" }],
                "plain": ["la la"],
                "writers": "W",
                "copyright": "C"
            },
            "lyrics_track_id": "3135556",
            "nav_overlay": { "AlbumDetail": { "from_artist": false } }
        }))
        .expect("populated snapshot");

        let once = serde_json::to_value(&snap).expect("serialize");
        let back: DaemonSnapshot = serde_json::from_value(once.clone()).expect("reparse");
        let twice = serde_json::to_value(&back).expect("reserialize");
        assert_eq!(once, twice);

        assert_eq!(back.screen, Screen::Main);
        assert_eq!(back.status, PlaybackStatus::Paused);
        assert_eq!(back.position_ms, 12_345);
        assert_eq!(back.volume, 0.75);
        let lyrics = back.lyrics.as_ref().expect("lyrics");
        assert_eq!(
            lyrics.synced[0].translated.as_deref(),
            Some("lo lo"),
            "the translated lyric field must survive the trip"
        );
    }

    /// The tick is its own message type with the same defaulting rules.
    #[test]
    fn tick_round_trips_and_defaults_missing_fields() {
        let tick: PlaybackTick =
            serde_json::from_value(json!({ "position_secs": 7 })).expect("partial tick");
        assert_eq!(tick.position_secs, 7);
        assert_eq!(tick.position_ms, 0);
        assert_eq!(tick.status, PlaybackStatus::Stopped);

        let once = serde_json::to_value(tick).expect("serialize");
        let back: PlaybackTick = serde_json::from_value(once.clone()).expect("reparse");
        assert_eq!(serde_json::to_value(back).expect("reserialize"), once);
    }

    /// A snapshot populated on every tab, to prune from.
    fn full_snapshot() -> DaemonSnapshot {
        let mut snap: DaemonSnapshot = serde_json::from_value(json!({})).expect("empty snapshot");
        let item = |name: &str| DisplayItem {
            col1: name.into(),
            col2: String::new(),
            col3: String::new(),
            col4: String::new(),
            track: None,
            album_id: None,
            playlist_id: None,
            artist_id: None,
            show_id: None,
            kind: None,
        };
        snap.search_display = vec![item("search")];
        snap.favorites_display = vec![item("favorite")];
        snap.radios = vec![RadioItem {
            id: 1,
            title: "radio".into(),
        }];
        snap.moods = vec![MoodEntry::default()];
        snap.genres = vec![GenreItem {
            id: 1,
            name: "genre".into(),
        }];
        let track: TrackData = serde_json::from_value(json!({
            "SNG_ID": "1", "SNG_TITLE": "t", "ART_NAME": "a", "DURATION": "10"
        }))
        .expect("track");
        snap.offline_tracks = vec![OfflineTrack {
            track,
            quality: AudioQuality::default(),
        }];
        snap.playlists = vec![PlaylistData {
            playlist_id: "1".into(),
            title: "playlist".into(),
            nb_songs: 1,
            author: String::new(),
            collaborative: false,
        }];
        snap.favorite_track_ids = vec!["10".into()];
        snap.offline_track_ids = vec!["20".into()];
        snap.search_selected = 3;
        snap
    }

    /// Only the active tab's lists ride the wire; the fields other tabs
    /// depend on (hearts, download badges, the picker) and the selection
    /// flags always do.
    #[test]
    fn prune_inactive_tabs_ships_only_the_active_tab() {
        let mut snap = full_snapshot();
        snap.active_tab = ActiveTab::Search;
        snap.prune_inactive_tabs();
        assert_eq!(snap.search_display.len(), 1);
        assert!(snap.favorites_display.is_empty());
        assert!(snap.radios.is_empty() && snap.moods.is_empty() && snap.genres.is_empty());
        assert!(snap.offline_tracks.is_empty());
        assert_eq!(snap.search_selected, 3, "selection flags ride along");
        assert_eq!(
            snap.favorite_track_ids,
            vec!["10".to_string()],
            "hearts cross tabs"
        );
        assert_eq!(
            snap.offline_track_ids,
            vec!["20".to_string()],
            "badges cross tabs"
        );
        assert_eq!(snap.playlists.len(), 1, "the picker opens from any tab");

        let mut snap = full_snapshot();
        snap.active_tab = ActiveTab::Favorites;
        snap.prune_inactive_tabs();
        assert_eq!(snap.favorites_display.len(), 1);
        assert!(snap.search_display.is_empty());

        // Explore ships only the sub-category on screen.
        for (category, kept) in [
            (ExploreCategory::Moods, 1),
            (ExploreCategory::Categories, 1),
            (ExploreCategory::Radios, 1),
        ] {
            let mut snap = full_snapshot();
            snap.active_tab = ActiveTab::Explore;
            snap.explore_category = category;
            snap.prune_inactive_tabs();
            let (radios, moods, genres) = (snap.radios.len(), snap.moods.len(), snap.genres.len());
            let shipped = radios + moods + genres;
            assert_eq!(shipped, kept, "{category:?} ships exactly one list");
            assert_eq!(
                match category {
                    ExploreCategory::Moods => moods,
                    ExploreCategory::Categories => genres,
                    ExploreCategory::Radios => radios,
                },
                1,
                "{category:?} keeps its own list"
            );
        }

        let mut snap = full_snapshot();
        snap.active_tab = ActiveTab::Downloads;
        snap.prune_inactive_tabs();
        assert_eq!(snap.offline_tracks.len(), 1);
        assert!(snap.radios.is_empty() && snap.search_display.is_empty());
    }
}
