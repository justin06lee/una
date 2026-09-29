# una desktop on Linux

Status: the Linux code paths compile on x86_64 Linux (checked on Arch with
webkit2gtk-4.1) but have **not been run against a live compositor yet**
(development happens on macOS). Treat this as a build recipe plus the setup
the code expects; issues are welcome.

## Build dependencies

una is a tauri v2 app: it needs webkit2gtk 4.1 and a tray (appindicator)
library, plus ALSA headers for audio. All of these are available on arm64
(aarch64) as well as x86_64.

### Debian / Ubuntu (incl. arm64)

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libasound2-dev
```

### Fedora

```sh
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel alsa-lib-devel
```

### Arch

```sh
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg alsa-lib
```

### Build

```sh
cd client/apps/desktop/ui && bun install && bun run build && cd ../../..
cargo build --release -p una-desktop -p una-cli
# or, for .deb/.AppImage bundles: cargo tauri build
```

## Text injection setup

una inserts text by putting it on the clipboard and synthesizing a paste
chord. On X11 this works out of the box (XTest). On Wayland a helper is
required:

### ydotool (recommended — works on every compositor)

1. Install the package: `ydotool` (Debian 13+/Ubuntu 22.04+: `sudo apt
   install ydotool`; Fedora: `sudo dnf install ydotool`; Arch: `sudo pacman
   -S ydotool`).
2. ydotool injects through `/dev/uinput`, which is root-owned by default.
   Give your user access with a udev rule and an `input` group membership:

   ```sh
   sudo usermod -aG input "$USER"
   echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' \
     | sudo tee /etc/udev/rules.d/80-uinput.rules
   sudo udevadm control --reload && sudo udevadm trigger
   # log out and back in for the group change
   ```

3. Run the daemon as a user service:

   ```sh
   systemctl --user enable --now ydotoold.service
   ```

   (If your distro ships no unit, create
   `~/.config/systemd/user/ydotoold.service` with
   `ExecStart=/usr/bin/ydotoold` under `[Service]`, plus a `[Unit]`
   description and `WantedBy=default.target` in `[Install]`.)

una looks for the socket at `$YDOTOOL_SOCKET`, then
`$XDG_RUNTIME_DIR/.ydotool_socket`, then `/tmp/.ydotool_socket`.

### wtype (fallback)

`wtype` works on wlroots compositors (Sway, Hyprland, river) via the
virtual-keyboard protocol, but **not on GNOME**. Install it and una uses it
when ydotool's socket is absent.

### Neither installed

Dictation still works: the text lands on the clipboard and the HUD reports
it; paste manually with Ctrl+V. Terminals often use Ctrl+Shift+V — una
already ships paste-chord overrides for kitty, alacritty, foot,
gnome-terminal, and konsole in its config
(`~/.config/una/config.toml`, `[insert.paste_overrides]`).

## Hold-to-talk via compositor keybinds

Wayland compositors don't deliver global key *release* events to apps, so
the built-in hotkey may behave toggle-ish depending on compositor. The
reliable route is binding the `una` CLI (it talks to the app's socket at
`$XDG_RUNTIME_DIR/una/una.sock`):

### Hyprland — true hold-to-talk

```conf
# ~/.config/hypr/hyprland.conf
bind  = CTRL ALT, SPACE, exec, una start   # key down
bindr = CTRL ALT, SPACE, exec, una stop    # key release
```

### Sway — true hold-to-talk

```conf
bindsym Ctrl+Alt+space exec una start
bindsym --release Ctrl+Alt+space exec una stop
```

### GNOME / KDE — toggle only

Neither exposes key-release custom shortcuts, so bind a toggle:

- GNOME: Settings → Keyboard → Custom Shortcuts → command `una toggle`.
- KDE: System Settings → Shortcuts → Add Command → `una toggle`.

Press once to start, again to finish (identical to una's "toggle" mode).

## GNOME tray note

GNOME removed tray icons; install the **AppIndicator and KStatusNotifierItem
Support** extension (`gnome-shell-extension-appindicator`) to see una's menu
bar icon. Without it the app still runs — control it via the `una` CLI and
the settings window (relaunch the binary to re-show settings; a second
instance forwards to the first).

## Wayland limitations

| Capability | X11 | wlroots (Sway/Hyprland) | GNOME | KDE |
|---|---|---|---|---|
| Paste injection | XTest (built-in) | ydotool or wtype | ydotool only | ydotool only |
| Global hotkey press | yes | yes | via custom shortcut | via custom shortcut |
| Global hotkey release (hold mode) | yes | `bindr` / `--release` + CLI | no (toggle only) | no (toggle only) |
| Frontmost app name (`app_name` field) | yes (EWMH WM_CLASS) | Hyprland/Sway: yes | no | no |
| Clipboard restore | text only (arboard) | text only | text only | text only |
| Tray icon | yes | yes (bar-dependent) | needs extension | yes |

Known gaps in the current code (see `client/crates/una-platform/src/linux/`):
frontmost-app detection returns `None` on GNOME/KDE Wayland (the server
accepts a missing `app_name`), and clipboard save/restore preserves plain
text only.
