#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT_DIR/VERSION")"
REPOSITORY="${TMUX_ENHANCE_REPOSITORY:-dough654/tmux-enhance}"
SYSTEM="${TMUX_ENHANCE_SYSTEM:-$(uname -s)}"
MACHINE="${TMUX_ENHANCE_MACHINE:-$(uname -m)}"
temporary=''

cleanup() {
  if [[ -n "$temporary" ]]; then
    rm -rf -- "$temporary"
  fi
}

trap cleanup EXIT

release_target() {
  case "$SYSTEM:$MACHINE" in
    Linux:x86_64 | Linux:amd64)
      printf '%s\n' 'x86_64-unknown-linux-musl'
      ;;
    Linux:aarch64 | Linux:arm64)
      printf '%s\n' 'aarch64-unknown-linux-musl'
      ;;
    Darwin:x86_64 | Darwin:amd64)
      printf '%s\n' 'x86_64-apple-darwin'
      ;;
    Darwin:arm64 | Darwin:aarch64)
      printf '%s\n' 'aarch64-apple-darwin'
      ;;
    *)
      printf 'tmux-enhance does not provide a release binary for %s/%s\n' "$SYSTEM" "$MACHINE" >&2
      return 1
      ;;
  esac
}

download() {
  local url="$1"
  local destination="$2"

  if command -v curl >/dev/null 2>&1; then
    curl --fail --location --silent --show-error --retry 3 \
      --output "$destination" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget --quiet --output-document="$destination" "$url"
  else
    printf 'tmux-enhance requires curl or wget to download a release binary\n' >&2
    return 1
  fi
}

sha256() {
  local file="$1"

  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$file" | awk '{print $NF}'
  else
    printf 'tmux-enhance requires sha256sum, shasum, or openssl to verify its download\n' >&2
    return 1
  fi
}

install_binary() {
  local target archive package base_url expected actual extracted installed
  target="$(release_target)"
  archive="tmux-enhance-${VERSION}-${target}.tar.gz"
  package="${archive%.tar.gz}"
  base_url="https://github.com/${REPOSITORY}/releases/download/v${VERSION}"
  installed="$ROOT_DIR/bin/tmux-enhance"

  if [[ -x "$installed" ]] && [[ "$($installed --version 2>/dev/null || true)" == "tmux-enhance $VERSION" ]]; then
    return 0
  fi

  temporary="$(mktemp -d "${TMPDIR:-/tmp}/tmux-enhance.XXXXXX")"

  download "$base_url/$archive" "$temporary/$archive"
  download "$base_url/SHA256SUMS" "$temporary/SHA256SUMS"

  expected="$(awk -v archive="$archive" '$2 == archive { print $1 }' "$temporary/SHA256SUMS")"
  if [[ -z "$expected" ]]; then
    printf 'tmux-enhance could not find %s in SHA256SUMS\n' "$archive" >&2
    return 1
  fi
  actual="$(sha256 "$temporary/$archive")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'tmux-enhance checksum verification failed for %s\n' "$archive" >&2
    return 1
  fi

  tar -xzf "$temporary/$archive" -C "$temporary"
  extracted="$temporary/$package/bin/tmux-enhance"
  if [[ ! -x "$extracted" ]] || [[ "$($extracted --version)" != "tmux-enhance $VERSION" ]]; then
    printf 'tmux-enhance release archive did not contain the expected binary\n' >&2
    return 1
  fi

  mkdir -p "$ROOT_DIR/bin"
  install -m 755 "$extracted" "$ROOT_DIR/bin/.tmux-enhance.$$"
  mv -f "$ROOT_DIR/bin/.tmux-enhance.$$" "$installed"
  printf 'Installed tmux-enhance %s for %s\n' "$VERSION" "$target"
}

if [[ "${1:-}" == '--print-target' ]]; then
  release_target
else
  install_binary
fi
