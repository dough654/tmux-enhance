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

Reload tmux. Press `prefix + s` to open the session and window picker.

Press `prefix + s` to open the session picker. It prioritizes the last session you visited, keeps the current session out of the way, and previews up to four windows using an adaptive one-, two-, or four-cell layout. Press `Tab` on a session to expand or collapse its indented window list, or `Shift + Tab` to expand or collapse every session; `Enter` on a child jumps directly to that window.

Searching automatically reveals matching child windows beneath their parent sessions without changing the saved expanded state. The original flat window picker remains available as `tmux-enhance picker windows`, but is no longer bound by default. `prefix + Space` is restored to tmux's native `next-layout` action.

Press `prefix + a` to search OpenCode and Claude Code agent panes together. Their integrations report the exact session attached to each tmux pane and distinguish `processing`, `permission`, `input`, and `ready`; OpenCode title/path matching remains available as a fallback for unintegrated panes. Statuses refresh once per second while the picker is open. Agents use the same MRU behavior as sessions: the pane you came from is prioritized and the currently focused agent is moved to the bottom.

### OpenCode integration

Loading `tmux-enhance.tmux` automatically registers the bundled terminal plugin in OpenCode's global `cli.json`. Registration is idempotent and preserves unrelated settings and plugins. Existing OpenCode clients reload valid settings automatically; newly opened clients start reporting immediately.

Check or repair the integration with:

```sh
tmux-enhance doctor
tmux-enhance opencode status --plugin-dir /path/to/tmux-enhance/opencode
tmux-enhance opencode install \
  --plugin-dir /path/to/tmux-enhance/opencode \
  --binary /path/to/tmux-enhance
```

Remove only the tmux-enhance registration with:

```sh
tmux-enhance opencode remove --plugin-dir /path/to/tmux-enhance/opencode
```

### Claude Code integration

Loading `tmux-enhance.tmux` also registers tmux-enhance lifecycle hooks in Claude Code's global `~/.claude/settings.json`. Installation is idempotent and preserves unrelated settings and hooks. New or resumed Claude Code sessions report prompt, permission, elicitation, processing, and completion transitions automatically.

Check, repair, or remove only the tmux-enhance hooks with:

```sh
tmux-enhance claude-code status
tmux-enhance claude-code install --binary /path/to/tmux-enhance
tmux-enhance claude-code remove
```

### Picker keys

- The picker opens in insert mode for immediate searching.
- Insert mode: type to search, `Esc` for normal mode, arrows to select, `Enter` to open.
- Normal mode: `j`/`k` to select, `h`/`l` to move through the query, `i` to insert before the cursor, `a` after it, `A` or `/` at the end, `gg`/`G` for first/last, `Enter` to open, `q` to close.

The picker inherits its background, foreground, and accent from tmux's resolved window, status, and pane border styles. Agent status colors are selected from the resolved status-bar palette by semantic hue: green for ready, orange for permission, light blue/cyan for input, and the active-pane accent for processing. Static colors are used only when the active theme does not expose a suitable color. When a theme exposes only its brighter status-bar surface, tmux-enhance derives a darker canvas for dark themes. Borders use the active-pane accent when the normal border would disappear into the background. The selected-row color is a subtle blend of the canvas and accent rather than tmux's often-bright copy-mode highlight. This works with theme plugins without requiring theme-specific integration. Individual colors can be overridden before loading tmux-enhance:

```tmux
set -g @enhance-color-background '#282c34'
set -g @enhance-color-foreground '#abb2bf'
set -g @enhance-color-accent '#61afef'
set -g @enhance-color-border '#5c6370'
set -g @enhance-color-selection-background '#3e4451'
set -g @enhance-color-selection-foreground '#ffffff'
set -g @enhance-color-normal-mode-background '#e5c07b'
set -g @enhance-color-normal-mode-foreground '#282c34'
set -g @enhance-color-agent-processing '#6272a4'
set -g @enhance-color-agent-permission '#e5a84b'
set -g @enhance-color-agent-input '#2878b8'
set -g @enhance-color-agent-ready '#15803d'
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
| `prefix + Enter` | Jump to the latest agent waiting for input |
