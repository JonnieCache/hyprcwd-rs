# hyprcwd-rs

Outputs the working directory of the currently active window in the hyprland window manager.

Intended for starting a new terminal window from a hotkey, in the directory of the currently active window.

Rust port of [https://github.com/vilari-mickopf/hyprcwd], to shave off those milliseconds.

## Install

- `cargo install hyprcwd`

- The included `flake.nix`

- Binary from the relases page

## Usage

Hyprland binding:

```
bind = $mainMod, T, exec, kitty -d "$(hyprcwd)"
```

or equivalent for your terminal app.
