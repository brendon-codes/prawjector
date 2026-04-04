use crate::config::Project;
use kdl::KdlDocument;
use std::fs;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const SESSION_REMOVAL_POLL_INTERVAL: Duration = Duration::from_millis(20);
const SESSION_REMOVAL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq)]
enum SessionState {
    Running,
    Exited,
    NotFound,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SavedLayoutState {
    HasTabs,
    Empty,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LaunchDecision {
    Attach,
    CreateNewSession,
    DeleteAndCreateSession,
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
        remove_session(state, session_name)?;
        return create_new_session(session_name, cwd, project);
    }

    let saved_layout = match state {
        SessionState::Exited => classify_saved_session_layout(session_name),
        SessionState::Running | SessionState::NotFound => SavedLayoutState::Unknown,
    };

    match decide_launch(state, saved_layout) {
        LaunchDecision::Attach => attach_session(session_name, cwd),
        LaunchDecision::CreateNewSession => create_new_session(session_name, cwd, project),
        LaunchDecision::DeleteAndCreateSession => {
            remove_session(SessionState::Exited, session_name)?;
            create_new_session(session_name, cwd, project)
        }
    }
}

fn decide_launch(state: SessionState, saved_layout: SavedLayoutState) -> LaunchDecision {
    match state {
        SessionState::Running => LaunchDecision::Attach,
        SessionState::Exited => match saved_layout {
            SavedLayoutState::HasTabs | SavedLayoutState::Unknown => LaunchDecision::Attach,
            SavedLayoutState::Empty => LaunchDecision::DeleteAndCreateSession,
        },
        SessionState::NotFound => LaunchDecision::CreateNewSession,
    }
}

fn attach_args(session_name: &str) -> Vec<String> {
    vec!["attach".to_string(), session_name.to_string()]
}

fn session_cleanup_args(state: SessionState, session_name: &str) -> Option<Vec<String>> {
    match state {
        SessionState::Running => Some(vec![
            "delete-session".to_string(),
            "--force".to_string(),
            session_name.to_string(),
        ]),
        SessionState::Exited => Some(vec!["delete-session".to_string(), session_name.to_string()]),
        SessionState::NotFound => None,
    }
}

fn remove_session(state: SessionState, session_name: &str) -> color_eyre::Result<()> {
    match session_cleanup_args(state, session_name) {
        Some(args) => {
            Command::new("zellij").args(&args).status()?;
            wait_for_session_removal(session_name)
        }
        None => Ok(()),
    }
}

fn wait_for_session_removal(session_name: &str) -> color_eyre::Result<()> {
    let deadline = Instant::now() + SESSION_REMOVAL_TIMEOUT;
    loop {
        match find_session(session_name) {
            SessionState::NotFound => return Ok(()),
            state if Instant::now() >= deadline => {
                return Err(color_eyre::eyre::eyre!(
                    "Timed out waiting for zellij session '{}' to be removed (last state: {:?}); run 'zellij delete-session --force {}' manually and retry",
                    session_name,
                    state,
                    session_name
                ));
            }
            SessionState::Exited => {
                Command::new("zellij")
                    .args(["delete-session", session_name])
                    .status()?;
            }
            SessionState::Running => {}
        }
        thread::sleep(SESSION_REMOVAL_POLL_INTERVAL);
    }
}

fn attach_session(session_name: &str, cwd: &Path) -> color_eyre::Result<()> {
    let err = Command::new("zellij")
        .args(attach_args(session_name))
        .current_dir(cwd)
        .exec();
    Err(color_eyre::eyre::eyre!("Failed to exec zellij: {}", err))
}

fn create_new_session(session_name: &str, cwd: &Path, project: &Project) -> color_eyre::Result<()> {
    let status = Command::new("zellij")
        .args(create_background_args(session_name))
        .current_dir(cwd)
        .status()?;
    if !status.success() {
        return Err(color_eyre::eyre::eyre!(
            "Failed to create zellij session '{}'",
            session_name
        ));
    }

    if let Some(script) = build_setup_script(session_name, project) {
        Command::new("bash")
            .args(["-c", &script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    attach_session(session_name, cwd)
}

fn create_background_args(session_name: &str) -> Vec<String> {
    vec![
        "attach".to_string(),
        "--create-background".to_string(),
        session_name.to_string(),
    ]
}

fn build_setup_script(session_name: &str, project: &Project) -> Option<String> {
    let escaped_session = shell_escape(session_name);

    let initial_write = project
        .tabs
        .first()
        .and_then(|tab| tab.launch.as_ref())
        .map(|cmd| {
            format!(
                "zellij -s {} action write-chars {}",
                escaped_session,
                shell_escape(&format!("{}\n", expand_launch_command(cmd).join(" ")))
            )
        });

    let tab_lines: Vec<String> = build_tab_commands(session_name, project)
        .iter()
        .map(|args| {
            let escaped_args: Vec<String> = args.iter().map(|arg| shell_escape(arg)).collect();
            format!("zellij -s {} {}", escaped_session, escaped_args.join(" "))
        })
        .collect();

    if initial_write.is_none() && tab_lines.is_empty() {
        return None;
    }

    let wait_for_attach = format!(
        "tries=0\nwhile ! zellij -s {} action list-clients 2>/dev/null | tail -n +2 | grep -q .; do\n  tries=$((tries + 1))\n  if [ \"$tries\" -ge 50 ]; then exit 1; fi\n  sleep 0.1\ndone\nsleep 0.2",
        escaped_session
    );

    let focus_first_tab = (!tab_lines.is_empty())
        .then(|| format!("zellij -s {} action go-to-tab 1", escaped_session));

    let lines: Vec<String> = std::iter::once(wait_for_attach)
        .chain(initial_write)
        .chain(tab_lines)
        .chain(focus_first_tab)
        .collect();

    Some(lines.join("\n") + "\n")
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

fn classify_saved_session_layout(session_name: &str) -> SavedLayoutState {
    saved_session_layout_path(session_name)
        .map(|path| classify_saved_layout_file(&path))
        .unwrap_or(SavedLayoutState::Unknown)
}

fn saved_session_layout_path(session_name: &str) -> Option<PathBuf> {
    dirs::cache_dir().map(|cache_dir| {
        cache_dir
            .join("zellij")
            .join("contract_version_1")
            .join("session_info")
            .join(session_name)
            .join("session-layout.kdl")
    })
}

fn classify_saved_layout_file(path: &Path) -> SavedLayoutState {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return SavedLayoutState::Empty,
        Err(_) => return SavedLayoutState::Unknown,
    };

    match contents.parse::<KdlDocument>() {
        Ok(document) => classify_saved_layout_document(&document),
        Err(_) => SavedLayoutState::Unknown,
    }
}

fn classify_saved_layout_document(document: &KdlDocument) -> SavedLayoutState {
    let has_tabs = document
        .nodes()
        .iter()
        .filter(|node| node.name().value() == "layout")
        .filter_map(|node| node.children())
        .flat_map(|children| children.nodes().iter())
        .any(|node| node.name().value() == "tab");

    if has_tabs {
        SavedLayoutState::HasTabs
    } else {
        SavedLayoutState::Empty
    }
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
    fn test_create_background_args() {
        assert_eq!(
            create_background_args("my-project"),
            vec!["attach", "--create-background", "my-project"]
        );
    }

    #[test]
    fn test_build_setup_script_with_commands() {
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

        let script = build_setup_script("test", &project).unwrap();
        assert!(script.contains("zellij -s 'test' action list-clients"));
        assert!(!script.contains("list-sessions"));
        assert!(script.contains("write-chars"));
        assert!(script.contains("action go-to-tab 1"));
        assert!(script.contains("'action' 'new-tab'"));
    }

    #[test]
    fn test_build_setup_script_no_initial_command() {
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

        let script = build_setup_script("test", &project).unwrap();
        assert!(!script.contains("write-chars"));
        assert!(script.contains("'action' 'new-tab'"));
    }

    #[test]
    fn test_build_setup_script_single_tab_without_launch_is_none() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![Tab { launch: None }],
        };

        assert_eq!(build_setup_script("test", &project), None);
    }

    #[test]
    fn test_build_setup_script_single_tab_with_launch_skips_go_to_tab() {
        let project = Project {
            name: "Test".to_string(),
            path: "/tmp/test".to_string(),
            tabs: vec![Tab {
                launch: Some("nvim".to_string()),
            }],
        };

        let script = build_setup_script("test", &project).unwrap();
        assert!(script.contains("write-chars"));
        assert!(!script.contains("go-to-tab"));
        assert!(!script.contains("new-tab"));
    }

    #[test]
    fn test_classify_saved_layout_with_direct_tab() {
        let document = r#"
            layout {
                tab name="main"
            }
        "#
        .parse::<KdlDocument>()
        .unwrap();

        assert_eq!(
            classify_saved_layout_document(&document),
            SavedLayoutState::HasTabs
        );
    }

    #[test]
    fn test_classify_saved_layout_ignores_templates_and_nested_tabs() {
        let document = r#"
            layout {
                new_tab_template {
                    pane
                }
                swap_tiled_layout name="stacked" {
                    tab
                }
                plugin_template name="status" {
                    tab
                }
            }
        "#
        .parse::<KdlDocument>()
        .unwrap();

        assert_eq!(
            classify_saved_layout_document(&document),
            SavedLayoutState::Empty
        );
    }

    #[test]
    fn test_classify_missing_saved_layout_file_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-session-layout.kdl");

        assert_eq!(classify_saved_layout_file(&path), SavedLayoutState::Empty);
    }

    #[test]
    fn test_classify_invalid_saved_layout_file_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session-layout.kdl");
        fs::write(&path, "layout {").unwrap();

        assert_eq!(classify_saved_layout_file(&path), SavedLayoutState::Unknown);
    }

    #[test]
    fn test_exited_with_saved_tabs_attaches_without_force_run_commands() {
        let args = attach_args("my-project");

        assert_eq!(
            decide_launch(SessionState::Exited, SavedLayoutState::HasTabs),
            LaunchDecision::Attach
        );
        assert_eq!(args, vec!["attach", "my-project"]);
        assert!(!args.contains(&"--force-run-commands".to_string()));
    }

    #[test]
    fn test_exited_with_unknown_saved_layout_attaches_without_deleting() {
        let args = attach_args("my-project");

        assert_eq!(
            decide_launch(SessionState::Exited, SavedLayoutState::Unknown),
            LaunchDecision::Attach
        );
        assert_eq!(args, vec!["attach", "my-project"]);
        assert!(!args.contains(&"--force-run-commands".to_string()));
    }

    #[test]
    fn test_exited_with_empty_saved_layout_deletes_and_creates() {
        assert_eq!(
            decide_launch(SessionState::Exited, SavedLayoutState::Empty),
            LaunchDecision::DeleteAndCreateSession
        );
    }

    #[test]
    fn test_running_session_attaches() {
        assert_eq!(
            decide_launch(SessionState::Running, SavedLayoutState::Empty),
            LaunchDecision::Attach
        );
        assert_eq!(attach_args("my-project"), vec!["attach", "my-project"]);
    }

    #[test]
    fn test_not_found_session_creates_new_session() {
        assert_eq!(
            decide_launch(SessionState::NotFound, SavedLayoutState::HasTabs),
            LaunchDecision::CreateNewSession
        );
    }

    #[test]
    fn test_session_cleanup_args_running_force_deletes() {
        assert_eq!(
            session_cleanup_args(SessionState::Running, "my-project"),
            Some(vec![
                "delete-session".to_string(),
                "--force".to_string(),
                "my-project".to_string()
            ])
        );
    }

    #[test]
    fn test_session_cleanup_args_exited_deletes_without_force() {
        assert_eq!(
            session_cleanup_args(SessionState::Exited, "my-project"),
            Some(vec!["delete-session".to_string(), "my-project".to_string()])
        );
    }

    #[test]
    fn test_session_cleanup_args_not_found_is_none() {
        assert_eq!(
            session_cleanup_args(SessionState::NotFound, "my-project"),
            None
        );
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
