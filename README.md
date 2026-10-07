# hyprcwd-rs

Outputs the working directory of the currently active window in the hyprland window manager.

Intended for starting a new terminal window from a hotkey, in the directory of the currently active window.

Rust port of https://github.com/vilari-mickopf/hyprcwd, to shave off those milliseconds. Now featuring a more reliable algorithm, and support for the kitty remote control protocol.

## Install

- `cargo install --path .`
- Binary from the releases page

### NixOS / Flakes

If you are using NixOS with Flakes, you can add `hyprcwd` directly to your system configuration.

First, add the repository to your `flake.nix` inputs, using `follows` to avoid downloading redundant dependencies:

```nix
inputs = {
  hyprcwd = {
    url = "github:JonnieCache/hyprcwd-rs";
    inputs.nixpkgs.follows = "nixpkgs";
  };
};
```

Then, pass the inputs to your modules and add the package to your environment.systemPackages (or home.packages if using Home Manager):

```nix
  environment.systemPackages = [
    inputs.hyprcwd.packages.${pkgs.stdenv.hostPlatform.system}.default
  ];
```

## Usage

```
Usage: hyprcwd [OPTIONS]

Options:
  -d, --default-dir <DIR>      Directory to be printed if no active window is found
      --kitty-socket <SOCKET>  Kitty UNIX socket path/address; supports {kitty_pid}
  -h, --help                   Print help
```

Hyprland key binding:

```
hl.bind("SUPER + T", hl.dsp.exec_cmd('kitty -d "$(hyprcwd)"'))
```

or the equivalent for your terminal app.

## Kitty integration

Users of the [kitty terminal](https://sw.kovidgoyal.net/kitty) may desire to use its single-instance mode, `-1` in order to share GPU resources between windows for almost instant startup. Unfortunately this prevents hyprcwd from finding the right window. If however you enable the kitty remote control socket, hyprcwd will use that to determine the value directly. Add the below to your `kitty.conf`:

```conf
allow_remote_control socket-only # or just true
listen_on unix:${XDG_RUNTIME_DIR}/kitty-{kitty_pid}
```

If you use the above value for `listen_on`, hyprcwd will find it automatically. If you already have kitty listening on a different path, you can supply it to hyprcwd like so:

```sh
# For listen_on unix:/tmp/mykitty in kitty.conf, which appends -<PID>:
hyprcwd --kitty-socket '/tmp/mykitty-{kitty_pid}'

# For a fixed socket created with kitty --listen-on unix:/tmp/mykitty:
hyprcwd --kitty-socket /tmp/mykitty
```

### Permissions

To prevent the remote control socket from doing anything other than answering the particular request needed by hyprcwd, write the following to `~/.config/kitty/hyprcwd_auth.py`:

```python
def is_cmd_allowed(pcmd, window, from_socket, extra_data):
    return (
        from_socket
        and pcmd.get("cmd") == "ls"
        and pcmd.get("payload") == {"match": "state:focused"}
    )
```

Then add this to your `kitty.conf`:

```conf
remote_control_password "" hyprcwd_auth.py
```
```
