export const site_content = {
  nav_items: [
    { href: "#installation", label: "Install" },
    { href: "#configuration", label: "Configure" },
    { href: "#usage", label: "Usage" },
    { href: "#faq", label: "FAQ" },
  ],
  header_github: {
    label: "GitHub",
    aria_label: "GitHub (opens in a new tab)",
    href: "https://github.com/brendon-codes/prawjector",
  },
  hero: {
    title: ["A terminal project launcher"],
    cells: [
      {
        body: "Prawjector is a keyboard-driven project launcher for developers who prefer to stay in the terminal. It opens named, persistent Zellij sessions and recreates pre-configured tab layouts on each launch. Reattaching to an existing session resumes the exact tabs and shells that were active before.",
      },
      {
        body: "Prawjector works with most existing Zellij configurations without modification and does not add a daemon or background process. Projects are declared in a default JSON config at `~/.prawjector/prawjector.json`, or in another file selected with `--config <PATH>`.",
      },
    ],
    video_title: "Prawjector terminal walkthrough",
  },
  installation: {
    methods: [
      {
        title: "Install from GitHub releases",
        commands: [
          "tar -xzf prawjector-*-x86_64-linux-musl.tar.gz",
          "mkdir -p ~/.local/bin",
          "install -m 755 prawjector-*-x86_64-linux-musl/prawjector ~/.local/bin/prawjector",
          "prawjector make-config",
        ],
        notes: [
          "Download the archive for your Linux platform from the latest GitHub release.",
          "`prawjector-*-x86_64-linux-musl.tar.gz` supports most x86_64 Linux systems when you want the most portable binary.",
          "`prawjector-*-aarch64-linux-musl.tar.gz` supports most ARM64 Linux systems when you want the most portable binary.",
          "`prawjector-*-x86_64-linux-gnu.tar.gz` supports x86_64 glibc Linux systems.",
          "`prawjector-*-aarch64-linux-gnu.tar.gz` supports ARM64 glibc Linux systems.",
          "Use a MUSL archive if you use Alpine Linux, are unsure which libc your system uses, or want the fewest runtime library assumptions.",
          "Use a GNU archive when you specifically want a glibc-targeted binary. GNU archives target glibc 2.17 or newer.",
          "Verify downloads with the release `SHA256SUMS` file before installing when possible.",
          "Zellij must be installed first. Prawjector relies on it for every launch.",
        ],
      },
      {
        title: "Build from source",
        commands: ["cargo install --path .", "prawjector make-config"],
        notes: [
          "The install command uses the current checkout path. Build prawjector from a local clone of the upstream repository.",
          "Zellij must be installed first. Prawjector relies on it for every launch.",
          "Source builds require Rust stable, a C toolchain, `pkg-config`, and zlib development headers or a static zlib build setup.",
          "OpenSSL is not required for Prawjector's default local Git metadata support.",
          "`prawjector make-config` creates an example JSON config at the selected config path and leaves existing files untouched.",
          "Run `prawjector validate-config` after editing your projects to catch invalid entries before launch.",
        ],
      },
    ],
  },
  configuration: {
    intro:
      "Config is plain JSON. The `tabs` array per project is what captures the startup shape you want.",
    command: "prawjector make-config",
    validation_command: "prawjector validate-config",
    notes: [
      "`prawjector make-config` writes the default `~/.prawjector/prawjector.json` with starter projects to edit.",
      "Use `--config <PATH>` before or after supported subcommands to select another config file.",
      "Each project needs `name`, `path`, and a `tabs` array.",
      "Each tab uses a `launch` value. Set it to `null` to open a plain shell.",
      "Launch commands resolve through your shell PATH, so bare commands like `nvim` or `cargo` work when your shell can find them.",
    ],
    example: `{
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
}`,
    auto_startup: {
      title: "Auto-start Prawjector in your terminal",
      intro:
        "Configure your terminal to run `prawjector start` through your shell so `zellij` and project launch commands inherit the same `PATH` your terminal normally provides.",
      notes: [
        "These examples use `zsh`; replace `/bin/zsh`, `zsh`, or `prawjector` with your shell or an absolute binary path when needed.",
      ],
      terminals: [
        {
          title: "Alacritty 0.14 and newer",
          description: "Add this to `~/.config/alacritty/alacritty.toml`.",
          config: `[terminal.shell]
program = "/bin/zsh"
args = ["-l", "-i", "-c", "prawjector start; exec zsh -l"]`,
          notes: ["Alacritty 0.14.0 moved `shell` to `terminal.shell`."],
        },
        {
          title: "Alacritty 0.13.x",
          description: "TOML configs used `[shell]` with the same keys.",
          config: `[shell]
program = "/bin/zsh"
args = ["-l", "-i", "-c", "prawjector start; exec zsh -l"]`,
          notes: [
            "Alacritty 0.13.0 migrated configs from YAML to TOML. Run `alacritty migrate` if you are updating an older config.",
          ],
        },
        {
          title: "Ghostty startup surface",
          description: "Add this to `~/.config/ghostty/config`.",
          config:
            "initial-command = /bin/zsh -l -i -c 'prawjector start; exec zsh -l'",
          notes: [
            "Ghostty's `initial-command` only runs for the first terminal surface created at startup.",
          ],
        },
        {
          title: "Ghostty every surface",
          description:
            "Use this when every new Ghostty window, tab, or split should start Prawjector.",
          config: "command = /bin/zsh -l -i -c 'prawjector start; exec zsh -l'",
          notes: [],
        },
      ],
    },
  },
  usage: {
    guide_sections: [
      {
        title: "Launch projects",
        commands: ["prawjector start"],
        notes: [
          "Run `prawjector start` to open the project picker. Running bare `prawjector` prints the help screen.",
          "`Up` / `Down` move the selection through the list. Selection stops at the first and last entries.",
          "Type a number to select an indexed entry; `0` selects `Empty Session`. Use `Backspace` to edit typed numeric input.",
          "`Space` toggles `new session` mode for configured projects, `Enter` launches the selected or typed entry, and `q` / `Esc` exits without launching.",
        ],
      },
      {
        title: "Create and check config",
        commands: ["prawjector make-config", "prawjector validate-config"],
        notes: [
          "`prawjector make-config` creates an example JSON config at the selected config path when it does not already exist.",
          "`prawjector validate-config` reads the selected config, reports invalid project entries, and prints the project count when validation succeeds.",
        ],
      },
      {
        title: "Use alternate config files",
        commands: [
          "prawjector --config ./prawjector.local.json start",
          "prawjector validate-config --config ./prawjector.local.json",
        ],
        notes: [
          "`--config <PATH>` uses a specific config file instead of the default `~/.prawjector/prawjector.json`.",
          "The option is global and works before or after `start`, `validate-config`, `make-config`, `add`, and `remove`.",
        ],
      },
      {
        title: "Add projects from the CLI",
        commands: [
          "prawjector add",
          'prawjector add --name "My Project" --path ~/projects/my-project --tab nvim --tab - --tab "cargo test"',
        ],
        notes: [
          "`prawjector add` appends the current directory to the config, derives the project name from the folder, and creates one plain shell tab.",
          "`--name <NAME>` overrides the derived project name, and `--path <PATH>` adds a project from a different directory.",
          "Repeat `--tab <LAUNCH>` to define tabs in order. Use `--tab -` for a plain shell tab with `launch: null`.",
          "When no `--tab` flags are provided, Prawjector adds one plain shell tab by default.",
        ],
      },
      {
        title: "Remove projects from config",
        commands: ["prawjector remove", "prawjector remove --force"],
        notes: [
          "`prawjector remove` removes project entries whose expanded path matches the current working directory.",
          "Removal prompts for confirmation by default. Use `--force` to skip the prompt after verifying the current directory matches the project you want to remove.",
        ],
      },
      {
        title: "Get help",
        commands: [
          "prawjector",
          "prawjector --help",
          "prawjector help add",
          "prawjector remove --help",
        ],
        notes: [
          "The generated help lists `start`, `validate-config`, `make-config`, `add`, `remove`, and `help`.",
          "Use `-h` or `--help` on any command to print its help. Use `-V` or `--version` to print the binary version.",
        ],
      },
    ],
  },
  faq: [
    {
      question: "Does Prawjector require Zellij?",
      answer:
        "Yes. Prawjector uses Zellij to create and attach to project sessions, so `zellij` must be installed and available on your `PATH`.",
    },
    {
      question: "Can Prawjector start an empty session?",
      answer:
        "Yes. Select `Empty Session` or press `0`, then press Enter to open a fresh Zellij session with one plain shell tab.",
    },
    {
      question: "Can Prawjector use another config file?",
      answer:
        "Yes. Pass `--config <PATH>` before or after supported subcommands to use another config file instead of the default `~/.prawjector/prawjector.json`.",
    },
    {
      question: "How do I remove a project?",
      answer:
        "Run `prawjector remove` from the project directory. Prawjector matches the current directory against expanded project paths and asks for confirmation unless you pass `--force`.",
    },
    {
      question: "Can Prawjector launch when my terminal opens?",
      answer:
        "Yes. Configure your terminal to run `prawjector start` through a shell or login shell so `zellij` and launch commands inherit the PATH your shell normally provides.",
    },
    {
      question: "Which platforms are supported?",
      answer: "prawjector has only been tested on Linux.",
    },
  ],
  footer: {
    copyright_year: 2026,
    notice: "Prawjector. All rights reserved.",
  },
} as const
