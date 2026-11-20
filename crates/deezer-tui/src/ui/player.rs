use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};
use ratatui_image::{Resize, StatefulImage};

use deezer_core::player::state::{PlaybackStatus, RepeatMode};

use crate::client::{ClickTarget, ViewState};
use crate::i18n::t;
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, view: &mut ViewState, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Theme::border())
        .style(Style::default().bg(Theme::surface()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Optional now-playing cover on the left (spans all content rows).
    let has_np_cover = view.now_playing_image.is_some();
    let (cover_area, content_area) = if has_np_cover && inner.width > 24 {
        // Keep the cover roughly square (terminal cells are ~2:1 height:width).
        // A larger area greatly improves clarity, especially with halfblock fallback.
        let cover_w = (inner.height.saturating_mul(2))
            .min(18)
            .min(inner.width.saturating_sub(16));
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(cover_w),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(inner);
        (Some(cols[0]), cols[2])
    } else {
        (None, inner)
    };

    if let (Some(area), Some(proto)) = (cover_area, view.now_playing_image.as_mut()) {
        let image_widget = StatefulImage::<ratatui_image::protocol::StatefulProtocol>::default()
            .resize(Resize::Fit(None));
        frame.render_stateful_widget(image_widget, area, proto);
    }

    // Center the 3 text rows vertically within the (possibly taller) player bar.
    let text_h = 3u16.min(content_area.height);
    let vpad = content_area.height.saturating_sub(text_h) / 2;
    let text_area = Rect {
        x: content_area.x,
        y: content_area.y + vpad,
        width: content_area.width,
        height: text_h,
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Track info
            Constraint::Length(1), // Progress bar + volume
            Constraint::Length(1), // Controls + Flow button
        ])
        .split(text_area);

    // Track info line
    let status_icon = match view.status {
        PlaybackStatus::Playing => Span::styled("  >> ", Style::default().fg(Color::Green)),
        PlaybackStatus::Paused => Span::styled("  || ", Style::default().fg(Color::Yellow)),
        PlaybackStatus::Loading => Span::styled("  .. ", Style::default().fg(Color::Cyan)),
        PlaybackStatus::Stopped => Span::styled("  [] ", Theme::dim()),
    };

    let (track_left, track_right, heart_click_offset) = if let Some(ref track) = view.current_track
    {
        let is_fav = view.is_track_favorite(&track.track_id);
        let heart_span = if is_fav {
            Span::styled(
                "♥",
                Style::default()
                    .fg(Color::Rgb(255, 75, 100))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("♡", Theme::dim())
        };
        // Status icon (5) + Title (chars) + " " (1)
        let heart_x = 5 + track.title.chars().count() as u16 + 1;
        (
            Line::from(vec![
                status_icon,
                Span::styled(
                    &track.title,
                    Style::default()
                        .fg(Theme::text_color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                heart_span,
                Span::raw(" "),
                Span::styled("L", Theme::shortcut_key()),
                Span::styled(" - ", Theme::dim()),
                Span::styled(&track.artist, Style::default().fg(Theme::primary())),
                Span::styled(format!("  ({})", track.album), Theme::dim()),
            ]),
            Line::from(Span::styled(
                format!("[{}] ", view.quality.as_api_format()),
                Theme::dim(),
            )),
            Some(heart_x),
        )
    } else {
        (
            Line::from(vec![
                status_icon,
                Span::styled(t().no_track_loaded, Theme::dim()),
            ]),
            Line::default(),
            None,
        )
    };
    let quality_width = track_right.width() as u16;
    let track_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(quality_width)])
        .split(chunks[0]);
    frame.render_widget(Paragraph::new(track_left), track_chunks[0]);
    view.record_click(track_chunks[0], ClickTarget::CurrentTrack);
    if let Some(x_off) = heart_click_offset {
        let heart_start = track_chunks[0].x + x_off;
        if heart_start < track_chunks[0].right() {
            let click_x = heart_start.saturating_sub(1);
            let click_w = 4.min(track_chunks[0].right().saturating_sub(click_x));
            let heart_rect = Rect {
                x: click_x,
                y: track_chunks[0].y,
                width: click_w,
                height: 1,
            };
            view.record_click(heart_rect, ClickTarget::ToggleLikeCurrentTrack);
        }
    }
    frame.render_widget(
        Paragraph::new(track_right).alignment(Alignment::Right),
        track_chunks[1],
    );

    // Progress bar + volume (right)
    let s = t();
    let vol_pct = (view.volume * 100.0) as u8;
    let vol_rest = format!(" {}: {vol_pct}% ", s.vol);
    let vol_width = (1 + 3 + vol_rest.len()) as u16; // " " + "+/-" chip + rest

    let progress_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(vol_width)])
        .split(chunks[1]);

    let ratio = view.progress_percent().min(1.0);
    let time_label = view.format_position();
    let progress = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Theme::progress_fill())
                .bg(Theme::progress_bg()),
        )
        .ratio(ratio)
        .label(time_label);
    frame.render_widget(progress, progress_chunks[0]);
    view.record_click(progress_chunks[0], ClickTarget::CurrentTrack);

    let vol_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("+/-", Theme::shortcut_key()),
        Span::styled(vol_rest, Theme::dim()),
    ]);
    frame.render_widget(
        Paragraph::new(vol_line).alignment(Alignment::Right),
        progress_chunks[1],
    );

    // Controls line + Flow button (right)
    let shuffle_style = if view.shuffle {
        Style::default()
            .fg(Theme::primary())
            .add_modifier(Modifier::BOLD)
    } else {
        Theme::dim()
    };
    let (repeat_label, repeat_style) = match view.repeat {
        RepeatMode::Off => (s.repeat, Theme::dim()),
        RepeatMode::Queue => (
            s.repeat_all,
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
        RepeatMode::Track => (
            s.repeat_one,
            Style::default()
                .fg(Theme::primary())
                .add_modifier(Modifier::BOLD),
        ),
    };

    // Shortcut keys render as a white "chip" (no brackets, no padding); labels
    // have no bg.
    let key = Theme::shortcut_key();
    let controls_left = Line::from(vec![
        Span::styled("  ", Theme::dim()),
        Span::styled("?", key),
        Span::styled(format!(" {}  ", s.help), Theme::dim()),
        Span::styled("Space", key),
        Span::styled(format!(" {}  ", s.play_pause), Theme::dim()),
        Span::styled("n", key),
        Span::styled(format!(" {}  ", s.next), Theme::dim()),
        Span::styled("b", key),
        Span::styled(format!(" {}  ", s.prev), Theme::dim()),
        Span::styled("s", key),
        Span::styled(format!(" {}  ", s.shuffle), shuffle_style),
        Span::styled("r", key),
        Span::styled(format!(" {}  ", repeat_label), repeat_style),
    ]);

    // Flow is special: the whole "f Flow" chip is white on a secondary bg,
    // with only the "f" key underlined.
    let flow_style = Theme::shortcut_flow_chip();
    let flow_label = format!(" f {} ", s.flow);
    let flow_width = flow_label.chars().count() as u16;
    let controls_right = Line::from(vec![
        Span::styled(" ", flow_style),
        Span::styled("f", flow_style.add_modifier(Modifier::UNDERLINED)),
        Span::styled(format!(" {} ", s.flow), flow_style),
    ]);

    let ctrl_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(flow_width)])
        .split(chunks[2]);
    frame.render_widget(Paragraph::new(controls_left), ctrl_chunks[0]);
    frame.render_widget(
        Paragraph::new(controls_right).alignment(Alignment::Right),
        ctrl_chunks[1],
    );
    view.record_click(ctrl_chunks[1], ClickTarget::FlowChip);
}
