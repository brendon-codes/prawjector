mod app;
mod config;
mod git_info;
mod ui;
mod zellij;

use clap::{CommandFactory, Parser, Subcommand};
use crossterm::event::{self, Event, KeyEventKind};
use std::path::Path;
use std::time::Duration;

enum LaunchAction {
    Quit,
    LaunchProject(usize, bool),
    LaunchEmpty,
}

#[derive(Parser)]
#[command(
    name = "prawjector",
    version,
    about = "Terminal project launcher for Zellij",
    long_about = "Terminal project launcher for Zellij. Pick a configured project from an interactive terminal UI, then open it in Zellij with its configured tabs."
)]
struct Cli {
    #[arg(
        long,
        value_name = "PATH",
        global = true,
        help = "Use a specific prawjector config file instead of the default.",
        long_help = "Use a specific prawjector config file instead of the default ~/.prawjector/prawjector.json. This applies to start, validate-config, make-config, add, and remove."
    )]
    config: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        about = "Open the interactive project launcher",
        long_about = "Open the interactive terminal UI, let you select a configured project, and launch it in Zellij. This is the command to use from terminal startup hooks."
    )]
    Start,
    #[command(
        about = "Validate the project configuration file",
        long_about = "Read the selected configuration file, report invalid project entries, and print the number of configured projects when validation succeeds."
    )]
    ValidateConfig,
    #[command(
        about = "Create an example project configuration file",
        long_about = "Create an example JSON configuration file at the selected config path. Existing files are left untouched."
    )]
    MakeConfig,
    #[command(
        about = "Add a project entry to the configuration file",
        long_about = "Add a project to the selected configuration file. When options are omitted, the current directory is used for the project path, its directory name is used for the project name, and one plain shell tab is created."
    )]
    Add {
        #[arg(
            long,
            help = "Set the project name stored in the config.",
            long_help = "Set the project name stored in the config. When omitted, prawjector derives a title from the project directory name."
        )]
        name: Option<String>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Set the project directory path stored in the config.",
            long_help = "Set the project directory path stored in the config. Supports absolute paths and paths beginning with ~. When omitted, the current working directory is used."
        )]
        path: Option<String>,
        #[arg(
            long = "tab",
            value_name = "LAUNCH",
            help = "Add a tab launch command for the project.",
            long_help = "Add a tab launch command for the project. For prawjector add, specify multiple --tab flags to define tabs in order, for example --tab nvim --tab - --tab \"cargo test\". Use --tab - to create a plain shell tab with launch set to null. When no --tab is specified, one plain shell tab is created."
        )]
        tabs: Vec<String>,
    },
    #[command(
        about = "Remove the current directory from the configuration file",
        long_about = "Remove project entries whose expanded path matches the current working directory in the selected configuration file. Prompts for confirmation unless --force is used."
    )]
    Remove {
        #[arg(
            long,
            help = "Remove matching project entries without prompting.",
            long_help = "Remove matching project entries without prompting for confirmation. Use this for scripts or when you have already verified the current directory matches the project you want to remove."
        )]
        force: bool,
    },
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Some(command) => {
            let config_path = config::config_path_from_arg(cli.config.as_deref())?;
            match command {
                Command::Start => run_tui(&config_path),
                Command::ValidateConfig => run_validate_config(&config_path),
                Command::MakeConfig => run_make_config(&config_path),
                Command::Add { name, path, tabs } => run_add(&config_path, name, path, tabs),
                Command::Remove { force } => run_remove(&config_path, force),
            }
        }
        None => run_help(),
    }
}

fn run_help() -> color_eyre::Result<()> {
    Cli::command().print_help()?;
    println!();
    Ok(())
}

fn run_validate_config(config_path: &Path) -> color_eyre::Result<()> {
    let config = config::load_config(config_path)?;
    let errors = config::validate_config(&config);

    if errors.is_empty() {
        println!("Config is valid.");
        println!("{} project(s) configured.", config.projects.len());
        Ok(())
    } else {
        eprintln!("Config validation failed:");
        errors.iter().for_each(|e| eprintln!("  - {}", e));
        std::process::exit(1);
    }
}

fn run_make_config(config_path: &Path) -> color_eyre::Result<()> {
    config::make_config(config_path)
}

fn run_add(
    config_path: &Path,
    name: Option<String>,
    path: Option<String>,
    tabs: Vec<String>,
) -> color_eyre::Result<()> {
    config::add_project(config_path, config::AddProjectOptions { name, path, tabs })
}

fn run_remove(config_path: &Path, force: bool) -> color_eyre::Result<()> {
    config::remove_project(config_path, force)
}

fn run_tui(config_path: &Path) -> color_eyre::Result<()> {
    let shlvl: u32 = std::env::var("SHLVL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if shlvl < 1 {
        eprintln!("Error: prawjector must be run from within a shell (SHLVL not set).");
        eprintln!("Launch it from your terminal or configure your terminal to run a login shell.");
        std::process::exit(1);
    }

    let config = config::load_config(config_path)?;
    let mut app = app::App::new(&config);

    let mut terminal = ratatui::init();
    let action = run_app(&mut terminal, &mut app, &config);
    ratatui::restore();

    match action? {
        LaunchAction::Quit => Ok(()),
        LaunchAction::LaunchEmpty => zellij::launch_empty(),
        LaunchAction::LaunchProject(idx, new_session) => {
            zellij::launch(&config.projects[idx], new_session)
        }
    }
}

fn run_app(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut app::App,
    config: &config::Config,
) -> color_eyre::Result<LaunchAction> {
    let total_options = config.projects.len() + 1;

    loop {
        let padding_mode = ui::detect_padding_mode();
        terminal.draw(|frame| ui::draw(frame, app, config, padding_mode))?;

        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let Some((selected, new_session)) = app.handle_key(key, total_options)
        {
            return if selected == 0 {
                Ok(LaunchAction::LaunchEmpty)
            } else {
                Ok(LaunchAction::LaunchProject(selected - 1, new_session))
            };
        }

        app.tick();

        if app.should_quit {
            return Ok(LaunchAction::Quit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_start_command() {
        let cli = Cli::try_parse_from(["prawjector", "start"]).unwrap();

        match cli.command {
            Some(Command::Start) => {}
            _ => panic!("expected start command"),
        }
    }

    #[test]
    fn test_parse_no_command() {
        let cli = Cli::try_parse_from(["prawjector"]).unwrap();

        assert!(cli.command.is_none());
        assert_eq!(cli.config, None);
    }

    #[test]
    fn test_parse_config_option_before_command() {
        let cli = Cli::try_parse_from(["prawjector", "--config", "/tmp/prawjector.json", "start"])
            .unwrap();

        assert_eq!(cli.config, Some("/tmp/prawjector.json".to_string()));
        match cli.command {
            Some(Command::Start) => {}
            _ => panic!("expected start command"),
        }
    }

    #[test]
    fn test_parse_config_option_after_command() {
        let cli = Cli::try_parse_from(["prawjector", "start", "--config", "/tmp/prawjector.json"])
            .unwrap();

        assert_eq!(cli.config, Some("/tmp/prawjector.json".to_string()));
        match cli.command {
            Some(Command::Start) => {}
            _ => panic!("expected start command"),
        }
    }

    #[test]
    fn test_help_includes_start_command() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("start"));
    }

    #[test]
    fn test_help_includes_remove_command() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("remove"));
    }

    #[test]
    fn test_help_includes_config_option() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("--config <PATH>"));
        assert!(help.contains("specific prawjector config file"));
    }

    #[test]
    fn test_add_help_mentions_multiple_tab_flags() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("add")
            .unwrap()
            .render_long_help()
            .to_string();

        assert!(help.contains("specify multiple --tab flags"));
    }

    #[test]
    fn test_parse_add_options() {
        let cli = Cli::try_parse_from([
            "prawjector",
            "add",
            "--name",
            "Test Project",
            "--path",
            "/tmp/test-project",
            "--tab",
            "nvim",
            "--tab",
            "-",
        ])
        .unwrap();

        match cli.command {
            Some(Command::Add { name, path, tabs }) => {
                assert_eq!(name, Some("Test Project".to_string()));
                assert_eq!(path, Some("/tmp/test-project".to_string()));
                assert_eq!(tabs, vec!["nvim".to_string(), "-".to_string()]);
            }
            _ => panic!("expected add command"),
        }
    }

    #[test]
    fn test_parse_remove_command() {
        let cli = Cli::try_parse_from(["prawjector", "remove"]).unwrap();

        match cli.command {
            Some(Command::Remove { force }) => {
                assert!(!force);
            }
            _ => panic!("expected remove command"),
        }
    }

    #[test]
    fn test_parse_remove_force_option() {
        let cli = Cli::try_parse_from(["prawjector", "remove", "--force"]).unwrap();

        match cli.command {
            Some(Command::Remove { force }) => {
                assert!(force);
            }
            _ => panic!("expected remove command"),
        }
    }
}
