# Kali ESXi Mouse Fixer

**VMware ESXi 6.x SVGA doesn't render the hardware cursor on modern Kali Linux.**
This daemon draws a software cursor on top of everything so you can actually see where your mouse is.

Built with ❤️ by **[NoHatHacker](https://noHatHacker.com)**

---

## How it works

- Creates a tiny borderless, always-on-top X11 window
- Polls the real pointer position at 60 fps via `XQueryPointer`
- Draws a classic black/white arrow at that position
- The window is **fully input-transparent** — clicks and keyboard events pass straight through

No VMware tools patch, no kernel module, no root required.

---

## Requirements

```
libx11-dev libxfixes-dev cargo
```

Install on Kali:

```bash
sudo apt install libx11-dev libxfixes-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Install

```bash
git clone https://github.com/mr1b31r0/Kali_ESXi_mouseFixer
cd Kali_ESXi_mouseFixer
chmod +x install.sh
./install.sh
```

`install.sh` builds the binary, puts it in `~/.local/bin/`, and registers a systemd **user** service that starts automatically with your graphical session.

---

## Uninstall

```bash
systemctl --user disable --now softcursor
rm ~/.local/bin/softcursor
rm ~/.config/systemd/user/softcursor.service
```

---

## Manual run (no install)

```bash
cargo build --release
DISPLAY=:0 ./target/release/softcursor &
```

---

## Cursor size

Edit `SCALE` in `src/main.rs` (default `18` px) and rebuild:

```bash
cargo build --release
```

---

## License

MIT — https://noHatHacker.com
