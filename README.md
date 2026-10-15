# tmux-enhance

Telescope-style navigation, Harpoon-style session marks, and agent awareness for tmux.

## Status

Early development. The first vertical slice is a cross-session window picker with a live terminal preview and Telescope-style insert/normal modes.

## Development install

```sh
cargo build --release
```

Then load the local checkout from `~/.tmux.conf`:

```tmux
run-shell '/home/doug/dev/tmux-plugins/tmux-enhance/tmux-enhance.tmux'
```

Reload tmux. Press `prefix + Space` to open the window picker.

### Picker keys

- The picker opens in insert mode for immediate searching.
- Insert mode: type to search, `Esc` for normal mode, arrows to select, `Enter` to open.
- Normal mode: `j`/`k` to select, `h`/`l` to move through the query, `i` to insert before the cursor, `a` after it, `A` or `/` at the end, `gg`/`G` for first/last, `Enter` to open, `q` to close.

The picker inherits its background, foreground, and accent from tmux's resolved window, status, and pane border styles. When a theme exposes only its brighter status-bar surface, tmux-enhance derives a darker canvas for dark themes. Borders use the active-pane accent when the normal border would disappear into the background. The selected-row color is a subtle blend of the canvas and accent rather than tmux's often-bright copy-mode highlight. This works with theme plugins without requiring theme-specific integration. Individual colors can be overridden before loading tmux-enhance:

```tmux
set -g @enhance-color-background '#282c34'
set -g @enhance-color-foreground '#abb2bf'
set -g @enhance-color-accent '#61afef'
set -g @enhance-color-border '#5c6370'
set -g @enhance-color-selection-background '#3e4451'
set -g @enhance-color-selection-foreground '#ffffff'
set -g @enhance-color-normal-mode-background '#e5c07b'
set -g @enhance-color-normal-mode-foreground '#282c34'
```

## Planned bindings

| Binding | Action |
| --- | --- |
| `prefix + Space` | Search windows across all sessions |
| `prefix + a` | Search active agents |
| `prefix + h`, `a` | Add the current session to Harpoon |
| `prefix + h`, `o` | Open the Harpoon list |
| `prefix + h`, `1`–`9` | Jump to a Harpoon slot |
| `prefix + Enter` | Jump to the latest agent waiting for input |
