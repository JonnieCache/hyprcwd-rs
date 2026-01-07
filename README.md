# hyprcwd-rs

Outputs the working directory of the currently active window in the hyprland window manager.

Intended for starting a new terminal window from a hotkey, in the directory of the currently active window.

Rust port of https://github.com/vilari-mickopf/hyprcwd, to shave off those milliseconds.

## Install

- `cargo install --path .`

- The included `flake.nix`

- Binary from the releases page

## Usage

```
Usage: hyprcwd [OPTIONS]

Options:
  -d, --default-dir <DIR>  Directory to be printed if no active window is found
  -h, --help               Print help
```

Hyprland key binding:

```
bind = $mainMod, T, exec, kitty -d "$(hyprcwd)"
```

or the equivalent for your terminal app.

