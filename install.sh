#!/bin/sh
# treeview installer — and the updater: re-run it to move to a newer release.
#
#   curl -fsSL https://raw.githubusercontent.com/fitri/treeview/main/install.sh | sh
#
# Downloads a prebuilt binary from the latest GitHub release and replaces any
# existing install. If there is already a copy at the same version it stops
# without downloading anything. If there is no prebuilt binary for the platform,
# it falls back to `cargo install --git`.
#
# Environment:
#   PREFIX   install directory (default: $HOME/.local/bin)
#   VERSION  release tag to install (default: latest; `VERSION=v0.1.0` pins it)
#   FORCE    set to 1 to reinstall even when the version already matches
set -eu

REPO="fitri/treeview"
BIN="treeview"
PREFIX="${PREFIX:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"

say() { printf '  %s\n' "$*"; }

# The literal `$PATH` in the output is deliberate: it is shell text to copy.
# shellcheck disable=SC2016
missing_path_hint() {
    printf '\n  note: %s is not on your PATH. Add:\n\n    export PATH="%s:$PATH"\n' \
        "$PREFIX" "$PREFIX"
}
die() { printf '\nerror: %s\n' "$*" >&2; exit 1; }

printf '\ntreeview installer\n\n'

# ---- work out the target triple ---------------------------------------------
case "$(uname -s)" in
    Linux)  OS="unknown-linux-musl" ;;
    Darwin) OS="apple-darwin" ;;
    *) die "unsupported operating system: $(uname -s)" ;;
esac

case "$(uname -m)" in
    x86_64 | amd64)  ARCH="x86_64" ;;
    arm64 | aarch64) ARCH="aarch64" ;;
    *) die "unsupported architecture: $(uname -m)" ;;
esac

TARGET="${ARCH}-${OS}"
say "platform: ${TARGET}"

# `VERSION=0.1.0` is a reasonable thing to type; tags carry the `v`.
case "$VERSION" in
    latest | v*) ;;
    [0-9]*) VERSION="v${VERSION}" ;;
esac

# ---- what is already here ----------------------------------------------------
# Prints "v0.1.0"-style, or nothing when there is no runnable install. A binary
# that will not report its version (wrong arch, truncated download) is treated
# as absent, so the reinstall path repairs it.
current_version() {
    [ -x "$PREFIX/$BIN" ] || return 1
    "$PREFIX/$BIN" --version 2>/dev/null | awk 'NR == 1 { print "v" $2 }'
}

# Tag of the latest release. Resolved from the `/releases/latest` redirect, so it
# costs one request, needs no API token and is not rate limited.
latest_tag() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSLI --proto '=https' --tlsv1.2 -o /dev/null -w '%{url_effective}' \
            "https://github.com/${REPO}/releases/latest" 2>/dev/null \
            | sed -n 's#.*/tag/##p'
    elif command -v wget >/dev/null 2>&1; then
        wget -q --max-redirect=5 --server-response --spider \
            "https://github.com/${REPO}/releases/latest" 2>&1 \
            | sed -n 's#.*[Ll]ocation: .*/tag/##p' | tail -n 1
    fi
}

HAVE="$(current_version || true)"
WANT="$VERSION"
if [ "$WANT" = "latest" ]; then
    WANT="$(latest_tag || true)"
    [ -n "$WANT" ] || say "could not read the latest release; trying the download anyway"
fi

if [ -n "$HAVE" ] && [ -n "$WANT" ] && [ "$HAVE" = "$WANT" ] && [ "${FORCE:-0}" != "1" ]; then
    say "already at ${HAVE} — nothing to do"
    printf '\nDone. Run `treeview` in any directory.\n\n'
    exit 0
fi

# ---- URLs --------------------------------------------------------------------
if [ -n "$WANT" ]; then
    BASE="https://github.com/${REPO}/releases/download/${WANT}"
else
    BASE="https://github.com/${REPO}/releases/latest/download"
fi
URL="${BASE}/${BIN}-${TARGET}.tar.gz"

# ---- download helpers --------------------------------------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https' --tlsv1.2 "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        return 1
    fi
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# Tells "this platform has no asset" apart from "there is no such release", so a
# typo in VERSION does not send the user into a five-minute source build.
release_exists() {
    [ -n "$WANT" ] || return 0
    if command -v curl >/dev/null 2>&1; then
        curl -fsSLI --proto '=https' --tlsv1.2 -o /dev/null \
            "https://github.com/${REPO}/releases/tag/${WANT}" 2>/dev/null
    elif command -v wget >/dev/null 2>&1; then
        wget -q --spider "https://github.com/${REPO}/releases/tag/${WANT}" 2>/dev/null
    else
        return 0
    fi
}

# Published next to the archive since v0.1.1. Older releases have none, so a
# missing checksum is not an error — but a mismatched one always is.
verify() {
    fetch "${URL}.sha256" "$TMP/pkg.sha256" 2>/dev/null || return 0
    expected="$(cut -d' ' -f1 "$TMP/pkg.sha256" | tr -d '[:space:]')"
    [ -n "$expected" ] || return 0
    actual="$(sha256_of "$TMP/pkg.tar.gz" || true)"
    [ -n "$actual" ] || { say "no sha256 tool here; skipping verification"; return 0; }
    [ "$expected" = "$actual" ] || die "checksum mismatch for ${URL}"
    say "checksum ok"
}

# ---- try the prebuilt binary ------------------------------------------------
if fetch "$URL" "$TMP/pkg.tar.gz" 2>/dev/null; then
    say "downloaded ${WANT:-latest}"
    verify
    tar -xzf "$TMP/pkg.tar.gz" -C "$TMP" || die "could not extract the archive"
    [ -f "$TMP/$BIN" ] || die "archive did not contain a '$BIN' binary"

    mkdir -p "$PREFIX" || die "cannot create $PREFIX (try PREFIX=/usr/local/bin)"
    install -m 755 "$TMP/$BIN" "$PREFIX/$BIN" 2>/dev/null \
        || { cp "$TMP/$BIN" "$PREFIX/$BIN" && chmod 755 "$PREFIX/$BIN"; } \
        || die "cannot write to $PREFIX"
    say "installed to $PREFIX/$BIN"
else
    # ---- fall back to building from source ---------------------------------
    release_exists || die "no release tagged ${WANT} (see https://github.com/${REPO}/releases)"
    say "no prebuilt binary for ${TARGET}; building from source"
    command -v cargo >/dev/null 2>&1 \
        || die "install Rust (https://rustup.rs) or download a release manually"

    if [ -n "$WANT" ]; then
        cargo install --git "https://github.com/${REPO}" --tag "$WANT" --locked --root "$TMP/root" \
            || die "cargo install failed"
    else
        cargo install --git "https://github.com/${REPO}" --locked --root "$TMP/root" \
            || die "cargo install failed"
    fi

    mkdir -p "$PREFIX"
    install -m 755 "$TMP/root/bin/$BIN" "$PREFIX/$BIN"
    say "installed to $PREFIX/$BIN"
fi

# ---- checks -----------------------------------------------------------------
NEW="$(current_version || true)"
if [ -n "$NEW" ] && [ -n "$HAVE" ] && [ "$NEW" != "$HAVE" ]; then
    say "updated ${HAVE} → ${NEW}"
elif [ -n "$NEW" ]; then
    say "$NEW"
fi

case ":${PATH}:" in
    *":${PREFIX}:"*) ;;
    *) missing_path_hint ;;
esac

printf '\nDone. Run `treeview` in any directory; re-run this script to update.\n\n'
