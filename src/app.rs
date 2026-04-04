use crate::config::Config;
use crate::git_info::{ProjectInfo, get_project_info};
use crate::zellij::{Session, SessionDetails, Update};
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusedPane {
    Projects,
    Sessions,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchAction {
    Quit,
    CreateProject(usize),
    CreateEmpty,
    Connect(String),
}

pub struct App {
    pub should_quit: bool,
    pub focused_pane: FocusedPane,
    pub selected_index: usize,
    pub session_index: usize,
    pub sessions: Vec<Session>,
    pub session_details: Option<std::result::Result<SessionDetails, String>>,
    pub discovery_error: Option<String>,
    pub cleanup_errors: Vec<String>,
    pub launch_error: Option<String>,
    pub sessions_loaded: bool,
    pub input_buffer: String,
    pub detail_scroll: u16,
    pub project_infos: HashMap<usize, ProjectInfo>,
    pub tick: u64,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let project_infos = config
            .projects
            .iter()
            .enumerate()
            .filter_map(|(index, project)| {
                get_project_info(&project.expanded_path())
                    .ok()
                    .map(|info| (index, info))
            })
            .collect();
        Self {
            should_quit: false,
            focused_pane: FocusedPane::Projects,
            selected_index: 0,
            session_index: 0,
            sessions: vec![],
            session_details: None,
            discovery_error: None,
            cleanup_errors: vec![],
            launch_error: None,
            sessions_loaded: false,
            input_buffer: String::new(),
            detail_scroll: 0,
            project_infos,
            tick: 0,
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.session_index)
    }

    pub fn selected_session_name(&self) -> Option<String> {
        self.selected_session().map(|session| session.name.clone())
    }

    pub fn apply_update(&mut self, update: Update) {
        match update {
            Update::Sessions(Ok(sessions)) => {
                let selected = self.selected_session_name();
                let changed = self
                    .sessions
                    .iter()
                    .map(|session| &session.name)
                    .ne(sessions.iter().map(|session| &session.name));
                self.session_index = selected
                    .as_ref()
                    .and_then(|name| sessions.iter().position(|session| &session.name == name))
                    .unwrap_or(self.session_index.min(sessions.len().saturating_sub(1)));
                self.sessions = sessions;
                if self.selected_session_name() != selected {
                    self.session_details = None;
                    self.detail_scroll = 0;
                }
                if changed && self.focused_pane == FocusedPane::Sessions {
                    self.input_buffer.clear();
                }
                self.discovery_error = None;
                self.sessions_loaded = true;
            }
            Update::Sessions(Err(error)) => {
                self.discovery_error = Some(error);
                self.sessions_loaded = true;
            }
            Update::Details(name, details) => {
                if self.selected_session_name().as_deref() == Some(&name) {
                    self.session_details = Some(details);
                }
            }
            Update::CleanupErrors(errors) => self.cleanup_errors.extend(errors),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent, total_projects: usize) -> Option<LaunchAction> {
        let old_session = self.selected_session_name();
        let old_project = self.selected_index;
        let old_focus = self.focused_pane;
        let total = match self.focused_pane {
            FocusedPane::Projects => total_projects,
            FocusedPane::Sessions => self.sessions.len(),
        };
        let selected = match self.focused_pane {
            FocusedPane::Projects => &mut self.selected_index,
            FocusedPane::Sessions => &mut self.session_index,
        };
        let action = match key.code {
            KeyCode::PageUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(5);
                None
            }
            KeyCode::PageDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(5);
                None
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                Some(LaunchAction::Quit)
            }
            KeyCode::Left | KeyCode::Right => {
                self.focused_pane = match self.focused_pane {
                    FocusedPane::Projects => FocusedPane::Sessions,
                    FocusedPane::Sessions => FocusedPane::Projects,
                };
                self.input_buffer.clear();
                None
            }
            KeyCode::Up | KeyCode::Down => {
                *selected = if key.code == KeyCode::Up {
                    selected.saturating_sub(1)
                } else {
                    (*selected + 1).min(total.saturating_sub(1))
                };
                self.input_buffer.clear();
                None
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.input_buffer.push(c);
                if let Ok(index) = self.input_buffer.parse::<usize>()
                    && index < total
                {
                    *selected = index;
                }
                None
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                if let Ok(index) = self.input_buffer.parse::<usize>()
                    && index < total
                {
                    *selected = index;
                }
                None
            }
            KeyCode::Enter => {
                let index = if self.input_buffer.is_empty() {
                    Some(*selected)
                } else {
                    self.input_buffer.parse::<usize>().ok()
                };
                self.input_buffer.clear();
                index
                    .filter(|index| *index < total)
                    .map(|index| match self.focused_pane {
                        FocusedPane::Projects if index == 0 => LaunchAction::CreateEmpty,
                        FocusedPane::Projects => LaunchAction::CreateProject(index - 1),
                        FocusedPane::Sessions => {
                            LaunchAction::Connect(self.sessions[index].name.clone())
                        }
                    })
            }
            _ => None,
        };
        if self.selected_session_name() != old_session {
            self.session_details = None;
        }
        if self.selected_session_name() != old_session
            || self.selected_index != old_project
            || self.focused_pane != old_focus
        {
            self.detail_scroll = 0;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn app() -> App {
        App::new(&Config { projects: vec![] })
    }

    fn key(app: &mut App, code: KeyCode) -> Option<LaunchAction> {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 15)
    }

    fn sessions(names: &[&str]) -> Update {
        Update::Sessions(Ok(names
            .iter()
            .map(|name| Session {
                name: (*name).into(),
                age: "1m".into(),
                current: false,
                exited: false,
            })
            .collect()))
    }

    #[test]
    fn starts_in_projects_with_empty_session() {
        let mut app = app();
        assert_eq!(app.focused_pane, FocusedPane::Projects);
        assert_eq!(
            key(&mut app, KeyCode::Enter),
            Some(LaunchAction::CreateEmpty)
        );
    }

    #[test]
    fn information_scroll_resets_when_the_active_selection_changes() {
        let mut app = app();
        key(&mut app, KeyCode::PageDown);
        assert_eq!(app.detail_scroll, 5);
        key(&mut app, KeyCode::PageUp);
        assert_eq!(app.detail_scroll, 0);
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::Down);
        assert_eq!(app.detail_scroll, 0);
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::Right);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn both_arrows_toggle_and_remember_selections() {
        let mut app = app();
        app.apply_update(sessions(&["a", "b", "c"]));
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Right);
        assert_eq!(app.selected_index, 1);
        assert_eq!(
            key(&mut app, KeyCode::Enter),
            Some(LaunchAction::CreateProject(0))
        );
        key(&mut app, KeyCode::Left);
        assert_eq!(app.session_index, 2);
        assert_eq!(
            key(&mut app, KeyCode::Enter),
            Some(LaunchAction::Connect("c".into()))
        );
        key(&mut app, KeyCode::Left);
        assert_eq!(app.focused_pane, FocusedPane::Projects);
    }

    #[test]
    fn numeric_input_targets_focused_list_and_clears_on_toggle() {
        let mut app = app();
        app.apply_update(sessions(&["a", "b", "c"]));
        key(&mut app, KeyCode::Char('1'));
        key(&mut app, KeyCode::Char('2'));
        assert_eq!(app.selected_index, 12);
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.selected_index, 1);
        key(&mut app, KeyCode::Right);
        assert!(app.input_buffer.is_empty());
        key(&mut app, KeyCode::Char('2'));
        assert_eq!(
            key(&mut app, KeyCode::Enter),
            Some(LaunchAction::Connect("c".into()))
        );
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn invalid_numeric_input_does_not_activate_remembered_row() {
        let mut app = app();
        for _ in 0..30 {
            key(&mut app, KeyCode::Char('9'));
        }
        assert_eq!(key(&mut app, KeyCode::Enter), None);
        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn space_does_nothing_in_both_lists() {
        let mut app = app();
        app.apply_update(sessions(&["a"]));
        for pane in [FocusedPane::Projects, FocusedPane::Sessions] {
            app.focused_pane = pane;
            app.input_buffer = "0".into();
            assert_eq!(key(&mut app, KeyCode::Char(' ')), None);
            assert_eq!(app.input_buffer, "0");
            assert_eq!(app.focused_pane, pane);
        }
    }

    #[test]
    fn empty_sessions_ignore_activation_and_navigation() {
        let mut app = app();
        key(&mut app, KeyCode::Right);
        for code in [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('0'),
            KeyCode::Enter,
        ] {
            assert_eq!(key(&mut app, code), None);
        }
        assert_eq!(app.session_index, 0);
    }

    #[test]
    fn changing_sessions_preserves_identity_and_discards_numeric_index() {
        let mut app = app();
        app.apply_update(sessions(&["b", "c"]));
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Char('1'));
        app.apply_update(sessions(&["a", "b", "c"]));
        assert_eq!(app.session_index, 2);
        assert!(app.input_buffer.is_empty());
        assert_eq!(
            key(&mut app, KeyCode::Enter),
            Some(LaunchAction::Connect("c".into()))
        );
        app.apply_update(sessions(&["a"]));
        assert_eq!(app.session_index, 0);
        app.apply_update(sessions(&[]));
        assert_eq!(app.selected_session(), None);
    }

    #[test]
    fn details_are_cleared_on_selection_change_and_stale_results_ignored() {
        let mut app = app();
        app.apply_update(sessions(&["a", "b"]));
        app.apply_update(Update::Details("a".into(), Err("a error".into())));
        assert!(app.session_details.is_some());
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Down);
        assert!(app.session_details.is_none());
        app.apply_update(Update::Details("a".into(), Err("stale".into())));
        assert!(app.session_details.is_none());
        app.apply_update(Update::Details("b".into(), Err("b error".into())));
        assert_eq!(app.session_details, Some(Err("b error".into())));
        app.apply_update(sessions(&[]));
        assert!(app.session_details.is_none());
    }

    #[test]
    fn discovery_errors_keep_last_list_and_are_distinct_from_empty() {
        let mut app = app();
        app.apply_update(sessions(&["a"]));
        app.apply_update(Update::Sessions(Err("failed".into())));
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.discovery_error.as_deref(), Some("failed"));
        app.apply_update(sessions(&[]));
        assert!(app.discovery_error.is_none());
        assert!(app.sessions_loaded);
    }

    #[test]
    fn navigation_stops_at_list_boundaries_and_quit_works() {
        let mut app = app();
        key(&mut app, KeyCode::Up);
        assert_eq!(app.selected_index, 0);
        for _ in 0..20 {
            key(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected_index, 14);
        assert_eq!(key(&mut app, KeyCode::Esc), Some(LaunchAction::Quit));
        assert!(app.should_quit);
    }
}
