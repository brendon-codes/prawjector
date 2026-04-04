use crate::config::Project;
use std::os::unix::process::CommandExt;
use std::process::Command;

#[derive(Debug, PartialEq)]
enum SessionState {
    Running,
    Exited,
    NotFound,
}

pub fn launch(project: &Project, new_session: bool) -> color_eyre::Result<()> {
    let session_name = sanitize_session_name(&project.name);
    let cwd = project.expanded_path();

    if std::env::var("ZELLIJ").is_ok() {
        launch_inside_zellij(&session_name, project)
    } else {
        launch_outside_zellij(&session_name, &cwd, project, new_session)
    }
}

fn launch_inside_zellij(_session_name: &str, project: &Project) -> color_eyre::Result<()> {
    let cwd = project.expanded_path();
    let cwd_str = cwd.to_string_lossy();

    for tab in &project.tabs {
        let mut args = vec![
            "action".to_string(),
            "new-tab".to_string(),
            "--cwd".to_string(),
            cwd_str.to_string(),
        ];
        if let Some(cmd) = &tab.launch {
            args.push("--".to_string());
            args.extend(expand_launch_command(cmd));
        }
        Command::new("zellij").args(&args).status()?;
    }

    Command::new("zellij")
        .args(["action", "go-to-tab", "1"])
        .status()?;

    Ok(())
}

fn launch_outside_zellij(
    session_name: &str,
    cwd: &std::path::Path,
    project: &Project,
    new_session: bool,
) -> color_eyre::Result<()> {
    let state = find_session(session_name);

    if new_session {
        match state {
            SessionState::Running => {
                Command::new("zellij")
                    .args(["kill-session", session_name])
                    .status()?;
                Command::new("zellij")
                    .args(["delete-session", session_name])
                    .status()?;
            }
            SessionState::Exited => {
                Command::new("zellij")
                    .args(["delete-session", session_name])
                    .status()?;
            }
            SessionState::NotFound => {}
        }
        return create_new_session(session_name, cwd, project);
    }

    match state {
        SessionState::Running => {
            let err = Command::new("zellij")
                .args(["attach", session_name])
                .current_dir(cwd)
                .exec();
            Err(color_eyre::eyre::eyre!("Failed to exec zellij: {}", err))
        }
        SessionState::Exited => {
            let err = Command::new("zellij")
                .args(["attach", "--force-run-commands", session_name])
                .current_dir(cwd)
                .exec();
            Err(color_eyre::eyre::eyre!("Failed to exec zellij: {}", err))
        }
        SessionState::NotFound => create_new_session(session_name, cwd, project),
    }
}

fn create_new_session(
    session_name: &str,
    cwd: &std::path::Path,
    project: &Project,
) -> color_eyre::Result<()> {
    let script = build_background_script(session_name, project);

    Command::new("bash")
        .args(["-c", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let err = Command::new("zellij")
        .args(["-s", session_name])
        .current_dir(cwd)
        .exec();
    Err(color_eyre::eyre::eyre!("Failed to exec zellij: {}", err))
}

fn build_background_script(session_name: &str, project: &Project) -> String {
    let escaped_session = shell_escape(session_name);

    let mut script = format!(
        "while ! zellij list-sessions 2>/dev/null | grep -q {}; do sleep 0.1; done\nsleep 0.2\n",
        escaped_session
    );

    if let Some(initial_cmd) = project.tabs.first().and_then(|t| t.launch.as_ref()) {
        let expanded = expand_launch_command(initial_cmd).join(" ");
        script.push_str(&format!(
            "zellij -s {} action write-chars {}\n",
            escaped_session,
            shell_escape(&format!("{}\n", expanded))
        ));
    }

    let tab_commands = build_tab_commands(session_name, project);
    for args in &tab_commands {
        let escaped_args: Vec<String> = args.iter().map(|a| shell_escape(a)).collect();
        script.push_str(&format!(
            "zellij -s {} {}\n",
            escaped_session,
            escaped_args.join(" ")
        ));
    }

    script.push_str(&format!(
        "zellij -s {} action go-to-tab 1\n",
        escaped_session
    ));

    script
}

fn build_tab_commands(session_name: &str, project: &Project) -> Vec<Vec<String>> {
    let cwd = project.expanded_path();
    let cwd_str = cwd.to_string_lossy().to_string();
    let _ = session_name;

    project
        .tabs
        .iter()
        .skip(1)
        .map(|tab| {
            let mut args = vec![
                "action".to_string(),
                "new-tab".to_string(),
                "--cwd".to_string(),
                cwd_str.clone(),
            ];
            if let Some(cmd) = &tab.launch {
                args.push("--".to_string());
                args.extend(expand_launch_command(cmd));
            }
            args
        })
        .collect()
}

fn expand_launch_command(cmd: &str) -> Vec<String> {
    cmd.split_whitespace()
        .map(|part| shellexpand::tilde(part).into_owned())
        .collect()
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn launch_empty() -> color_eyre::Result<()> {
    let err = Command::new("zellij").exec();
    Err(color_eyre::eyre::eyre!("Failed to exec zellij: {}", err))
}

fn sanitize_session_name(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

fn find_session(session_name: &str) -> SessionState {
    let output = Command::new("zellij")
        .args(["list-sessions", "--no-formatting"])
        .output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return SessionState::NotFound,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_session_state(&stdout, session_name)
}

fn parse_session_state(output: &str, session_name: &str) -> SessionState {
    for line in output.lines() {
        let name = match line.split_once(" [") {
            Some((name, _)) => name,
            None => line.trim(),
        };
        if name == session_name {
            return if line.contains("(EXITED") {
                SessionState::Exited
            } else {
                SessionState::Running
            };
        }
    }
    SessionState::NotFound
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Project, Tab};

    #[test]
    fn test_build_tab_commands_with_commands() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![
                Tab {
                    launch: Some("/usr/bin/nvim".to_string()),
                },
                Tab {
                    launch: Some("/usr/bin/cargo watch -x check".to_string()),
                },
                Tab { launch: None },
            ],
        };

        let commands = build_tab_commands("test", &project);
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0],
            vec![
                "action",
                "new-tab",
                "--cwd",
                "/tmp/test",
                "--",
                "/usr/bin/cargo",
                "watch",
                "-x",
                "check"
            ]
        );
        assert_eq!(commands[1], vec!["action", "new-tab", "--cwd", "/tmp/test"]);
    }

    #[test]
    fn test_build_tab_commands_empty_tabs() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![],
        };

        let commands = build_tab_commands("test", &project);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_build_tab_commands_null_launch() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![Tab { launch: None }, Tab { launch: None }],
        };

        let commands = build_tab_commands("test", &project);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], vec!["action", "new-tab", "--cwd", "/tmp/test"]);
        assert!(!commands[0].contains(&"--".to_string()));
    }

    #[test]
    fn test_sanitize_session_name() {
        assert_eq!(sanitize_session_name("My Project"), "my-project");
        assert_eq!(sanitize_session_name("hello world!"), "hello-world");
        assert_eq!(sanitize_session_name("Test_123"), "test123");
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("path/to dir"), "'path/to dir'");
    }

    #[test]
    fn test_build_background_script_with_commands() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![
                Tab {
                    launch: Some("nvim".to_string()),
                },
                Tab {
                    launch: Some("cargo watch".to_string()),
                },
                Tab { launch: None },
            ],
        };

        let script = build_background_script("test", &project);
        assert!(script.contains("while ! zellij list-sessions"));
        assert!(script.contains("grep -q 'test'"));
        assert!(script.contains("write-chars"));
        assert!(script.contains("action go-to-tab 1"));
        assert!(script.contains("'action' 'new-tab'"));
    }

    #[test]
    fn test_build_background_script_no_initial_command() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![
                Tab { launch: None },
                Tab {
                    launch: Some("nvim".to_string()),
                },
            ],
        };

        let script = build_background_script("test", &project);
        assert!(!script.contains("write-chars"));
        assert!(script.contains("'action' 'new-tab'"));
    }

    #[test]
    fn test_parse_session_state_running() {
        let output = "my-project [Created 2h ago]\n";
        assert_eq!(
            parse_session_state(output, "my-project"),
            SessionState::Running
        );
    }

    #[test]
    fn test_parse_session_state_exited() {
        let output = "my-project [Created 2h ago] (EXITED - attach to resurrect)\n";
        assert_eq!(
            parse_session_state(output, "my-project"),
            SessionState::Exited
        );
    }

    #[test]
    fn test_parse_session_state_not_found() {
        let output = "other-project [Created 1h ago]\n";
        assert_eq!(
            parse_session_state(output, "my-project"),
            SessionState::NotFound
        );
    }

    #[test]
    fn test_parse_session_state_empty() {
        assert_eq!(
            parse_session_state("", "my-project"),
            SessionState::NotFound
        );
    }

    #[test]
    fn test_parse_session_state_exact_match_no_substring() {
        let output = "foobar [Created 1h ago]\n";
        assert_eq!(parse_session_state(output, "foo"), SessionState::NotFound);
    }

    #[test]
    fn test_parse_session_state_exact_match_no_superstring() {
        let output = "foo [Created 1h ago]\n";
        assert_eq!(
            parse_session_state(output, "foobar"),
            SessionState::NotFound
        );
    }

    #[test]
    fn test_parse_session_state_multiple_sessions() {
        let output = "alpha [Created 3h ago]\nbeta [Created 1h ago] (EXITED - attach to resurrect)\ngamma [Created 30m ago]\n";
        assert_eq!(parse_session_state(output, "beta"), SessionState::Exited);
        assert_eq!(parse_session_state(output, "gamma"), SessionState::Running);
        assert_eq!(parse_session_state(output, "delta"), SessionState::NotFound);
    }

    #[test]
    fn test_parse_session_state_current_session() {
        let output = "my-project [Created 5m ago] (current)\n";
        assert_eq!(
            parse_session_state(output, "my-project"),
            SessionState::Running
        );
    }

    #[test]
    fn test_expand_launch_command_with_tilde() {
        let parts = expand_launch_command("~/.local/bin/claude --flag ~/some/path");
        assert_eq!(parts.len(), 3);
        assert!(!parts[0].contains('~'));
        assert!(parts[0].ends_with("/.local/bin/claude"));
        assert_eq!(parts[1], "--flag");
        assert!(!parts[2].contains('~'));
        assert!(parts[2].ends_with("/some/path"));
    }

    #[test]
    fn test_expand_launch_command_bare_command() {
        let parts = expand_launch_command("ls -la");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "ls");
        assert_eq!(parts[1], "-la");
    }
}
