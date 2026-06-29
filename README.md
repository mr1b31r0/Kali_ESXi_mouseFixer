# Kali ESXi Mouse Fixer

**VMware ESXi 6.x SVGA doesn't render the hardware cursor on modern Kali Linux.**
This daemon draws a software cursor on top of everything so you can actually see where your mouse is.

Works on **XFCE, GNOME (GDM3), Cinnamon, KDE, MATE** — any desktop running X11.

Built **WITH CSK** by **[NoHatHacker.com](https://noHatHacker.com)**

> **Tested on:** VMware ESXi 6.5 HP Build SP3 · Kali Linux · GDM3 + Cinnamon · software rendering

---

## Quick install — pre-built binary (no Rust needed)

```bash
# Runtime deps only (usually already installed on Kali)
sudo apt install libx11-6 libxfixes3

# Download binary from latest release
wget https://github.com/mr1b31r0/Kali_ESXi_mouseFixer/releases/latest/download/softcursor
chmod +x softcursor

# Test it now
DISPLAY=:0 ./softcursor &
```

### Install as a user service (auto-starts with desktop)

```bash
mkdir -p ~/.local/bin ~/.config/systemd/user

mv softcursor ~/.local/bin/

wget -O ~/.config/systemd/user/softcursor.service \
  https://raw.githubusercontent.com/mr1b31r0/Kali_ESXi_mouseFixer/main/softcursor.service

systemctl --user daemon-reload
systemctl --user enable --now softcursor
```

---

## Build from source

### Requirements

```bash
sudo apt install libx11-dev libxfixes-dev pkg-config
```

> Rust is also required — install once with:
> ```bash
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> ```

### One-shot install script

```bash
git clone https://github.com/mr1b31r0/Kali_ESXi_mouseFixer
cd Kali_ESXi_mouseFixer
chmod +x install.sh
./install.sh
```

`install.sh` is bulletproof — it auto-detects and installs missing `apt` packages,
checks for `cargo`, builds the release binary, drops it into `~/.local/bin/`,
and registers the systemd user service. Falls back to a plain background process
if systemd is not available.

---

## How it works

- Creates a tiny borderless, always-on-top X11 window (`override_redirect`)
- Polls the real pointer position at 60 fps via `XQueryPointer`
- Draws a classic black/white arrow at that position
- The window is **fully input-transparent** via XFixes input shape — clicks and keyboard events pass straight through

No VMware tools patch, no kernel module, no root required after install.

---

## Cursor size

Edit `SCALE` in `src/main.rs` (default `18` px) and rebuild:

```bash
cargo build --release
cp target/release/softcursor ~/.local/bin/
systemctl --user restart softcursor
```

---

## Uninstall

```bash
systemctl --user disable --now softcursor
rm ~/.local/bin/softcursor
rm ~/.config/systemd/user/softcursor.service
```

---

## Troubleshooting

| Problem | Fix |
|---|---|
| `pkg-config: command not found` | `sudo apt install pkg-config` |
| `cargo: command not found` | `source ~/.cargo/env` or re-open terminal after rustup install |
| Cursor visible but clicks don't pass through | XFixes extension missing — `sudo apt install libxfixes3` |
| Service starts but no cursor visible | Check `DISPLAY` — run `echo $DISPLAY` and make sure it matches `:0` in the service file |
| `systemctl --user` not available | Run `loginctl enable-linger $USER`, log out and back in |

---

## License

MIT — Built WITH CSK by [NoHatHacker.com](https://noHatHacker.com)
