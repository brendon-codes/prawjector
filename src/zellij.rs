use crate::config::Project;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use time::OffsetDateTime;

const PREFIX: &str = "prawjector-";
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub age: String,
    pub current: bool,
    pub exited: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TabInfo {
    pub name: String,
    pub position: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PaneInfo {
    pub is_plugin: bool,
    pub tab_position: Option<usize>,
    #[serde(rename = "pane_cwd")]
    pub cwd: Option<String>,
    #[serde(rename = "pane_command")]
    pub terminal_command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDetails {
    pub tabs: Vec<TabInfo>,
    pub panes: Vec<PaneInfo>,
}

pub enum Update {
    Sessions(Result<Vec<Session>>),
    Details(String, Result<SessionDetails>),
    CleanupErrors(Vec<String>),
}

pub struct RefreshWorker {
    pub updates: Receiver<Update>,
    selected: Sender<Option<String>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl RefreshWorker {
    pub fn start(cleanup: bool) -> Self {
        let (send, updates) = mpsc::channel();
        let (selected, requests) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let handle = thread::spawn(move || {
            let runner = ProcessRunner { cancelled };
            let mut selection: Option<String> = None;
            let mut next_refresh = Instant::now();
            if cleanup {
                let (sessions, errors) = startup_sessions(&runner);
                let _ = send.send(Update::CleanupErrors(errors));
                let _ = send.send(Update::Sessions(sessions));
                next_refresh = Instant::now() + REFRESH_INTERVAL;
            }
            while !runner.cancelled.load(Ordering::Relaxed) {
                let mut changed = false;
                while let Ok(name) = requests.try_recv() {
                    changed |= selection != name;
                    selection = name;
                }
                let refresh = Instant::now() >= next_refresh;
                if refresh {
                    let _ = send.send(Update::Sessions(discover(&runner).map(running_managed)));
                    next_refresh = Instant::now() + REFRESH_INTERVAL;
                }
                if (refresh || changed)
                    && let Some(name) = &selection
                    && !runner.cancelled.load(Ordering::Relaxed)
                {
                    let _ = send.send(Update::Details(name.clone(), details(&runner, name)));
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
        Self {
            updates,
            selected,
            stop,
            handle: Some(handle),
        }
    }

    pub fn select(&self, name: Option<String>) {
        let _ = self.selected.send(name);
    }
}

impl Drop for RefreshWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

trait Runner {
    type SetupClient;

    fn run(&self, args: &[String], cwd: Option<&Path>, clean_environment: bool) -> Result<String>;
    fn setup_client(&self, name: &str) -> Result<Self::SetupClient>;
}

#[derive(Default)]
struct ProcessRunner {
    cancelled: Arc<AtomicBool>,
}

impl Runner for ProcessRunner {
    type SetupClient = SetupClient;

    fn setup_client(&self, name: &str) -> Result<SetupClient> {
        let client = SetupClient::start(name)?;
        wait_for_tab(self, name, None)?;
        Ok(client)
    }

    fn run(&self, args: &[String], cwd: Option<&Path>, clean_environment: bool) -> Result<String> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("Session refresh stopped".into());
        }
        let mut command = Command::new("zellij");
        command.args(args);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        if clean_environment {
            clear_session_environment(&mut command);
        }
        let timeout = if args.iter().any(|arg| arg == "--create-background") {
            Duration::from_secs(15)
        } else {
            Duration::from_secs(3)
        };
        run_bounded(command, timeout, &self.cancelled)
            .map_err(|error| format!("zellij {}: {error}", args.join(" ")))
    }
}

struct SetupClient {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    output: Option<JoinHandle<()>>,
}

impl SetupClient {
    fn start(name: &str) -> Result<Self> {
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: rows.max(24),
                cols: cols.max(80),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let mut command = CommandBuilder::new("zellij");
        command.args(["attach", name]);
        for key in ["ZELLIJ", "ZELLIJ_SESSION_NAME", "ZELLIJ_PANE_ID"] {
            command.env_remove(key);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);
        let output = thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        Ok(Self {
            child,
            output: Some(output),
        })
    }
}

impl Drop for SetupClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(output) = self.output.take() {
            let _ = output.join();
        }
    }
}

#[derive(Deserialize)]
struct SetupTab {
    tab_id: u32,
    active: bool,
    display_area_rows: usize,
    display_area_columns: usize,
    selectable_tiled_panes_count: usize,
    selectable_floating_panes_count: usize,
}

fn wait_for_tab(runner: &impl Runner, name: &str, tab_id: Option<u32>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let output = runner.run(
            &strings(&["-s", name, "action", "list-tabs", "--json"]),
            None,
            true,
        )?;
        let tabs: Vec<SetupTab> =
            serde_json::from_str(&output).map_err(|error| error.to_string())?;
        if tabs.iter().any(|tab| {
            tab_id.map_or(tab.active, |id| tab.tab_id == id)
                && tab.display_area_rows > 0
                && tab.display_area_columns > 0
                && tab.selectable_tiled_panes_count + tab.selectable_floating_panes_count > 0
        }) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Timed out waiting for session '{name}' tab {tab_id:?} to initialize"
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn clear_session_environment(command: &mut Command) {
    for key in ["ZELLIJ", "ZELLIJ_SESSION_NAME", "ZELLIJ_PANE_ID"] {
        command.env_remove(key);
    }
}

fn run_bounded(mut command: Command, timeout: Duration, cancelled: &AtomicBool) -> Result<String> {
    let empty_listing_allowed = command
        .get_args()
        .eq(["list-sessions", "--no-formatting"].map(std::ffi::OsStr::new));
    let mut execute = || -> std::io::Result<_> {
        let mut stdout = tempfile::tempfile()?;
        let mut stderr = tempfile::tempfile()?;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .spawn()?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline && !cancelled.load(Ordering::Relaxed) => {
                    thread::sleep(Duration::from_millis(20));
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(result.err().unwrap_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "command timed out or was cancelled",
                        )
                    }));
                }
            }
        };
        let mut output = String::new();
        let mut error = String::new();
        stdout.seek(SeekFrom::Start(0))?;
        stderr.seek(SeekFrom::Start(0))?;
        stdout.read_to_string(&mut output)?;
        stderr.read_to_string(&mut error)?;
        Ok((status, output, error))
    };
    let (status, output, error) = execute().map_err(|error| error.to_string())?;
    if empty_listing_allowed
        && status.code() == Some(1)
        && output.is_empty()
        && error.trim() == "No active zellij sessions found."
    {
        return Ok(String::new());
    }
    if status.success() {
        Ok(output)
    } else {
        Err(format!("{status}: {} {}", error.trim(), output.trim()))
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

fn discover(runner: &impl Runner) -> Result<Vec<Session>> {
    let output = runner.run(&strings(&["list-sessions", "--no-formatting"]), None, false)?;
    parse_sessions(&output)
}

fn parse_sessions(output: &str) -> Result<Vec<Session>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let invalid = || format!("Unrecognized session listing: {line}");
            let (name, rest) = line
                .trim_end()
                .rsplit_once(" [Created ")
                .ok_or_else(invalid)?;
            let (age, markers) = rest.split_once(" ago]").ok_or_else(invalid)?;
            let markers = markers.trim();
            let exited = markers.contains("(EXITED - attach to resurrect)");
            let current = markers.contains("(current)");
            let unknown = markers
                .replace("(EXITED - attach to resurrect)", "")
                .replace("(current)", "");
            if name.is_empty() || age.is_empty() || !unknown.trim().is_empty() {
                return Err(invalid());
            }
            Ok(Session {
                name: name.to_owned(),
                age: age.to_owned(),
                current,
                exited,
            })
        })
        .collect()
}

fn running_managed(sessions: Vec<Session>) -> Vec<Session> {
    let mut sessions: Vec<_> = sessions
        .into_iter()
        .filter(|session| session.name.starts_with(PREFIX) && !session.exited)
        .collect();
    sessions.sort_by(|a, b| a.name.cmp(&b.name));
    sessions
}

fn startup_sessions(runner: &impl Runner) -> (Result<Vec<Session>>, Vec<String>) {
    let sessions = match discover(runner) {
        Ok(sessions) => sessions,
        Err(error) => return (Err(error), vec![]),
    };
    let mut errors = Vec::new();
    for session in sessions
        .iter()
        .filter(|session| session.name.starts_with(PREFIX) && session.exited)
    {
        if let Err(error) = runner.run(&strings(&["delete-session", &session.name]), None, true) {
            errors.push(format!("Could not delete {}: {error}", session.name));
        }
    }
    (discover(runner).map(running_managed), errors)
}

fn details(runner: &impl Runner, name: &str) -> Result<SessionDetails> {
    let tabs = runner.run(
        &strings(&["-s", name, "action", "list-tabs", "--json"]),
        None,
        true,
    )?;
    let panes = runner.run(
        &strings(&["-s", name, "action", "list-panes", "--json", "--all"]),
        None,
        true,
    )?;
    Ok(SessionDetails {
        tabs: serde_json::from_str(&tabs)
            .map_err(|error| format!("Invalid tab details: {error}"))?,
        panes: serde_json::from_str::<Vec<PaneInfo>>(&panes)
            .map_err(|error| format!("Invalid pane details: {error}"))?
            .into_iter()
            .filter(|pane| !pane.is_plugin)
            .collect(),
    })
}

fn sanitize_project_name(name: &str) -> String {
    let name: String = name
        .to_ascii_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if name.is_empty() {
        "project".into()
    } else {
        name
    }
}

fn timestamp(now: OffsetDateTime) -> String {
    let now = now.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}{:09}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.nanosecond()
    )
}

fn unique_name(
    project: &str,
    sessions: &[Session],
    mut clock: impl FnMut() -> OffsetDateTime,
) -> String {
    loop {
        let name = format!(
            "{PREFIX}{}-{}",
            sanitize_project_name(project),
            timestamp(clock())
        );
        if !sessions.iter().any(|session| session.name == name) {
            return name;
        }
    }
}

fn expand_launch_command(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| shellexpand::tilde(part).into_owned())
        .collect()
}

fn append_launch(args: &mut Vec<String>, command: Option<&str>) {
    if let Some(command) = command {
        let parts = expand_launch_command(command);
        if !parts.is_empty() {
            args.push("--".into());
            args.extend(parts);
        }
    }
}

fn creation_commands(name: &str, project: Option<&Project>) -> Vec<Vec<String>> {
    let mut first = strings(&["attach", "--create-background", name]);
    append_launch(
        &mut first,
        project
            .and_then(|project| project.tabs.first())
            .and_then(|tab| tab.launch.as_deref()),
    );
    let mut commands = vec![first];
    if let Some(project) = project {
        for tab in project.tabs.iter().skip(1) {
            let mut args = strings(&[
                "-s",
                name,
                "action",
                "new-tab",
                "--no-focus",
                "--cwd",
                &project.expanded_path().to_string_lossy(),
            ]);
            append_launch(&mut args, tab.launch.as_deref());
            commands.push(args);
        }
    }
    commands
}

fn create(runner: &impl Runner, project: Option<&Project>) -> Result<String> {
    let sessions = discover(runner)?;
    let name = unique_name(
        project.map_or("empty", |project| project.name.as_str()),
        &sessions,
        OffsetDateTime::now_utc,
    );
    let cwd = project.map(Project::expanded_path);
    let setup = || -> Result<()> {
        let commands = creation_commands(&name, project);
        runner.run(&commands[0], cwd.as_deref(), true)?;
        if commands.len() > 1 {
            let _client = runner.setup_client(&name)?;
            for command in &commands[1..] {
                let output = runner.run(command, cwd.as_deref(), true)?;
                let tab_id = output
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("Zellij did not return a tab ID: {output:?}"))?;
                wait_for_tab(runner, &name, Some(tab_id))?;
            }
        }
        Ok(())
    };
    setup().map_err(|error| format!("Could not finish session '{name}': {error}. Any partially created session remains available for recovery."))?;
    Ok(name)
}

fn require_running(runner: &impl Runner, name: &str) -> Result<()> {
    if discover(runner)?
        .iter()
        .any(|session| session.name == name && !session.exited)
    {
        Ok(())
    } else {
        Err(format!(
            "Session '{name}' has exited or disappeared. Select a running session or create a new one."
        ))
    }
}

fn connect_args(name: &str, inside: bool) -> Vec<String> {
    if inside {
        strings(&["action", "switch-session", name])
    } else {
        strings(&["attach", name])
    }
}

pub fn launch(project: Option<&Project>) -> Result<()> {
    let runner = ProcessRunner::default();
    let name = create(&runner, project)?;
    connect(&runner, &name)
}

pub fn connect_existing(name: &str) -> Result<()> {
    let runner = ProcessRunner::default();
    require_running(&runner, name)?;
    connect(&runner, name)
}

fn connect(runner: &impl Runner, name: &str) -> Result<()> {
    if std::env::var("ZELLIJ_SESSION_NAME").as_deref() == Ok(name) {
        return Ok(());
    }
    let inside = std::env::var_os("ZELLIJ").is_some();
    let args = connect_args(name, inside);
    if inside {
        runner.run(&args, None, false).map(|_| ())
    } else {
        Err(format!(
            "Could not attach to session '{name}': {}",
            Command::new("zellij").args(args).exec()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Tab;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::PathBuf;

    type Call = (Vec<String>, Option<PathBuf>, bool);

    struct FakeRunner {
        replies: RefCell<VecDeque<Result<String>>>,
        calls: RefCell<Vec<Call>>,
    }

    impl FakeRunner {
        fn new(replies: Vec<Result<&str>>) -> Self {
            Self {
                replies: RefCell::new(
                    replies
                        .into_iter()
                        .map(|reply| reply.map(str::to_string))
                        .collect(),
                ),
                calls: RefCell::new(vec![]),
            }
        }
    }

    impl Runner for FakeRunner {
        type SetupClient = ();

        fn setup_client(&self, name: &str) -> Result<()> {
            self.run(&strings(&["attach", name]), None, true)
                .map(|_| ())
        }

        fn run(&self, args: &[String], cwd: Option<&Path>, clean: bool) -> Result<String> {
            self.calls
                .borrow_mut()
                .push((args.to_vec(), cwd.map(Path::to_path_buf), clean));
            self.replies
                .borrow_mut()
                .pop_front()
                .expect("unexpected command")
        }
    }

    fn project() -> Project {
        Project {
            name: "My Project!".into(),
            path: "/tmp/project space".into(),
            tabs: vec![
                Tab {
                    launch: Some("echo first".into()),
                },
                Tab {
                    launch: Some("echo second".into()),
                },
                Tab { launch: None },
            ],
        }
    }

    #[test]
    fn parses_exact_full_names_status_and_age() {
        let sessions = parse_sessions("prawjector-web [Created 1h 2m ago] (current)\nprawjector-space name [Created 2s ago]\nprawjector-dead [Created 3days ago] (EXITED - attach to resurrect)\n").unwrap();
        assert_eq!(sessions[0].age, "1h 2m");
        assert!(sessions[0].current);
        assert!(!sessions[0].exited);
        assert_eq!(sessions[1].name, "prawjector-space name");
        assert!(!sessions[1].current);
        assert!(sessions[2].exited);
    }

    #[test]
    fn filters_only_exact_prefix_and_sorts_running_including_detached() {
        let sessions = parse_sessions("prawjector-z [Created 1s ago]\nprawjector [Created 1s ago]\nother-prawjector-a [Created 1s ago]\nprawjector-a [Created 1s ago] (current)\nprawjector-b [Created 1s ago] (EXITED - attach to resurrect)").unwrap();
        let names: Vec<_> = running_managed(sessions)
            .into_iter()
            .map(|session| session.name)
            .collect();
        assert_eq!(names, ["prawjector-a", "prawjector-z"]);
    }

    #[test]
    fn malformed_listing_is_an_error() {
        for output in [
            "garbage",
            " [Created 1s ago]",
            "prawjector-x [Created ago]",
            "prawjector-x [Created 1s ago] (unknown)",
        ] {
            assert!(parse_sessions(output).is_err(), "{output}");
        }
        assert_eq!(parse_sessions("\n").unwrap(), vec![]);
    }

    #[test]
    fn cleanup_only_deletes_owned_exited_sessions_without_force_and_relists() {
        let listing = "prawjector-live [Created 1s ago]\nprawjector-old [Created 1s ago] (EXITED - attach to resurrect)\nprawjector [Created 1s ago] (EXITED - attach to resurrect)\nother [Created 1s ago] (EXITED - attach to resurrect)";
        let runner = FakeRunner::new(vec![Ok(listing), Ok(""), Ok(listing)]);
        let (sessions, errors) = startup_sessions(&runner);
        assert!(errors.is_empty());
        assert_eq!(sessions.unwrap().len(), 1);
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[1].0, ["delete-session", "prawjector-old"]);
        assert_eq!(calls[2].0, ["list-sessions", "--no-formatting"]);
    }

    #[test]
    fn cleanup_continues_after_individual_deletion_failure() {
        let listing = "prawjector-a [Created 1s ago] (EXITED - attach to resurrect)\nprawjector-b [Created 1s ago] (EXITED - attach to resurrect)";
        let runner = FakeRunner::new(vec![Ok(listing), Err("denied".into()), Ok(""), Ok("")]);
        let (sessions, errors) = startup_sessions(&runner);
        assert_eq!(sessions.unwrap(), vec![]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("prawjector-a"));
        assert_eq!(
            runner.calls.borrow()[2].0,
            ["delete-session", "prawjector-b"]
        );
    }

    #[test]
    fn failed_or_malformed_discovery_never_triggers_cleanup() {
        for reply in [Err("failed".into()), Ok("malformed")] {
            let runner = FakeRunner::new(vec![reply]);
            assert!(startup_sessions(&runner).0.is_err());
            assert_eq!(runner.calls.borrow().len(), 1);
        }
    }

    #[test]
    fn project_names_are_ascii_alphanumeric_with_fallback() {
        assert_eq!(sanitize_project_name("My Project-42_!?"), "myproject42");
        assert_eq!(sanitize_project_name("é東京_-"), "project");
        assert_eq!(sanitize_project_name("Ünicode"), "nicode");
    }

    #[test]
    fn timestamp_is_utc_with_nine_fractional_digits() {
        let date = OffsetDateTime::from_unix_timestamp(0)
            .unwrap()
            .replace_nanosecond(123)
            .unwrap();
        assert_eq!(timestamp(date), "19700101000000000000123");
        assert_eq!(
            timestamp(date.to_offset(time::UtcOffset::from_hms(5, 0, 0).unwrap())),
            timestamp(date)
        );
    }

    #[test]
    fn naming_retries_collisions_with_running_and_exited_sessions() {
        let first = OffsetDateTime::UNIX_EPOCH;
        let second = first + time::Duration::nanoseconds(1);
        let third = second + time::Duration::nanoseconds(1);
        let listing = format!(
            "prawjector-web-{} [Created 1s ago]\nprawjector-web-{} [Created 1s ago] (EXITED - attach to resurrect)",
            timestamp(first),
            timestamp(second)
        );
        let mut clock = [first, second, third].into_iter();
        let name = unique_name("web", &parse_sessions(&listing).unwrap(), || {
            clock.next().unwrap()
        });
        assert_eq!(name, format!("prawjector-web-{}", timestamp(third)));
    }

    #[test]
    fn creation_uses_native_initial_command_and_ordered_unfocused_tabs() {
        let commands = creation_commands("prawjector-test", Some(&project()));
        assert_eq!(
            commands[0],
            [
                "attach",
                "--create-background",
                "prawjector-test",
                "--",
                "echo",
                "first"
            ]
        );
        assert_eq!(
            commands[1],
            [
                "-s",
                "prawjector-test",
                "action",
                "new-tab",
                "--no-focus",
                "--cwd",
                "/tmp/project space",
                "--",
                "echo",
                "second"
            ]
        );
        assert_eq!(
            commands[2],
            [
                "-s",
                "prawjector-test",
                "action",
                "new-tab",
                "--no-focus",
                "--cwd",
                "/tmp/project space"
            ]
        );
    }

    #[test]
    fn empty_and_null_initial_commands_create_shells() {
        assert_eq!(
            creation_commands("test", None),
            vec![strings(&["attach", "--create-background", "test"])]
        );
        let mut project = project();
        project.tabs = vec![Tab { launch: None }];
        assert_eq!(
            creation_commands("test", Some(&project)),
            creation_commands("test", None)
        );
        project.tabs.clear();
        assert_eq!(
            creation_commands("test", Some(&project)),
            creation_commands("test", None)
        );
    }

    #[test]
    fn command_splitting_and_tilde_expansion_are_preserved() {
        let command = expand_launch_command("echo   ~/file  'two words'");
        assert_eq!(command[0], "echo");
        assert_eq!(
            command[1],
            dirs::home_dir().unwrap().join("file").to_string_lossy()
        );
        assert_eq!(&command[2..], ["'two", "words'"]);
    }

    #[test]
    fn create_checks_each_command_uses_project_cwd_and_cleans_environment() {
        let runner = FakeRunner::new(vec![
            Ok(""),
            Ok(""),
            Ok(""),
            Ok("1"),
            Ok(READY_TABS),
            Ok("2"),
            Ok(READY_TABS),
        ]);
        let project = project();
        let name = create(&runner, Some(&project)).unwrap();
        assert!(name.starts_with("prawjector-myproject-"));
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 7);
        for call in [&calls[1], &calls[3], &calls[5]] {
            assert_eq!(call.1, Some(project.expanded_path()));
            assert!(call.2);
        }
        assert_eq!(calls[1].0[0], "attach");
        assert_eq!(calls[2].0, ["attach", &name]);
        assert_eq!(calls[3].0[3], "new-tab");
        assert_eq!(calls[4].0[3], "list-tabs");
        assert_eq!(calls[5].0[3], "new-tab");
        assert_eq!(calls[6].0[3], "list-tabs");
    }

    #[test]
    fn failed_creation_or_tab_setup_stops_and_reports_exact_name() {
        for replies in [
            vec![Ok(""), Err("create failed".into())],
            vec![Ok(""), Ok(""), Err("client failed".into())],
            vec![Ok(""), Ok(""), Ok(""), Err("tab failed".into())],
        ] {
            let count = replies.len();
            let runner = FakeRunner::new(replies);
            let error = create(&runner, Some(&project())).unwrap_err();
            let calls = runner.calls.borrow();
            assert_eq!(calls.len(), count);
            assert!(error.contains(&calls[1].0[2]));
            assert!(error.contains("recovery"));
            assert!(
                !calls
                    .iter()
                    .any(|call| call.0.contains(&"delete-session".into()))
            );
        }
    }

    #[test]
    fn failed_discovery_prevents_creation() {
        let runner = FakeRunner::new(vec![Err("discovery failed".into())]);
        assert!(create(&runner, None).is_err());
        assert_eq!(runner.calls.borrow().len(), 1);
    }

    const READY_TABS: &str = r#"[
        {"tab_id":1,"active":true,"display_area_rows":24,"display_area_columns":80,"selectable_tiled_panes_count":1,"selectable_floating_panes_count":0},
        {"tab_id":2,"active":false,"display_area_rows":24,"display_area_columns":80,"selectable_tiled_panes_count":1,"selectable_floating_panes_count":0}
    ]"#;

    #[test]
    fn waits_for_a_client_and_a_populated_tab() {
        let runner = FakeRunner::new(vec![Ok("[]"), Ok(READY_TABS)]);
        wait_for_tab(&runner, "prawjector-a", None).unwrap();
        assert_eq!(runner.calls.borrow().len(), 2);
        let runner = FakeRunner::new(vec![
            Ok(
                r#"[{"tab_id":2,"active":false,"display_area_rows":0,"display_area_columns":0,"selectable_tiled_panes_count":0,"selectable_floating_panes_count":0}]"#,
            ),
            Ok(READY_TABS),
        ]);
        wait_for_tab(&runner, "prawjector-a", Some(2)).unwrap();
        assert_eq!(runner.calls.borrow().len(), 2);
    }

    #[test]
    fn a_successful_tab_command_without_an_id_stops_setup() {
        let runner = FakeRunner::new(vec![Ok(""), Ok(""), Ok(""), Ok("")]);
        let error = create(&runner, Some(&project())).unwrap_err();
        assert!(error.contains("did not return a tab ID"));
        assert_eq!(runner.calls.borrow().len(), 4);
    }

    #[test]
    fn empty_session_gets_managed_unique_name() {
        let runner = FakeRunner::new(vec![Ok(""), Ok("")]);
        let name = create(&runner, None).unwrap();
        assert!(name.starts_with("prawjector-empty-"));
        assert_eq!(runner.calls.borrow()[1].1, None);
    }

    #[test]
    fn recheck_accepts_only_exact_running_target() {
        for listing in [
            "prawjector-a [Created 1s ago] (EXITED - attach to resurrect)",
            "prawjector-ab [Created 1s ago]",
            "",
        ] {
            let runner = FakeRunner::new(vec![Ok(listing)]);
            assert!(require_running(&runner, "prawjector-a").is_err());
            assert_eq!(runner.calls.borrow().len(), 1);
        }
        let runner = FakeRunner::new(vec![Ok("prawjector-a [Created 1s ago] (current)")]);
        assert!(require_running(&runner, "prawjector-a").is_ok());
    }

    #[test]
    fn connection_commands_do_not_create_or_resurrect_intentionally() {
        assert_eq!(
            connect_args("prawjector-a", false),
            ["attach", "prawjector-a"]
        );
        assert_eq!(
            connect_args("prawjector-a", true),
            ["action", "switch-session", "prawjector-a"]
        );
    }

    #[test]
    fn details_use_targeted_json_and_actual_pane_fields() {
        let runner = FakeRunner::new(vec![
            Ok(r#"[{"name":"Editor","position":0}]"#),
            Ok(
                r#"[{"is_plugin":true},{"is_plugin":false,"tab_position":0,"pane_cwd":"/tmp/work","pane_command":"nvim"}]"#,
            ),
        ]);
        let details = details(&runner, "prawjector-a").unwrap();
        assert_eq!(details.tabs[0].name, "Editor");
        assert_eq!(details.panes.len(), 1);
        assert_eq!(details.panes[0].cwd.as_deref(), Some("/tmp/work"));
        assert_eq!(details.panes[0].terminal_command.as_deref(), Some("nvim"));
        for call in runner.calls.borrow().iter() {
            assert_eq!(&call.0[..2], ["-s", "prawjector-a"]);
            assert!(call.0.contains(&"--json".into()));
            assert!(call.2);
        }
    }

    #[test]
    fn detail_failures_are_explicit() {
        for replies in [
            vec![Err("tabs failed".into())],
            vec![Ok("[]"), Err("panes failed".into())],
            vec![Ok("bad json"), Ok("[]")],
        ] {
            assert!(details(&FakeRunner::new(replies), "prawjector-a").is_err());
        }
    }

    #[test]
    fn subprocess_deadline_and_cancellation_are_bounded() {
        let start = Instant::now();
        let mut command = Command::new("sleep");
        command.arg("10");
        assert!(run_bounded(command, Duration::from_millis(30), &AtomicBool::new(false)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        let mut command = Command::new("sleep");
        command.arg("10");
        assert!(run_bounded(command, Duration::from_secs(10), &AtomicBool::new(true)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn failed_subprocess_includes_stderr() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf failure >&2; exit 4"]);
        let error =
            run_bounded(command, Duration::from_secs(1), &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("failure"));
        assert!(error.contains('4'));
    }

    #[test]
    fn creation_environment_removes_only_session_targeting_variables() {
        let mut command = Command::new("zellij");
        command.env("PATH", "/bin");
        clear_session_environment(&mut command);
        let env: Vec<_> = command.get_envs().collect();
        assert!(env.contains(&(
            std::ffi::OsStr::new("PATH"),
            Some(std::ffi::OsStr::new("/bin"))
        )));
        for key in ["ZELLIJ", "ZELLIJ_SESSION_NAME", "ZELLIJ_PANE_ID"] {
            assert!(env.contains(&(std::ffi::OsStr::new(key), None)));
        }
    }
}
