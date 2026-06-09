# hyprcwd-rs

Outputs the working directory of the currently active window in the hyprland window manager.

Intended for starting a new terminal window from a hotkey, in the directory of the currently active window.

Rust port of https://github.com/vilari-mickopf/hyprcwd, to shave off those milliseconds. Now featuring a more reliable algorithm.

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

Hyprland Lua key binding:

```
hl.bind("SUPER + T", hl.dsp.exec_cmd([[kitty -d "$(hyprcwd)"]]))
```

Legacy hyprlang key binding:

```
bind = $mainMod, T, exec, kitty -d "$(hyprcwd)"
```

or the equivalent for your terminal app.

### NixOS / Flakes

If you are using NixOS with Flakes, you can add `hyprcwd` directly to your system configuration.

First, add the repository to your `flake.nix` inputs. We highly recommend using `follows` to ensure it builds using your system's existing Rust toolchain rather than downloading redundant dependencies:

```nix
inputs = {
  # ... your other inputs
  hyprcwd = {
    url = "github:JonnieCache/hyprcwd-rs";
    inputs.nixpkgs.follows = "nixpkgs";
  };
};
```

Then, pass the inputs to your modules and add the package to your environment.systemPackages (or home.packages if using Home Manager):

```nix
{ pkgs, inputs, ... }: {
  environment.systemPackages = [
    inputs.hyprcwd.packages.${pkgs.system}.default
  ];
}
```
