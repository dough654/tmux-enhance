# tmux-enhance

Telescope-style navigation, Harpoon-style session marks, and agent awareness for tmux.

## Status

The initial feature set is complete. GitHub releases provide tested packages for supported Linux and macOS architectures, and TPM installs the correct prebuilt binary automatically.

## Requirements

- tmux 3.7 or newer. Proactive notifications use tmux 3.7 floating panes so they can remain non-modal.
- A Rust toolchain when installing from source. Release packages include a prebuilt binary.
- OpenCode and Claude Code are optional; their integrations are provisioned automatically when present.

### Supported platforms

| Platform | Architectures | Release target |
| --- | --- | --- |
| Linux | x86_64, ARM64 | Statically linked musl binaries |
| macOS 11+ | Intel, Apple Silicon | Native binaries |
| Windows | x86_64 or ARM64 through WSL | Corresponding Linux binary |

Native Windows is not supported because tmux itself requires a Unix-like environment.

## Install with TPM

Add tmux-enhance to `~/.tmux.conf` with [TPM](https://github.com/tmux-plugins/tpm):

```tmux
set -g @plugin 'dough654/tmux-enhance'
```

Keep `run '~/.tmux/plugins/tpm/tpm'` at the bottom of the file, reload tmux, then press `prefix + I` to install. tmux-enhance detects your operating system and architecture, downloads the matching release binary, verifies its checksum, and provisions optional OpenCode and Claude Code integrations automatically. Rust is not required.

Automatic binary installation can be disabled before loading the plugin:

```tmux
set -g @enhance-auto-install 'off'
```

## Manual release install

Download the archive for your platform from the [latest release](https://github.com/dough654/tmux-enhance/releases/latest), verify it against `SHA256SUMS`, and extract it into your tmux plugins directory. For example:

```sh
mkdir -p ~/.tmux/plugins
tar -xzf tmux-enhance-*-x86_64-unknown-linux-musl.tar.gz -C ~/.tmux/plugins
mv ~/.tmux/plugins/tmux-enhance-*-x86_64-unknown-linux-musl ~/.tmux/plugins/tmux-enhance
```

Then load the packaged plugin from `~/.tmux.conf`:

```tmux
run-shell '~/.tmux/plugins/tmux-enhance/tmux-enhance.tmux'
```

Release archives contain the complete plugin and do not require Rust. Each tagged version is built and smoke-tested on its native operating system before GitHub publishes the archives and checksums.

## Source install

Clone the repository into your tmux plugins directory and build the binary:

```sh
git clone https://github.com/dough654/tmux-enhance.git ~/.tmux/plugins/tmux-enhance
cd ~/.tmux/plugins/tmux-enhance
cargo build --release
```

Then load it from `~/.tmux.conf`:

```tmux
run-shell '~/.tmux/plugins/tmux-enhance/tmux-enhance.tmux'
```

Reload tmux with `tmux source-file ~/.tmux.conf`. Press `prefix + s` to open the session and window picker.

Press `prefix + s` to open the session picker. It prioritizes the last session you visited, keeps the current session out of the way, and previews up to four windows using an adaptive one-, two-, or four-cell layout. Press `Tab` on a session to expand or collapse its indented window list, or `Shift + Tab` to expand or collapse every session; `Enter` on a child jumps directly to that window.

Searching automatically reveals matching child windows beneath their parent sessions without changing the saved expanded state. The original flat window picker remains available as `tmux-enhance picker windows`, but is no longer bound by default. `prefix + Space` is restored to tmux's native `next-layout` action.

Press `prefix + a` to search OpenCode and Claude Code agent panes together. Their integrations report the exact session attached to each tmux pane and distinguish `processing`, `permission`, `input`, and `ready`; OpenCode title/path matching remains available as a fallback for unintegrated panes. Statuses refresh once per second while the picker is open. Agents use the same MRU behavior as sessions: the pane you came from is prioritized and the currently focused agent is moved to the bottom.

When an agent enters `permission` or `input`, tmux-enhance shows a small floating pane in the top-right of each active tmux window. Floating panes require tmux 3.7 or newer and, unlike popups, are non-modal: the pane underneath stays focused and continues receiving input. Repeated heartbeats are deduplicated, alerts are suppressed when that agent pane is already focused, and later `processing` or `ready` reports resolve the event automatically. Click a notification to jump directly to that agent, or press `prefix + Enter` to jump to the newest unresolved agent. Clicking requires tmux mouse support (`set -g mouse on`).

The overlay size and lifetime are configurable. `ready` alerts, terminal bells, and host-side `notify-send` notifications are opt-in:

```tmux
set -g @enhance-alerts 'on'
set -g @enhance-alert-overlay 'on'
set -g @enhance-alert-duration 15
set -g @enhance-alert-width 50
set -g @enhance-alert-height 6
set -g @enhance-alert-ready 'off'
set -g @enhance-alert-bell 'off'
set -g @enhance-alert-native 'off'
```

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

## License

tmux-enhance is open source under the [MIT License](LICENSE).
