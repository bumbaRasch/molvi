# Linux install & keybindings

molvi on Linux Wayland uses **compositor keybindings** to trigger push-to-talk.
Wayland has no global-hotkey API, and the `ashpd` GlobalShortcuts portal is
broken for overlay/layer-shell apps ([ashpd#213](https://github.com/bilelmoussaoui/ashpd/issues/213)).
molvi follows the same proven path as voxtype / whisrs / hyprwhspr /
nerd-dictation: a compositor keybinding runs `molvi record <verb>`, which
signals the already-running tray app via single-instance argv forwarding.

## Prerequisite

molvi must already be running (the tray app). The CLI subcommand **signals the
running instance** — it does not start one. Enable **Settings → General →
Start on login** so molvi is always in the tray.

## Two modes

- **Toggle mode** (tap to start, tap again to stop): one binding —
  `molvi record toggle`.
- **Push-to-talk / Command mode** (hold to record, release to finalize): two
  bindings — `molvi record start` on key **press**, `molvi record stop` on key
  **release**. Set the recognition mode in **Settings → Recognition** first.

`toggle`/`start` = key press; `stop` = key release. The `recognition_mode`
setting is read live, so changing it in Settings takes effect without a restart.

## Hyprland

`~/.config/hypr/hyprland.conf`:

```ini
# Toggle mode
bind = SUPER, V, exec, molvi record toggle

# Push-to-talk mode
bind  = SUPER, V, exec, molvi record start
bindr = SUPER, V, exec, molvi record stop
```

(`bindr` fires on key release.)

## Sway

`~/.config/sway/config`:

```
# Toggle mode
bindsym $mod+V exec molvi record toggle

# Push-to-talk mode
bindsym       $mod+V exec molvi record start
bindsym --release $mod+V exec molvi record stop
```

## Niri

`~/.config/niri/config.kdl`:

```kdl
// Toggle mode
binds {
    Mod+V { spawn "molvi" "record" "toggle"; }
}

// Push-to-talk mode
binds {
    Mod+V { spawn-at-press   "molvi" "record" "start";
            spawn-at-release "molvi" "record" "stop"; }
}
```

## River

`~/.config/river/init`:

```sh
# Toggle mode
riverctl map normal Super V spawn "molvi" "record" "toggle"

# Push-to-talk mode
riverctl map          normal Super V spawn "molvi" "record" "start"
riverctl map -release normal Super V spawn "molvi" "record" "stop"
```

## GNOME

**Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts → Add**,
Command: `molvi record toggle`, assign your key.

Or via `gsettings`:

```sh
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "['/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/molvi/']"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/molvi/ name 'molvi'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/molvi/ command 'molvi record toggle'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/molvi/ binding '<Super>v'
```

GNOME has no native key-release trigger for custom shortcuts — **use Toggle
mode**.

## KDE Plasma

**System Settings → Shortcuts → Add New → Command**, Command: `molvi record toggle`,
assign your key. Plasma 6 (Wayland) supports release triggers in custom
shortcuts for push-to-talk mode.

## X11 users

The in-app global hotkey works natively on X11 (**Settings → Hotkey**). The
`molvi record <verb>` subcommand also works on X11 but is optional there.
