use crate::config::Config;
use crate::git_info::{ProjectInfo, get_project_info};
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashMap;

pub struct App {
    pub should_quit: bool,
    pub selected_index: usize,
    pub input_buffer: String,
    pub project_infos: HashMap<usize, ProjectInfo>,
    pub tick: u64,
    pub new_session: bool,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let mut project_infos = HashMap::new();
        for (i, project) in config.projects.iter().enumerate() {
            if let Ok(info) = get_project_info(&project.expanded_path()) {
                project_infos.insert(i, info);
            }
        }
        Self {
            should_quit: false,
            selected_index: 0,
            input_buffer: String::new(),
            project_infos,
            tick: 0,
            new_session: false,
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn handle_key(&mut self, key: KeyEvent, total_options: usize) -> Option<(usize, bool)> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                None
            }
            KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                self.input_buffer.clear();
                None
            }
            KeyCode::Down => {
                if self.selected_index < total_options.saturating_sub(1) {
                    self.selected_index += 1;
                }
                self.input_buffer.clear();
                None
            }
            KeyCode::Char(' ') => {
                self.new_session = !self.new_session;
                None
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.input_buffer.push(c);
                if let Ok(num) = self.input_buffer.parse::<usize>()
                    && num < total_options
                {
                    self.selected_index = num;
                }
                None
            }
            KeyCode::Enter => {
                let result = if self.input_buffer.is_empty() {
                    Some((self.selected_index, self.new_session))
                } else {
                    self.input_buffer
                        .parse::<usize>()
                        .ok()
                        .filter(|&n| n < total_options)
                        .map(|n| (n, self.new_session))
                };
                self.input_buffer.clear();
                result
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn test_handle_key_quit_q() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Char('q')), 1);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_key_quit_esc() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Esc), 1);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_key_navigate_down() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Down), 3);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_handle_key_navigate_up() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.selected_index = 2;
        app.handle_key(make_key(KeyCode::Up), 3);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_handle_key_navigate_up_at_zero() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Up), 3);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_handle_key_ignores_j_and_k() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.selected_index = 1;
        app.input_buffer = "2".to_string();

        let j_result = app.handle_key(make_key(KeyCode::Char('j')), 3);
        assert_eq!(j_result, None);
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.input_buffer, "2");

        let k_result = app.handle_key(make_key(KeyCode::Char('k')), 3);
        assert_eq!(k_result, None);
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.input_buffer, "2");
    }

    #[test]
    fn test_handle_key_enter_selects() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.selected_index = 1;
        let result = app.handle_key(make_key(KeyCode::Enter), 3);
        assert_eq!(result, Some((1, false)));
    }

    #[test]
    fn test_handle_key_number_input() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Char('2')), 5);
        assert_eq!(app.input_buffer, "2");
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn test_handle_key_number_then_enter() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.handle_key(make_key(KeyCode::Char('1')), 5);
        let result = app.handle_key(make_key(KeyCode::Enter), 5);
        assert_eq!(result, Some((1, false)));
    }

    #[test]
    fn test_spacebar_toggles_new_session() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        assert!(!app.new_session);
        app.handle_key(make_key(KeyCode::Char(' ')), 3);
        assert!(app.new_session);
        app.handle_key(make_key(KeyCode::Char(' ')), 3);
        assert!(!app.new_session);
    }

    #[test]
    fn test_spacebar_then_enter_returns_new_session() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        app.selected_index = 1;
        app.handle_key(make_key(KeyCode::Char(' ')), 3);
        let result = app.handle_key(make_key(KeyCode::Enter), 3);
        assert_eq!(result, Some((1, true)));
    }

    #[test]
    fn test_tick_increments() {
        let config = crate::config::Config { projects: vec![] };
        let mut app = App::new(&config);
        assert_eq!(app.tick, 0);
        app.tick();
        assert_eq!(app.tick, 1);
        app.tick();
        assert_eq!(app.tick, 2);
    }
}
