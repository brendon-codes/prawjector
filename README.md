[![Prawjector terminal walkthrough](https://prawjector.com/media/usage/prawjector-basic-usage.poster.jpg)](https://prawjector.com/media/usage/prawjector-basic-usage.mp4)

# PRAWJECTOR

[GitHub Repo](https://github.com/brendon-codes/prawjector) | [Prawjector Website](https://prawjector.com/)

## A terminal project launcher

Prawjector is a keyboard-driven project launcher for developers who prefer to stay in the terminal. It opens named, persistent Zellij sessions and recreates pre-configured tab layouts on each launch. Reattaching to an existing session resumes the exact tabs and shells that were active before.

Prawjector works with most existing Zellij configurations without modification and does not add a daemon or background process. Projects are declared in a default JSON config at `~/.prawjector/prawjector.json`, or in another file selected with `--config <PATH>`.

## Install

### Install from GitHub releases

Download the archive for your Linux platform from the latest GitHub release:

- `prawjector-*-x86_64-linux-musl.tar.gz` for most x86_64 Linux systems when you want the most portable binary.
- `prawjector-*-aarch64-linux-musl.tar.gz` for most ARM64 Linux systems when you want the most portable binary.
- `prawjector-*-x86_64-linux-gnu.tar.gz` for x86_64 glibc Linux systems.
- `prawjector-*-aarch64-linux-gnu.tar.gz` for ARM64 glibc Linux systems.

```sh
tar -xzf prawjector-*-x86_64-linux-musl.tar.gz
mkdir -p ~/.local/bin
install -m 755 prawjector-*-x86_64-linux-musl/prawjector ~/.local/bin/prawjector
prawjector make-config
```

- Use a MUSL archive if you use Alpine Linux, are unsure which libc your system uses, or want the fewest runtime library assumptions.
- Use a GNU archive when you specifically want a glibc-targeted binary. GNU archives target glibc 2.17 or newer.
- Verify downloads with the release `SHA256SUMS` file before installing when possible.
- Zellij must be installed first. Prawjector relies on it for every launch.

### Build from source

```sh
cargo install --path .
prawjector make-config
```

- The install command uses the current checkout path. Build prawjector from a local clone of the upstream repository.
- Zellij must be installed first. Prawjector relies on it for every launch.
- Source builds require Rust stable, a C toolchain, `pkg-config`, and zlib development headers or a static zlib build setup.
- OpenSSL is not required for Prawjector's default local Git metadata support.
- `prawjector make-config` creates an example JSON config at the selected config path and leaves existing files untouched.
- Run `prawjector validate-config` after editing your projects to catch invalid entries before launch.

## Configure

### Create and validate config

```sh
prawjector make-config
prawjector validate-config
```

- `prawjector make-config` writes the default `~/.prawjector/prawjector.json` with starter projects to edit.
- The checked-in starter config lives at `examples/.prawjector/prawjector.json`.
- Use `--config <PATH>` before or after supported subcommands to select another config file.
- Each project needs `name`, `path`, and a `tabs` array.
- Each tab uses a `launch` value. Set it to `null` to open a plain shell.
- Launch commands resolve through your shell PATH, so bare commands like `nvim` or `cargo` work when your shell can find them.

### Minimal project config

```json
{
  "projects": [
    {
      "name": "Project 1",
      "path": "~/projects/project-1",
      "tabs": [
        { "launch": "claude" },
        { "launch": null }
      ]
    }
  ]
}
```

### Auto-start Prawjector in your terminal

Configure your terminal to run `prawjector start` through your shell so `zellij` and project launch commands inherit the same `PATH` your terminal normally provides. These examples use `zsh`; replace `/bin/zsh`, `zsh`, or `prawjector` with your shell or an absolute binary path when needed.

For Alacritty 0.14 and newer, add this to `~/.config/alacritty/alacritty.toml`:

```toml
[terminal.shell]
program = "/bin/zsh"
args = ["-l", "-i", "-c", "prawjector start; exec zsh -l"]
```

Alacritty 0.13.x TOML configs used `[shell]` with the same keys:

```toml
[shell]
program = "/bin/zsh"
args = ["-l", "-i", "-c", "prawjector start; exec zsh -l"]
```

Alacritty 0.13.0 migrated configs from YAML to TOML, and Alacritty 0.14.0 moved `shell` to `terminal.shell`. Run `alacritty migrate` if you are updating an older config.

For Ghostty, add this to `~/.config/ghostty/config`:

```ini
initial-command = /bin/zsh -l -i -c 'prawjector start; exec zsh -l'
```

Ghostty's `initial-command` only runs for the first terminal surface created at startup. Use `command = /bin/zsh -l -i -c 'prawjector start; exec zsh -l'` instead if every new Ghostty window, tab, or split should start Prawjector.

## Usage

### Launch projects

```sh
prawjector start
```

- Run `prawjector start` to open the project picker. Running bare `prawjector` prints the help screen.
- `Up` / `Down` move the selection through the list. Selection stops at the first and last entries.
- Type a number to select an indexed entry; `0` selects `Empty Session`. Use `Backspace` to edit typed numeric input.
- `Space` toggles `new session` mode for configured projects, `Enter` launches the selected or typed entry, and `q` / `Esc` exits without launching.

### Create and check config

```sh
prawjector make-config
prawjector validate-config
```

- `prawjector make-config` creates an example JSON config at the selected config path when it does not already exist.
- `prawjector validate-config` reads the selected config, reports invalid project entries, and prints the project count when validation succeeds.

### Use alternate config files

```sh
prawjector --config ./prawjector.local.json start
prawjector validate-config --config ./prawjector.local.json
```

- `--config <PATH>` uses a specific config file instead of the default `~/.prawjector/prawjector.json`.
- The option is global and works before or after `start`, `validate-config`, `make-config`, `add`, and `remove`.

### Add projects from the CLI

```sh
prawjector add
prawjector add --name "My Project" --path ~/projects/my-project --tab nvim --tab - --tab "cargo test"
```

- `prawjector add` appends the current directory to the config, derives the project name from the folder, and creates one plain shell tab.
- `--name <NAME>` overrides the derived project name, and `--path <PATH>` adds a project from a different directory.
- Repeat `--tab <LAUNCH>` to define tabs in order. Use `--tab -` for a plain shell tab with `launch: null`.
- When no `--tab` flags are provided, Prawjector adds one plain shell tab by default.

### Remove projects from config

```sh
prawjector remove
prawjector remove --force
```

- `prawjector remove` removes project entries whose expanded path matches the current working directory.
- Removal prompts for confirmation by default. Use `--force` to skip the prompt after verifying the current directory matches the project you want to remove.

### Get help

```sh
prawjector
prawjector --help
prawjector help add
prawjector remove --help
```

- The generated help lists `start`, `validate-config`, `make-config`, `add`, `remove`, and `help`.
- Use `-h` or `--help` on any command to print its help. Use `-V` or `--version` to print the binary version.

## FAQ

### Does Prawjector require Zellij?

Yes. Prawjector uses Zellij to create and attach to project sessions, so `zellij` must be installed and available on your `PATH`.

### Can Prawjector start an empty session?

Yes. Select `Empty Session` or press `0`, then press Enter to open a fresh Zellij session with one plain shell tab.

### Can Prawjector use another config file?

Yes. Pass `--config <PATH>` before or after supported subcommands to use another config file instead of the default `~/.prawjector/prawjector.json`.

### How do I remove a project?

Run `prawjector remove` from the project directory. Prawjector matches the current directory against expanded project paths and asks for confirmation unless you pass `--force`.

### Can Prawjector launch when my terminal opens?

Yes. See [Auto-start Prawjector in your terminal](#auto-start-prawjector-in-your-terminal) for Alacritty and Ghostty examples.

### Which platforms are supported?

prawjector has only been tested on Linux.

(c) 2026 Prawjector. All rights reserved.
