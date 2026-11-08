#!/bin/sh
# treeview installer
#
#   curl -fsSL https://raw.githubusercontent.com/fitri/treeview/main/install.sh | sh
#
# Downloads a prebuilt binary from the latest GitHub release. If there is no
# release for your platform, falls back to `cargo install --git`.
#
# Environment:
#   PREFIX   install directory (default: $HOME/.local/bin)
#   VERSION  release tag to install (default: latest)
set -eu

REPO="fitri/treeview"
BIN="treeview"
PREFIX="${PREFIX:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"

say() { printf '  %s\n' "$*"; }
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

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/${BIN}-${TARGET}.tar.gz"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${BIN}-${TARGET}.tar.gz"
fi

# ---- try the prebuilt binary ------------------------------------------------
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

if fetch "$URL" "$TMP/pkg.tar.gz" 2>/dev/null; then
    say "downloaded release"
    tar -xzf "$TMP/pkg.tar.gz" -C "$TMP" || die "could not extract the archive"
    [ -f "$TMP/$BIN" ] || die "archive did not contain a '$BIN' binary"

    mkdir -p "$PREFIX" || die "cannot create $PREFIX (try PREFIX=/usr/local/bin)"
    install -m 755 "$TMP/$BIN" "$PREFIX/$BIN" 2>/dev/null \
        || { cp "$TMP/$BIN" "$PREFIX/$BIN" && chmod 755 "$PREFIX/$BIN"; } \
        || die "cannot write to $PREFIX"
    say "installed to $PREFIX/$BIN"
else
    # ---- fall back to building from source ---------------------------------
    say "no prebuilt binary for ${TARGET}; building from source"
    command -v cargo >/dev/null 2>&1 \
        || die "install Rust (https://rustup.rs) or download a release manually"

    if [ "$VERSION" = "latest" ]; then
        cargo install --git "https://github.com/${REPO}" --locked --root "$TMP/root" \
            || die "cargo install failed"
    else
        cargo install --git "https://github.com/${REPO}" --tag "$VERSION" --locked --root "$TMP/root" \
            || die "cargo install failed"
    fi

    mkdir -p "$PREFIX"
    install -m 755 "$TMP/root/bin/$BIN" "$PREFIX/$BIN"
    say "installed to $PREFIX/$BIN"
fi

# ---- checks -----------------------------------------------------------------
if "$PREFIX/$BIN" --version >/dev/null 2>&1; then
    say "$("$PREFIX/$BIN" --version)"
fi

case ":${PATH}:" in
    *":${PREFIX}:"*) ;;
    *) printf '\n  note: %s is not on your PATH. Add:\n\n    export PATH="%s:$PATH"\n' "$PREFIX" "$PREFIX" ;;
esac

printf '\nDone. Run `treeview` in any directory.\n\n'
