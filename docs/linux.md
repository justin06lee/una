# una desktop on Linux

Status: runs on **Ubuntu 24.04 aarch64, GNOME on Xorg** (an NVIDIA Jetson Orin
Nano): hotkey, recording, paste, tray, and the history, settings, review and
fix windows. The Wayland paths below compile but are untested. Development
happens on macOS, so build on Linux after touching Linux code.

## Build dependencies

una is a tauri v2 app: it needs webkit2gtk 4.1 and a tray (appindicator)
library, plus ALSA headers for audio. All of these are available on arm64
(aarch64) as well as x86_64. You also need Rust and [bun](https://bun.sh).

### Debian / Ubuntu (incl. arm64)

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libasound2-dev pipewire-alsa
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

## Build and install

From the repo root:

```sh
make                # build → stop a running Una → install → launch
git pull && make    # later: the same, so Una restarts on the new build
```

This builds the pages with bun and the app with
`cargo build --release -p una-desktop --features tauri/custom-protocol` (without
that feature the windows look for a Vite dev server and come up blank), then
installs, without root:

- `una-desktop` and the `una` CLI into `/usr/local/bin` if you can write
  there, otherwise `~/.local/bin`;
- `~/.local/share/applications/sh.tenet.una.desktop` and its icons, so **Una**
  is in the app grid.

Launching Una opens its main window, the dictation history; **Settings** in
it opens the app's own settings. Tray → **Launch at Login** adds it to
`~/.config/autostart`. The app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` for
itself (WebKitGTK draws blank windows on NVIDIA/Tegra without it).

## Which server

The client needs a una server, and a small ARM board is a client, not a
server: CTranslate2 (under faster-whisper) ships CPU-only wheels for aarch64,
a Jetson comes without the CUDA toolkit, and large-v3-turbo plus an 8B cleanup
model don't fit next to a desktop in 8 GB of shared memory. Point it at your
GPU box in Settings → Server (LAN address plus a VPN/mesh one — see
[remote-access.md](remote-access.md)).

## Hotkey

On X11 both kinds of hotkey work, with hold, toggle and hybrid modes:

- **A key combination** (default `Ctrl+Alt+Space`), grabbed from the X server.
- **A single key**, like the Mac's Fn: Settings → Hotkey, click Left/Right
  Ctrl, Super or Alt in the keyboard row, or **Change** and press any key.
  una listens to XInput2 raw key events, so a bound modifier still works as a
  modifier (press another key within a second and that recording is dropped);
  any other single key (F13, Pause, …) is grabbed and stops reaching apps.

On Wayland neither is reliable; see the compositor keybinds below.

## Pasting

una puts the text on the clipboard and presses Ctrl+V with XTest, then puts
your clipboard back. Terminals paste with Ctrl+Shift+V: GNOME Terminal,
kitty, Alacritty, foot and Konsole are already listed in Settings → Pasting,
matched against the window's class.

## Tray

The tray icon (white bars, for GNOME's dark top bar) needs an AppIndicator
host: Ubuntu's GNOME has one on by default; elsewhere install the
**AppIndicator and KStatusNotifierItem Support** extension
(`gnome-shell-extension-appindicator`). Without one Una still runs: launch it
again to bring up its window (a second launch hands over to the first), and
the `una` CLI starts and stops dictation.

## Microphone

With the input device on **auto** una records from PipeWire (or PulseAudio),
so it uses the microphone chosen in Settings → Sound → Input. ALSA's own
`default` device can be a bare sound card that hears nothing (on a Jetson it
is the SoC's I2S interface), which is why auto skips it.

## What's different from macOS

- **Silently learning from your edits** needs the macOS accessibility API;
  on Linux una doesn't watch the pasted text. Tray → **Fix Last Dictation…**
  opens the fix window for the last paste whenever you want it.
- **Fix up** (Ctrl+J) runs through [yagami](https://github.com/justin06lee/yagami)
  on this machine: install it (`bun add -g @justin06lee/yagami`) and sign in
  to at least one agent CLI it drives (Claude Code, Codex, …).
- Clipboard restore keeps plain text only.

## Wayland: pasting setup

On X11 pasting needs nothing (above). On Wayland a helper is required:

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
