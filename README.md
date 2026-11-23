# Deezer TUI

[![CircleCI](https://dl.circleci.com/status-badge/img/gh/Tatayoyoh/deezer-tui/tree/main.svg?style=shield)](https://dl.circleci.com/status-badge/redirect/gh/Tatayoyoh/deezer-tui/tree/main)
[![Release](https://img.shields.io/github/v/release/YoannDev90/deezer-tui)](https://github.com/YoannDev90/deezer-tui/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-1.90%2B-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20WSL2-blue.svg)]()
[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)

<div align="center">
    <img src="assets/logo.png" alt="Deezer TUI logo"/>
    <br>
    <h3>Bored of using 300 MB of RAM to play music?</h3>
    <div align="center">
        For developers ❤️<br>
        Easy account login<br>
        Low memory footprint<br>
        Plays in the background<br>
        Compliant with Deezer features<br>
    </div>
</div>

## Install

### Linux / macOS — one-liner

```bash
curl -LsSf https://raw.githubusercontent.com/YoannDev90/deezer-tui/main/install.sh | sh
```

Drops the binary in `~/.local/bin` (Linux x86_64 and aarch64).

### Or copy the binary yourself

```bash
wget -qO deezer-tui "https://github.com/YoannDev90/deezer-tui/releases/latest/download/deezer-tui-linux-x86_64"
chmod +x deezer-tui
mkdir -p ~/.local/bin && mv deezer-tui ~/.local/bin/
```

Binaries live on the [releases page](https://github.com/YoannDev90/deezer-tui/releases/latest) (`linux-x86_64`, `linux-aarch64`).

### macOS and Windows

* **macOS**: no binary published here yet — [build from source](#build-on-your-system), or grab a `macos-universal` build from the [upstream releases](https://github.com/Tatayoyoh/deezer-tui/releases/latest).
* **Windows**: use WSL2 with the Linux binary (the player needs Unix sockets and `fork`).

## Features

**Playback**
* Background player — a small daemon keeps playing while the TUI is closed or detached
* Audio quality selection (MP3 64 / 128 / 320, FLAC)
* 10-band biquad equalizer [`e`]
* Crossfade between tracks — Off to 12 s, from the settings menu
* Deezer Flow, shuffle, repeat, seeking
* Offline mode with downloaded tracks
* MPRIS support on Linux desktops: play / next / previous from your media keys

**Interface**
* Login through deezer.com
* Themes, straight from the official Deezer themes
* Album / artist miniatures (real images need Kitty or Ghostty)
* Live lyrics [`m`] — current line highlighted in time with the track
* Mouse events, dynamic terminal window/tab title
* Translations: EN, FR, ES, PT, DE

**Power user**
* A lot of [shortcuts](#useful-shortcuts) [`?`]
* CLI controls & status line (`--status`, `--json`, `--volume`, …) for tmux, Waybar, Polybar and scripts
* Shell completions for Bash, Zsh and Fish (`--completions <shell>`)
* Auto update

## In action

Simple search
![search](assets/search.gif)

Theme selection
![themes](assets/themes.gif)

Artist page
![artist](assets/artist.gif)

## Useful shortcuts

Press [`?`] in the app for the full, localized list.

**Navigation**

| Keys | Action |
|------|--------|
| `Tab` / `Shift+Tab` | Switch tabs |
| `h` / `l` or `←` / `→` | Switch category within the tab |
| `j` / `k` or `↑` / `↓` | Navigate the list |
| `Home` / `End`, `PgUp` / `PgDn` | First / last item, page up / down |
| `/` or `Ctrl+F` | Search mode |
| `Enter` / `Esc` | Submit or open / close overlay |

**Playback**

| Keys | Action |
|------|--------|
| `Space` | Play / pause |
| `n` / `b` | Next / previous track |
| `Ctrl+→` / `Ctrl+←` | Seek forward / backward |
| `s` / `r` | Toggle shuffle / cycle repeat |
| `+` / `-` | Volume up / down |
| `L` (or `Ctrl+L`) | Like / favorite |

**Menus & actions**

| Keys | Action |
|------|--------|
| `m` | Live lyrics |
| `x` | Context menu of the focused track |
| `Ctrl+Space` | Menu of the playing track |
| `a` / `t` | Album page / artist page of the focused track |
| `w` | Waiting list (queue) |
| `e` | Equalizer |
| `f` / `g` | Deezer Flow / shuffle play favorites |
| `Ctrl+O` | Settings (themes, crossfade) |
| `i` | Application info |
| `Ctrl+Z` | Background mode (keep playing, detach) |
| `Ctrl+Q` | Quit |

## CLI controls & status line

The TUI is a thin client over a background daemon, so you can drive it from anywhere:

```bash
deezer-tui --status              # now playing, plain text
deezer-tui --status --json       # same, JSON (Waybar / Polybar / scripts)
deezer-tui --toggle              # play / pause        (-p)
deezer-tui --next                # next track          (-n)
deezer-tui --prev                # previous track      (-b)
deezer-tui --volume 75           # set volume (0-100)
deezer-tui --seek 90             # seek to 1:30
deezer-tui --like                # favorite the playing track
deezer-tui --completions fish    # bash, zsh or fish
deezer-tui --quit                # stop the daemon     (-q)
```

Run `deezer-tui --help` for the full list (`--play`, `--pause`, `--stop`, `--volume-up`, `--seek-forward`, `--shuffle`, `--repeat`, `--dislike`, …).

## Build on your system

First install a Rust toolchain and the audio headers:

```bash
# Debian / Ubuntu
sudo apt install pkg-config libasound2-dev

# Arch
sudo pacman -S pkgconf alsa-lib

# Fedora
sudo dnf install pkgconf alsa-lib-devel

curl https://sh.rustup.rs -sSf | sh
source ~/.bashrc
```

Then:

```bash
cargo build --release
./target/release/deezer-tui
```

> **Build note:** `openssl-sys` only supports OpenSSL up to 3.x. On distributions already on OpenSSL 4 (Arch with `openssl` 4.x, …), point it at a 3.x install, e.g. `PKG_CONFIG_PATH=/usr/lib/openssl-3/pkgconfig cargo build --release` with the `openssl-3` package installed.

## Made with our brave Claude Code

And driven by human good ideas.

To be honest, I am not a Rust developer :p. Rust was a good match for this project 👍.

[Caveman](https://github.com/juliusbrussee/caveman) is used to compress the CLAUDE.md prompt, saving some session tokens.

This repository is a fork of [Tatayoyoh/deezer-tui](https://github.com/Tatayoyoh/deezer-tui) — the original project, its CI and its releases live there.

## Other good projects

Deezer players
* https://github.com/aunetx/deezer-linux - Deezer desktop app packaged into a webview
* https://github.com/yne/dzr - Deezer music from command line
* https://github.com/Minuga-RC/deezer-tui - another good TUI for Deezer
* https://gitlab.com/ColinDuquesnoy/MellowPlayer - Deezer desktop app packaged into a webview

Terminal audio players
* https://github.com/tramhao/termusic
* https://musikcube.com/
* https://github.com/timdubbins/tap
* https://github.com/ravachol/kew
* https://www.kariliq.nl/siren/
* https://github.com/raziman18/gomu
* https://github.com/dhulihan/grump
* https://github.com/Kingtous/RustPlayer

## Legal notice

Uses Deezer's undocumented private API for personal use. You need a valid Deezer account — this project is not affiliated with Deezer. The master decryption secret is **not** hardcoded: it is extracted at runtime from Deezer's public web resources, exactly like your browser does. Nothing facilitates piracy — audio is streamed, not saved.
