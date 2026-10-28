#!/usr/bin/env bash

CURRENT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
release_bin="$CURRENT_DIR/bin/tmux-enhance"
source_bin="$CURRENT_DIR/target/release/tmux-enhance"
configured_bin="$(tmux show-option -gqv @enhance-bin)"

tmux set-option -ogq @enhance-auto-install 'on'
auto_install="$(tmux show-option -gqv @enhance-auto-install)"

if [[ -n "$configured_bin" && "$configured_bin" != "$release_bin" ]]; then
  default_bin="$configured_bin"
elif [[ -x "$source_bin" ]]; then
  default_bin="$source_bin"
elif [[ "$auto_install" == 'on' && -f "$CURRENT_DIR/scripts/install-binary.sh" ]]; then
  if ! bash "$CURRENT_DIR/scripts/install-binary.sh" >/dev/null 2>&1; then
    tmux display-message 'tmux-enhance could not install its release binary; run scripts/install-binary.sh for details'
  fi
  default_bin="$release_bin"
elif [[ -x "$release_bin" ]]; then
  default_bin="$release_bin"
else
  default_bin="$source_bin"
fi

tmux set-option -gq @enhance-bin "$default_bin"
tmux set-option -ogq @enhance-picker-width '90%'
tmux set-option -ogq @enhance-picker-height '80%'
tmux set-option -ogq @enhance-toast-duration '4'
tmux set-option -ogq @enhance-harpoon-key 'h'
tmux set-option -ogq @enhance-auto-integrations 'on'
tmux set-option -ogq @enhance-alerts 'on'
tmux set-option -ogq @enhance-alert-overlay 'on'
tmux set-option -ogq @enhance-alert-duration '15'
tmux set-option -ogq @enhance-alert-width '50'
tmux set-option -ogq @enhance-alert-height '6'
tmux set-option -ogq @enhance-alert-ready 'on'
tmux set-option -ogq @enhance-alert-bell 'off'
tmux set-option -ogq @enhance-alert-native 'off'

enhance_bin="$(tmux show-option -gqv @enhance-bin)"
harpoon_key="$(tmux show-option -gqv @enhance-harpoon-key)"
auto_integrations="$(tmux show-option -gqv @enhance-auto-integrations)"

if [[ -x "$enhance_bin" && "$auto_integrations" == 'on' ]]; then
  if [[ -d "$CURRENT_DIR/opencode" ]]; then
    "$enhance_bin" opencode install \
      --plugin-dir "$CURRENT_DIR/opencode" \
      --binary "$enhance_bin" \
      --quiet >/dev/null 2>&1 &
  fi
  "$enhance_bin" claude-code install \
    --binary "$enhance_bin" \
    --quiet >/dev/null 2>&1 &
fi

tmux bind-key Space next-layout

tmux bind-key s run-shell -b \
  'TMUX_ENHANCE_CLIENT=#{q:client_name} #{q:@enhance-bin} picker-popup sessions'

tmux bind-key a run-shell -b \
  'TMUX_ENHANCE_CLIENT=#{q:client_name} #{q:@enhance-bin} picker-popup agents'

tmux bind-key Enter run-shell "\"$enhance_bin\" alert-jump"

tmux bind-key -T root MouseDown1Pane if-shell -F -t = \
  '#{!=:#{@enhance-alert-source},}' \
  "run-shell -t = \"\\\"$enhance_bin\\\" alert-click #{q:@enhance-alert-source}\"" \
  'select-pane -t = ; send-keys -M'

if [[ -n "$harpoon_key" && "$harpoon_key" != 'off' ]]; then
  tmux bind-key "$harpoon_key" switch-client -T enhance-harpoon
fi
tmux bind-key -T enhance-harpoon a run-shell "\"$enhance_bin\" harpoon add"
tmux bind-key -T enhance-harpoon r run-shell "\"$enhance_bin\" harpoon remove-current"
tmux bind-key -T enhance-harpoon o run-shell -b \
  'TMUX_ENHANCE_CLIENT=#{q:client_name} #{q:@enhance-bin} picker-popup harpoon'

for slot in {1..9}; do
  tmux bind-key -T enhance-harpoon "$slot" run-shell "\"$enhance_bin\" harpoon jump $slot"
done

tmux set-hook -g 'client-session-changed[987]' \
  "run-shell \"\\\"$enhance_bin\\\" session-touch #{q:hook_session_name}\""
