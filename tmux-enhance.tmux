#!/usr/bin/env bash

CURRENT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
default_bin="$CURRENT_DIR/target/release/tmux-enhance"

tmux set-option -gq @enhance-bin "$default_bin"
tmux set-option -gq @enhance-picker-width '90%'
tmux set-option -gq @enhance-picker-height '80%'
tmux set-option -ogq @enhance-toast-duration '4'

enhance_bin="$(tmux show-option -gqv @enhance-bin)"
picker_width="$(tmux show-option -gqv @enhance-picker-width)"
picker_height="$(tmux show-option -gqv @enhance-picker-height)"

tmux bind-key Space display-popup -E \
  -w "$picker_width" \
  -h "$picker_height" \
  -d "#{pane_current_path}" \
  "\"$enhance_bin\" picker windows"

tmux bind-key h switch-client -T enhance-harpoon
tmux bind-key -T enhance-harpoon a run-shell "\"$enhance_bin\" harpoon add"
tmux bind-key -T enhance-harpoon r run-shell "\"$enhance_bin\" harpoon remove-current"
tmux bind-key -T enhance-harpoon o display-popup -E \
  -w "$picker_width" \
  -h "$picker_height" \
  -d "#{pane_current_path}" \
  "\"$enhance_bin\" picker harpoon"

for slot in {1..9}; do
  tmux bind-key -T enhance-harpoon "$slot" run-shell "\"$enhance_bin\" harpoon jump $slot"
done
