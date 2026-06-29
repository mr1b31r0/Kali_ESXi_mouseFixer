#!/usr/bin/env bash
# install.sh — build and install softcursor as a user systemd service
# Built WITH CSK by NoHatHacker.com
set -e

BINARY="softcursor"
TARGET_DIR="$HOME/.local/bin"
SERVICE_DIR="$HOME/.config/systemd/user"
AUTOSTART_DIR="$HOME/.config/autostart"
REPO_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── Colour helpers ─────────────────────────────────────────────────────────────
green()  { printf '\033[32m[+]\033[0m %s\n' "$*"; }
red()    { printf '\033[31m[!]\033[0m %s\n' "$*" >&2; }
info()   { printf '\033[34m[*]\033[0m %s\n' "$*"; }

# ── Dependency check ───────────────────────────────────────────────────────────
MISSING_APT=()
for pkg in libx11-dev libxfixes-dev libxext-dev pkg-config; do
    dpkg -s "$pkg" &>/dev/null || MISSING_APT+=("$pkg")
done

if [ ${#MISSING_APT[@]} -gt 0 ]; then
    info "Installing missing packages: ${MISSING_APT[*]}"
    if command -v sudo &>/dev/null; then
        sudo apt-get install -y "${MISSING_APT[@]}"
    else
        red "sudo not available — run as root or install manually: apt install ${MISSING_APT[*]}"
        exit 1
    fi
fi

# ── Rust check ────────────────────────────────────────────────────────────────
if ! command -v cargo &>/dev/null; then
    # Try the default user install location
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        export PATH="$HOME/.cargo/bin:$PATH"
    else
        red "cargo not found. Install Rust first:"
        red "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
fi

# ── Build ──────────────────────────────────────────────────────────────────────
info "Building $BINARY (release)..."
cd "$REPO_DIR"
cargo build --release --bin "$BINARY"
green "Build complete."

# ── Install binary ─────────────────────────────────────────────────────────────
info "Installing binary → $TARGET_DIR/$BINARY"
mkdir -p "$TARGET_DIR"
cp "target/release/$BINARY" "$TARGET_DIR/$BINARY"
chmod +x "$TARGET_DIR/$BINARY"

# Ensure ~/.local/bin is on PATH for this session
if [[ ":$PATH:" != *":$TARGET_DIR:"* ]]; then
    export PATH="$TARGET_DIR:$PATH"
    info "Added $TARGET_DIR to PATH for this session."
    info "Add it permanently:  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
fi

# ── Install systemd user service ───────────────────────────────────────────────
# ── Autostart .desktop (works on XFCE, GNOME, Cinnamon, KDE — most reliable) ──
info "Installing autostart entry (~/.config/autostart)..."
mkdir -p "$AUTOSTART_DIR"
sed "s|%u|$HOME|g" "$REPO_DIR/softcursor.desktop" > "$AUTOSTART_DIR/softcursor.desktop"
green "Autostart entry installed — will launch on next login automatically."

# ── systemd user service (secondary, for DEs that signal graphical-session) ──
if command -v systemctl &>/dev/null && systemctl --user status &>/dev/null 2>&1; then
    info "Installing systemd user service..."
    mkdir -p "$SERVICE_DIR"
    cp "$REPO_DIR/softcursor.service" "$SERVICE_DIR/softcursor.service"
    systemctl --user daemon-reload
    systemctl --user enable softcursor.service 2>/dev/null || true
    green "systemd service enabled."
fi

# ── Start now in the current session ──────────────────────────────────────────
if pgrep -x softcursor >/dev/null 2>&1; then
    info "softcursor already running — restarting..."
    pkill -x softcursor 2>/dev/null || true
    sleep 0.5
fi
DISPLAY="${DISPLAY:-:0}" nohup "$TARGET_DIR/$BINARY" >/dev/null 2>&1 &
green "softcursor started now (PID $!)."

echo ""
green "Done. softcursor is running."
echo ""
echo "  Disable:  systemctl --user disable --now softcursor"
echo "  Logs:     journalctl --user -u softcursor -f"
echo "  Uninstall: rm $TARGET_DIR/$BINARY $SERVICE_DIR/softcursor.service $AUTOSTART_DIR/softcursor.desktop"
echo ""
echo "  Built WITH CSK by NoHatHacker.com"
