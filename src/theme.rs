use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};

use crate::tmux::Tmux;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub border: Color,
    pub selection_background: Color,
    pub selection_foreground: Color,
    pub normal_mode_background: Color,
    pub normal_mode_foreground: Color,
    pub agent_processing_background: Color,
    pub agent_permission_background: Color,
    pub agent_input_background: Color,
    pub agent_ready_background: Color,
}

impl Theme {
    pub fn from_tmux(tmux: &Tmux) -> Self {
        let options = tmux
            .options(&[
                "status-style",
                "window-active-style",
                "mode-style",
                "pane-border-style",
                "pane-active-border-style",
                "status-left",
                "status-right",
                "window-status-format",
                "window-status-current-format",
                "message-style",
                "@enhance-color-background",
                "@enhance-color-foreground",
                "@enhance-color-accent",
                "@enhance-color-border",
                "@enhance-color-selection-background",
                "@enhance-color-selection-foreground",
                "@enhance-color-normal-mode-background",
                "@enhance-color-normal-mode-foreground",
                "@enhance-color-agent-processing",
                "@enhance-color-agent-permission",
                "@enhance-color-agent-input",
                "@enhance-color-agent-ready",
            ])
            .unwrap_or_default();
        Self::from_options(&options)
    }

    fn from_options(options: &HashMap<String, String>) -> Self {
        let option = |name: &str| options.get(name).map(String::as_str).unwrap_or_default();
        let status = option("status-style");
        let window = option("window-active-style");
        let mode = option("mode-style");
        let border = option("pane-border-style");
        let active_border = option("pane-active-border-style");
        let palette = status_palette(options);

        let foreground = style_color(status, "fg").unwrap_or(Color::White);
        let status_background = style_color(status, "bg").unwrap_or(Color::Reset);
        let background = style_color(window, "bg")
            .filter(|color| *color != Color::Reset)
            .unwrap_or_else(|| canvas_from_status(status_background, foreground));
        let accent = style_color(active_border, "fg").unwrap_or(Color::Cyan);
        let configured_border = style_color(border, "fg").unwrap_or(Color::DarkGray);

        let mut theme = Self {
            background,
            foreground,
            accent,
            border: if configured_border == background {
                accent
            } else {
                configured_border
            },
            selection_background: blend(background, accent, 28).unwrap_or(Color::DarkGray),
            selection_foreground: foreground,
            normal_mode_background: semantic_color(&palette, 30.0)
                .or_else(|| style_color(mode, "bg"))
                .unwrap_or(Color::Yellow),
            normal_mode_foreground: foreground,
            agent_processing_background: accent,
            agent_permission_background: semantic_color(&palette, 30.0)
                .or_else(|| style_color(mode, "bg"))
                .unwrap_or(Color::Rgb(229, 168, 75)),
            agent_input_background: semantic_color(&palette, 190.0)
                .unwrap_or(Color::Rgb(40, 120, 184)),
            agent_ready_background: semantic_color(&palette, 125.0)
                .unwrap_or(Color::Rgb(21, 128, 61)),
        };

        theme.background = override_color(options, "@enhance-color-background", theme.background);
        theme.foreground = override_color(options, "@enhance-color-foreground", theme.foreground);
        theme.accent = override_color(options, "@enhance-color-accent", theme.accent);
        theme.border = override_color(options, "@enhance-color-border", theme.border);
        theme.selection_background = override_color(
            options,
            "@enhance-color-selection-background",
            theme.selection_background,
        );
        theme.selection_foreground = override_color(
            options,
            "@enhance-color-selection-foreground",
            theme.selection_foreground,
        );
        theme.normal_mode_background = override_color(
            options,
            "@enhance-color-normal-mode-background",
            theme.normal_mode_background,
        );
        theme.normal_mode_foreground = override_color(
            options,
            "@enhance-color-normal-mode-foreground",
            theme.normal_mode_foreground,
        );
        theme.agent_processing_background =
            override_color(options, "@enhance-color-agent-processing", theme.accent);
        theme.agent_permission_background = override_color(
            options,
            "@enhance-color-agent-permission",
            theme.agent_permission_background,
        );
        theme.agent_input_background = override_color(
            options,
            "@enhance-color-agent-input",
            theme.agent_input_background,
        );
        theme.agent_ready_background = override_color(
            options,
            "@enhance-color-agent-ready",
            theme.agent_ready_background,
        );
        theme
    }

    pub fn base(self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    pub fn match_style(self) -> Style {
        Style::default()
            .fg(self.foreground)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text(self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub fn toast_style(self) -> String {
        format!(
            "fg={},bg={},bold",
            tmux_color(self.foreground),
            tmux_color(self.accent)
        )
    }

    pub fn picker_popup_styles(self) -> (String, String) {
        (
            format!(
                "fg={},bg={}",
                tmux_color(self.foreground),
                tmux_color(self.background)
            ),
            format!(
                "fg={},bg={}",
                tmux_color(self.border),
                tmux_color(self.background)
            ),
        )
    }

    pub fn agent_overlay_styles(self, status: &str) -> (String, String) {
        let background = match status {
            "permission" => self.agent_permission_background,
            "input" => self.agent_input_background,
            "ready" => self.agent_ready_background,
            _ => self.accent,
        };
        (
            format!(
                "fg={},bg={}",
                tmux_color(self.foreground),
                tmux_color(self.background)
            ),
            format!(
                "fg={},bg={},bold",
                tmux_color(background),
                tmux_color(self.background)
            ),
        )
    }
}

fn tmux_color(color: Color) -> String {
    match color {
        Color::Reset => "default".into(),
        Color::Black => "black".into(),
        Color::Red => "red".into(),
        Color::Green => "green".into(),
        Color::Yellow => "yellow".into(),
        Color::Blue => "blue".into(),
        Color::Magenta => "magenta".into(),
        Color::Cyan => "cyan".into(),
        Color::Gray => "white".into(),
        Color::DarkGray => "brightblack".into(),
        Color::LightRed => "brightred".into(),
        Color::LightGreen => "brightgreen".into(),
        Color::LightYellow => "brightyellow".into(),
        Color::LightBlue => "brightblue".into(),
        Color::LightMagenta => "brightmagenta".into(),
        Color::LightCyan => "brightcyan".into(),
        Color::White => "brightwhite".into(),
        Color::Indexed(index) => format!("colour{index}"),
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
    }
}

fn blend(background: Color, foreground: Color, foreground_percent: u16) -> Option<Color> {
    let (br, bg, bb) = rgb(background)?;
    let (fr, fg, fb) = rgb(foreground)?;
    let background_percent = 100 - foreground_percent;
    Some(Color::Rgb(
        ((u16::from(br) * background_percent + u16::from(fr) * foreground_percent) / 100) as u8,
        ((u16::from(bg) * background_percent + u16::from(fg) * foreground_percent) / 100) as u8,
        ((u16::from(bb) * background_percent + u16::from(fb) * foreground_percent) / 100) as u8,
    ))
}

fn status_palette(options: &HashMap<String, String>) -> Vec<Color> {
    [
        "status-left",
        "status-right",
        "status-style",
        "window-status-format",
        "window-status-current-format",
        "message-style",
        "mode-style",
        "pane-active-border-style",
    ]
    .into_iter()
    .filter_map(|option| options.get(option))
    .flat_map(|value| colors_in_format(value))
    .fold(Vec::new(), |mut colors, color| {
        if !colors.contains(&color) {
            colors.push(color);
        }
        colors
    })
}

fn colors_in_format(value: &str) -> Vec<Color> {
    ["fg=", "bg="]
        .into_iter()
        .flat_map(|marker| {
            value.match_indices(marker).filter_map(move |(index, _)| {
                let value = &value[index + marker.len()..];
                let end = value
                    .find(|character: char| {
                        character == ',' || character == ']' || character.is_whitespace()
                    })
                    .unwrap_or(value.len());
                parse_color(&value[..end])
            })
        })
        .collect()
}

fn semantic_color(palette: &[Color], target_hue: f32) -> Option<Color> {
    palette
        .iter()
        .copied()
        .min_by(|left, right| {
            semantic_color_score(*left, target_hue)
                .partial_cmp(&semantic_color_score(*right, target_hue))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|color| semantic_color_score(*color, target_hue) < 50.0)
}

fn semantic_color_score(color: Color, target_hue: f32) -> f32 {
    let Some((red, green, blue)) = rgb(color) else {
        return f32::MAX;
    };
    let red = f32::from(red) / 255.0;
    let green = f32::from(green) / 255.0;
    let blue = f32::from(blue) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let delta = maximum - minimum;
    if maximum == 0.0 || delta / maximum < 0.25 {
        return f32::MAX;
    }
    let hue = if delta == 0.0 {
        0.0
    } else if maximum == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let distance = (hue - target_hue).abs();
    distance.min(360.0 - distance)
}

fn canvas_from_status(background: Color, foreground: Color) -> Color {
    let Some((br, bg, bb)) = rgb(background) else {
        return background;
    };
    let Some((fr, fg, fb)) = rgb(foreground) else {
        return background;
    };
    let background_luma = luma(br, bg, bb);
    let foreground_luma = luma(fr, fg, fb);

    if foreground_luma <= background_luma || background_luma <= 56.0 {
        return background;
    }

    let scale = 44.0 / background_luma;
    Color::Rgb(
        (f32::from(br) * scale).round() as u8,
        (f32::from(bg) * scale).round() as u8,
        (f32::from(bb) * scale).round() as u8,
    )
}

fn luma(red: u8, green: u8, blue: u8) -> f32 {
    0.2126 * f32::from(red) + 0.7152 * f32::from(green) + 0.0722 * f32::from(blue)
}

fn rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(red, green, blue) => Some((red, green, blue)),
        _ => None,
    }
}

fn override_color(options: &HashMap<String, String>, option: &str, fallback: Color) -> Color {
    options
        .get(option)
        .and_then(|value| parse_color(value.trim()))
        .unwrap_or(fallback)
}

fn style_color(style: &str, property: &str) -> Option<Color> {
    style.split(',').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == property).then(|| parse_color(value)).flatten()
    })
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#')
        && hex.len() == 6
    {
        return Some(Color::Rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ));
    }
    if let Some(index) = value
        .strip_prefix("colour")
        .or_else(|| value.strip_prefix("color"))
        .and_then(|index| index.parse().ok())
    {
        return Some(Color::Indexed(index));
    }
    Some(match value.as_str() {
        "default" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "brightblack" => Color::DarkGray,
        "brightred" => Color::LightRed,
        "brightgreen" => Color::LightGreen,
        "brightyellow" => Color::LightYellow,
        "brightblue" => Color::LightBlue,
        "brightmagenta" => Color::LightMagenta,
        "brightcyan" => Color::LightCyan,
        "brightwhite" => Color::White,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_theme_from_a_fresh_option_snapshot() {
        let mut options = HashMap::from([
            ("status-style".into(), "bg=#44475a,fg=#f8f8f2".into()),
            ("pane-border-style".into(), "fg=#6272a4".into()),
            ("pane-active-border-style".into(), "fg=#bd93f9".into()),
            ("status-left".into(), "#[bg=#50fa7b] ready".into()),
            ("status-right".into(), "#[bg=#ffb86c] permission".into()),
            ("message-style".into(), "bg=#8be9fd".into()),
        ]);
        let theme = Theme::from_options(&options);
        assert_eq!(theme.background, Color::Rgb(42, 44, 55));
        assert_eq!(theme.foreground, Color::Rgb(248, 248, 242));
        assert_eq!(theme.border, Color::Rgb(98, 114, 164));
        assert_eq!(theme.accent, Color::Rgb(189, 147, 249));
        assert_eq!(theme.agent_ready_background, Color::Rgb(80, 250, 123));
        assert_eq!(theme.agent_permission_background, Color::Rgb(255, 184, 108));
        assert_eq!(theme.agent_input_background, Color::Rgb(139, 233, 253));

        options.insert("status-style".into(), "bg=#f5f5f5,fg=#1e1e1e".into());
        let light = Theme::from_options(&options);
        assert_eq!(light.background, Color::Rgb(245, 245, 245));
        assert_eq!(light.foreground, Color::Rgb(30, 30, 30));
    }

    #[test]
    fn snapshot_preserves_color_overrides_and_missing_option_defaults() {
        let options = HashMap::from([
            ("@enhance-color-background".into(), "#101010".into()),
            ("@enhance-color-accent".into(), "colour123".into()),
            ("@enhance-color-foreground".into(), "invalid".into()),
            ("@enhance-color-agent-ready".into(), "#123456".into()),
        ]);
        let theme = Theme::from_options(&options);
        assert_eq!(theme.background, Color::Rgb(16, 16, 16));
        assert_eq!(theme.accent, Color::Indexed(123));
        assert_eq!(theme.foreground, Color::White);
        assert_eq!(theme.agent_processing_background, theme.accent);
        assert_eq!(theme.agent_ready_background, Color::Rgb(18, 52, 86));

        let defaults = Theme::from_options(&HashMap::new());
        assert_eq!(defaults.background, Color::Reset);
        assert_eq!(defaults.foreground, Color::White);
        assert_eq!(defaults.accent, Color::Cyan);
    }

    #[test]
    fn serializes_picker_content_and_border_styles() {
        let mut theme = Theme {
            background: Color::Rgb(40, 44, 52),
            foreground: Color::Indexed(231),
            border: Color::DarkGray,
            accent: Color::Cyan,
            selection_background: Color::Reset,
            selection_foreground: Color::Reset,
            normal_mode_background: Color::Reset,
            normal_mode_foreground: Color::Reset,
            agent_processing_background: Color::Reset,
            agent_permission_background: Color::Reset,
            agent_input_background: Color::Reset,
            agent_ready_background: Color::Reset,
        };
        assert_eq!(
            theme.picker_popup_styles(),
            (
                "fg=colour231,bg=#282c34".into(),
                "fg=brightblack,bg=#282c34".into()
            )
        );
        theme.background = Color::Reset;
        theme.foreground = Color::White;
        theme.border = Color::Indexed(8);
        assert_eq!(
            theme.picker_popup_styles(),
            (
                "fg=brightwhite,bg=default".into(),
                "fg=colour8,bg=default".into()
            )
        );
    }

    #[test]
    fn parses_tmux_style_colors() {
        assert_eq!(
            style_color("noattr,bg=#282c34,fg=colour231", "bg"),
            Some(Color::Rgb(40, 44, 52))
        );
        assert_eq!(
            style_color("noattr,bg=#282c34,fg=colour231", "fg"),
            Some(Color::Indexed(231))
        );
    }

    #[test]
    fn extracts_and_classifies_semantic_status_colors() {
        let palette =
            colors_in_format("#[bg=#50fa7b] ready #[bg=#ffb86c] permission #[bg=#8be9fd] input");
        assert_eq!(
            semantic_color(&palette, 125.0),
            Some(Color::Rgb(80, 250, 123))
        );
        assert_eq!(
            semantic_color(&palette, 30.0),
            Some(Color::Rgb(255, 184, 108))
        );
        assert_eq!(
            semantic_color(&palette, 190.0),
            Some(Color::Rgb(139, 233, 253))
        );
    }

    #[test]
    fn derives_a_subtle_selection_color() {
        assert_eq!(
            blend(Color::Rgb(40, 44, 52), Color::Rgb(97, 175, 239), 28),
            Some(Color::Rgb(55, 80, 104))
        );
    }

    #[test]
    fn darkens_a_raised_dark_theme_surface_for_the_canvas() {
        assert_eq!(
            canvas_from_status(Color::Rgb(68, 71, 90), Color::Rgb(248, 248, 242)),
            Color::Rgb(42, 44, 55)
        );
    }

    #[test]
    fn preserves_an_already_dark_or_light_theme_background() {
        assert_eq!(
            canvas_from_status(Color::Rgb(40, 44, 52), Color::Rgb(171, 178, 191)),
            Color::Rgb(40, 44, 52)
        );
        assert_eq!(
            canvas_from_status(Color::Rgb(245, 245, 245), Color::Rgb(30, 30, 30)),
            Color::Rgb(245, 245, 245)
        );
    }
}
