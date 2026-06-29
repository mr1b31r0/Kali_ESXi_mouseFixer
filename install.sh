#!/usr/bin/env bash
# install.sh — build and install softcursor as a user systemd service
# https://noHatHacker.com
set -e

BINARY="softcursor"
TARGET_DIR="$HOME/.local/bin"
SERVICE_DIR="$HOME/.config/systemd/user"

echo "[*] Building $BINARY (release)..."
cargo build --release --bin "$BINARY"

echo "[*] Installing binary to $TARGET_DIR/$BINARY"
mkdir -p "$TARGET_DIR"
cp "target/release/$BINARY" "$TARGET_DIR/$BINARY"
chmod +x "$TARGET_DIR/$BINARY"

echo "[*] Installing systemd user service..."
mkdir -p "$SERVICE_DIR"
cp softcursor.service "$SERVICE_DIR/softcursor.service"

echo "[*] Enabling and starting service..."
systemctl --user daemon-reload
systemctl --user enable --now softcursor.service

echo ""
echo "[+] Done. softcursor is running."
echo "    Disable anytime: systemctl --user disable --now softcursor"
echo "    Logs:            journalctl --user -u softcursor -f"
echo ""
echo "    noHatHacker.com"
