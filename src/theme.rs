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
}

impl Theme {
    pub fn from_tmux(tmux: &Tmux) -> Self {
        let status = tmux.option("status-style").unwrap_or_default();
        let window = tmux.option("window-active-style").unwrap_or_default();
        let mode = tmux.option("mode-style").unwrap_or_default();
        let border = tmux.option("pane-border-style").unwrap_or_default();
        let active_border = tmux.option("pane-active-border-style").unwrap_or_default();

        let foreground = style_color(&status, "fg").unwrap_or(Color::White);
        let status_background = style_color(&status, "bg").unwrap_or(Color::Reset);
        let background = style_color(&window, "bg")
            .filter(|color| *color != Color::Reset)
            .unwrap_or_else(|| canvas_from_status(status_background, foreground));
        let accent = style_color(&active_border, "fg").unwrap_or(Color::Cyan);
        let configured_border = style_color(&border, "fg").unwrap_or(Color::DarkGray);

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
            normal_mode_background: style_color(&mode, "bg").unwrap_or(Color::Yellow),
            normal_mode_foreground: style_color(&mode, "fg").unwrap_or(Color::Black),
        };

        theme.background = override_color(tmux, "@enhance-color-background", theme.background);
        theme.foreground = override_color(tmux, "@enhance-color-foreground", theme.foreground);
        theme.accent = override_color(tmux, "@enhance-color-accent", theme.accent);
        theme.border = override_color(tmux, "@enhance-color-border", theme.border);
        theme.selection_background = override_color(
            tmux,
            "@enhance-color-selection-background",
            theme.selection_background,
        );
        theme.selection_foreground = override_color(
            tmux,
            "@enhance-color-selection-foreground",
            theme.selection_foreground,
        );
        theme.normal_mode_background = override_color(
            tmux,
            "@enhance-color-normal-mode-background",
            theme.normal_mode_background,
        );
        theme.normal_mode_foreground = override_color(
            tmux,
            "@enhance-color-normal-mode-foreground",
            theme.normal_mode_foreground,
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

fn override_color(tmux: &Tmux, option: &str, fallback: Color) -> Color {
    tmux.option(option)
        .ok()
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
