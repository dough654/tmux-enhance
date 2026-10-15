# tmux-enhance

Telescope-style navigation, Harpoon-style session marks, and agent awareness for tmux.

## Status

Early development. Cross-session window search and persistent Harpoon session marks are available with live terminal previews and Telescope-style insert/normal modes.

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

## Harpoon sessions

Press `prefix + h`, then:

- `a` adds the current session to the first free slot from 1–9. Adding it again keeps its existing slot.
- `r` removes the current session from its slot.
- `o` opens the Harpoon sessions in the same Telescope-style picker.
- `1`–`9` jumps directly to that slot while preserving the session's active window.

In the Harpoon picker, press `Esc` for normal mode and `dd` to remove the selected slot without closing the picker.

Marks persist in `${XDG_DATA_HOME:-~/.local/share}/tmux-enhance/state.db`. Stopped sessions remain visible as unavailable entries until their slots are removed:

```sh
tmux-enhance harpoon list
tmux-enhance harpoon remove 3
```

Harpoon changes appear as non-modal, theme-aware status-right toasts. Their duration defaults to four seconds and can be configured in `.tmux.conf`:

```tmux
set -g @enhance-toast-duration 4
```

## Planned bindings

| Binding | Action |
| --- | --- |
| `prefix + Space` | Search windows across all sessions |
| `prefix + a` | Search active agents |
| `prefix + Enter` | Jump to the latest agent waiting for input |
