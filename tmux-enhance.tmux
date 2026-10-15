#!/usr/bin/env bash

CURRENT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
default_bin="$CURRENT_DIR/target/release/tmux-enhance"

tmux set-option -gq @enhance-bin "$default_bin"
tmux set-option -gq @enhance-picker-width '90%'
tmux set-option -gq @enhance-picker-height '80%'

enhance_bin="$(tmux show-option -gqv @enhance-bin)"
picker_width="$(tmux show-option -gqv @enhance-picker-width)"
picker_height="$(tmux show-option -gqv @enhance-picker-height)"

tmux bind-key Space display-popup -E \
  -w "$picker_width" \
  -h "$picker_height" \
  -d "#{pane_current_path}" \
  "\"$enhance_bin\" picker windows"
