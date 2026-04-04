use crate::app::{App, FocusedPane};
use crate::config::Config;
use crate::git_info::ProjectInfo;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

const GOLD: (u8, u8, u8) = (255, 215, 0);
const SILVER: (u8, u8, u8) = (192, 192, 192);
const GREEN: (u8, u8, u8) = (0, 200, 80);
const ORANGE: (u8, u8, u8) = (255, 140, 0);
const PALETTE: [(u8, u8, u8); 4] = [GOLD, SILVER, GREEN, ORANGE];
const LARGE_HEADER_HEIGHT_BREAKPOINT: u16 = 34;
const LARGE_HEADER_HEIGHT: u16 = 10;
const LARGE_HEADER_GAP: u16 = 2;
const COMPACT_HEADER_HEIGHT: u16 = 1;
const COMPACT_HEADER_GAP: u16 = 1;
const LARGE_HEADER_TEXT: &str = "PRAWJECTOR";
const LARGE_HEADER_LETTER_GAP: &str = "   ";
const SMALL_WINDOW_WIDTH_PX: u16 = 1500;
const SMALL_WINDOW_HEIGHT_PX: u16 = 900;
const TINY_WINDOW_WIDTH_PX: u16 = 1000;
const TINY_WINDOW_HEIGHT_PX: u16 = 750;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaddingMode {
    Normal,
    Small,
    Tiny,
}

pub const HELP_TEXT: &str = "Left/Right arrows to toggle projects/sessions.  Enter to submit";

pub fn draw(frame: &mut Frame, app: &App, config: &Config, padding_mode: PaddingMode) {
    let terminal = frame.area();
    let content_box = compute_content_box(terminal).intersection(terminal);
    let header_area = compute_header_area(terminal, content_box).intersection(terminal);
    draw_header(frame, app.tick, header_area, use_large_header(terminal));
    if content_box.width < 6 || content_box.height < 4 {
        return;
    }
    let usable_width = content_box.width.saturating_sub(4);
    let project_width = usable_width * 30 / 100;
    let session_width = usable_width * 30 / 100;
    let dividers = [
        content_box.x + 1 + project_width,
        content_box.x + 2 + project_width + session_width,
    ];
    let footer_width = content_box.width.saturating_sub(4);
    let footer_lines = wrap_heading(HELP_TEXT, footer_width);
    let footer = Paragraph::new(footer_lines.join("\n"));
    let footer_height = (footer_lines.len() as u16).min(content_box.height.saturating_sub(3));
    let footer_y = content_box.bottom() - 1 - footer_height;
    draw_box_border(
        frame.buffer_mut(),
        content_box,
        &dividers,
        footer_y,
        app.tick,
    );
    let (h_pad, v_pad) = inner_padding(content_box, padding_mode);
    let widths = [
        project_width,
        session_width,
        usable_width - project_width - session_width,
    ];
    let starts = [content_box.x + 1, dividers[0] + 1, dividers[1] + 1];
    let areas: [Rect; 3] = std::array::from_fn(|index| {
        Rect::new(
            starts[index] + h_pad,
            content_box.y + 1 + v_pad,
            widths[index].saturating_sub(2 * h_pad),
            footer_y.saturating_sub(content_box.y + 2 + 2 * v_pad),
        )
    });
    let titles = [
        "New session from project",
        "Existing sessions",
        "Information",
    ];
    let title_height = titles
        .iter()
        .zip(areas)
        .map(|(title, area)| wrap_heading(title, area.width).len() as u16)
        .max()
        .unwrap_or(1);
    let bodies: [Rect; 3] = std::array::from_fn(|index| {
        let active = matches!(
            (index, app.focused_pane),
            (0, FocusedPane::Projects) | (1, FocusedPane::Sessions)
        );
        let style = if active {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(wrap_heading(titles[index], areas[index].width).join("\n"))
                .style(style)
                .wrap(Wrap { trim: false }),
            Rect::new(
                areas[index].x,
                areas[index].y,
                areas[index].width,
                title_height.min(areas[index].height),
            ),
        );
        Rect::new(
            areas[index].x,
            areas[index].y + title_height.min(areas[index].height),
            areas[index].width,
            areas[index].height.saturating_sub(title_height),
        )
    });
    let projects = std::iter::once("Empty Session".to_string())
        .chain(config.projects.iter().map(|project| project.name.clone()))
        .collect();
    draw_list(
        frame,
        app,
        projects,
        app.selected_index,
        FocusedPane::Projects,
        bodies[0],
    );
    if app.sessions.is_empty() {
        let message = if app.discovery_error.is_some() {
            "Session discovery unavailable."
        } else if app.sessions_loaded {
            "No running managed sessions."
        } else {
            "Loading sessions..."
        };
        frame.render_widget(
            Paragraph::new(message).wrap(Wrap { trim: false }),
            bodies[1],
        );
    } else {
        let sessions = app
            .sessions
            .iter()
            .map(|session| {
                format!(
                    "{}{}",
                    session.name,
                    if session.current { " (current)" } else { "" }
                )
            })
            .collect();
        draw_list(
            frame,
            app,
            sessions,
            app.session_index,
            FocusedPane::Sessions,
            bodies[1],
        );
    }
    draw_detail(frame, app, config, bodies[2]);
    frame.render_widget(
        footer.style(Style::default().fg(Color::Gray)),
        Rect::new(content_box.x + 2, footer_y, footer_width, footer_height),
    );
}

fn wrap_heading(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut remaining = text;
    let mut lines = Vec::new();
    while remaining.len() > width {
        let split = remaining[..width]
            .rfind(' ')
            .filter(|position| *position > 0)
            .unwrap_or(width);
        lines.push(remaining[..split].to_string());
        remaining = remaining[split..].trim_start();
    }
    lines.push(remaining.to_string());
    lines
}

fn use_large_header(terminal: Rect) -> bool {
    terminal.height >= LARGE_HEADER_HEIGHT_BREAKPOINT
        && available_header_width(terminal) >= large_header_width()
}

fn header_metrics(terminal: Rect) -> (u16, u16) {
    if use_large_header(terminal) {
        (LARGE_HEADER_HEIGHT, LARGE_HEADER_GAP)
    } else {
        (COMPACT_HEADER_HEIGHT, COMPACT_HEADER_GAP)
    }
}

fn compute_content_box(terminal: Rect) -> Rect {
    const BREAKPOINT: u16 = 120;
    const MARGIN: u16 = 2;
    let (header_height, header_gap) = header_metrics(terminal);

    if terminal.width <= BREAKPOINT {
        Rect {
            x: terminal.x + MARGIN,
            y: terminal.y + MARGIN + header_height + header_gap,
            width: terminal.width.saturating_sub(MARGIN * 2),
            height: terminal
                .height
                .saturating_sub(MARGIN * 2 + header_height + header_gap),
        }
    } else {
        let w = terminal.width * 80 / 100;
        let preferred_h = terminal.height * 80 / 100;
        let h = preferred_h.min(
            terminal
                .height
                .saturating_sub(MARGIN * 2 + header_height + header_gap),
        );
        let cluster_h = header_height + header_gap + h;
        let top = terminal.y + (terminal.height.saturating_sub(cluster_h)) / 2;
        Rect {
            x: terminal.x + (terminal.width.saturating_sub(w)) / 2,
            y: top + header_height + header_gap,
            width: w,
            height: h,
        }
    }
}

fn compute_header_area(terminal: Rect, content_box: Rect) -> Rect {
    let (header_height, header_gap) = header_metrics(terminal);
    Rect {
        x: content_box.x,
        y: content_box.y.saturating_sub(header_height + header_gap),
        width: content_box.width,
        height: header_height,
    }
}

fn inner_padding(content_box: Rect, mode: PaddingMode) -> (u16, u16) {
    if content_box.width < 100 || content_box.height < 20 {
        return (0, 0);
    }
    match mode {
        PaddingMode::Normal => (2, 1),
        PaddingMode::Small | PaddingMode::Tiny => (1, 0),
    }
}

pub fn detect_padding_mode() -> PaddingMode {
    match crossterm::terminal::window_size() {
        Ok(ws) if ws.width > 0 && ws.height > 0 => {
            if ws.width <= TINY_WINDOW_WIDTH_PX || ws.height <= TINY_WINDOW_HEIGHT_PX {
                PaddingMode::Tiny
            } else if ws.width < SMALL_WINDOW_WIDTH_PX || ws.height < SMALL_WINDOW_HEIGHT_PX {
                PaddingMode::Small
            } else {
                PaddingMode::Normal
            }
        }
        _ => PaddingMode::Normal,
    }
}

fn draw_header(frame: &mut Frame, tick: u64, area: Rect, large: bool) {
    let lines = if large {
        build_large_header_lines(tick)
    } else {
        build_compact_header_lines(tick)
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn build_compact_header_lines(tick: u64) -> Vec<Line<'static>> {
    let row = "PRAWJECTOR";
    vec![animated_line(row, tick, 0, row.chars().count() as u32)]
}

fn build_large_header_lines(tick: u64) -> Vec<Line<'static>> {
    let rows = build_large_header_rows(LARGE_HEADER_TEXT);
    let total = rows
        .iter()
        .map(|row| row.chars().count() as u32)
        .sum::<u32>()
        .max(1);

    rows.into_iter()
        .scan(0u32, |offset, row| {
            let start = *offset;
            *offset += row.chars().count() as u32;
            Some(animated_line(&row, tick, start, total))
        })
        .collect()
}

fn available_header_width(terminal: Rect) -> u16 {
    const BREAKPOINT: u16 = 120;
    const MARGIN: u16 = 2;

    if terminal.width <= BREAKPOINT {
        terminal.width.saturating_sub(MARGIN * 2)
    } else {
        terminal.width * 80 / 100
    }
}

fn large_header_width() -> u16 {
    build_large_header_rows(LARGE_HEADER_TEXT)
        .into_iter()
        .map(|row| row.chars().count() as u16)
        .max()
        .unwrap_or_default()
}

fn build_large_header_rows(text: &str) -> [String; 10] {
    let mut rows = std::array::from_fn(|_| String::new());
    let chars = text.chars().collect::<Vec<_>>();
    let last_idx = chars.len().saturating_sub(1);

    chars.into_iter().enumerate().for_each(|(idx, ch)| {
        let glyph = banner_glyph(ch);
        (0..rows.len()).for_each(|row| {
            rows[row].push_str(glyph[row / 2]);
            if idx < last_idx {
                rows[row].push_str(LARGE_HEADER_LETTER_GAP);
            }
        });
    });

    rows
}

fn banner_glyph(ch: char) -> [&'static str; 5] {
    match ch {
        'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
        'C' => [" ████", "█    ", "█    ", "█    ", " ████"],
        'E' => ["█████", "█    ", "████ ", "█    ", "█████"],
        'J' => ["  ███", "   █ ", "   █ ", "█  █ ", " ██  "],
        'O' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
        'P' => ["█████", "█   █", "█████", "█    ", "█    "],
        'R' => ["████ ", "█   █", "████ ", "█  ██", "█   █"],
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
        'W' => ["█   █", "█   █", "█ █ █", "██ ██", "█   █"],
        _ => ["     "; 5],
    }
}

fn animated_line(text: &str, tick: u64, offset: u32, total: u32) -> Line<'static> {
    Line::from(
        text.chars()
            .enumerate()
            .map(|(idx, ch)| {
                if ch == ' ' {
                    Span::raw(ch.to_string())
                } else {
                    Span::styled(
                        ch.to_string(),
                        Style::default()
                            .fg(palette_color(tick, offset + idx as u32, total))
                            .add_modifier(Modifier::BOLD),
                    )
                }
            })
            .collect::<Vec<_>>(),
    )
}

fn draw_box_border(buf: &mut Buffer, area: Rect, dividers: &[u16], footer_y: u16, tick: u64) {
    let x1 = area.x;
    let x2 = area.x + area.width.saturating_sub(1);
    let y1 = area.y;
    let y2 = area.y + area.height.saturating_sub(1);

    let perimeter = 2 * (area.width as u32 + area.height.saturating_sub(2) as u32);
    let mut pos: u32 = 0;

    for x in x1..=x2 {
        let ch = if x == x1 {
            "\u{250C}"
        } else if x == x2 {
            "\u{2510}"
        } else if dividers.contains(&x) {
            "\u{252C}"
        } else {
            "\u{2500}"
        };
        set_border_cell(buf, x, y1, ch, palette_color(tick, pos, perimeter));
        pos += 1;
    }

    for y in (y1 + 1)..y2 {
        set_border_cell(buf, x2, y, "\u{2502}", palette_color(tick, pos, perimeter));
        pos += 1;
    }

    for x in (x1..=x2).rev() {
        let ch = if x == x1 {
            "\u{2514}"
        } else if x == x2 {
            "\u{2518}"
        } else {
            "\u{2500}"
        };
        set_border_cell(buf, x, y2, ch, palette_color(tick, pos, perimeter));
        pos += 1;
    }

    for y in ((y1 + 1)..y2).rev() {
        set_border_cell(buf, x1, y, "\u{2502}", palette_color(tick, pos, perimeter));
        pos += 1;
    }

    for divider_x in dividers {
        for y in (y1 + 1)..footer_y.saturating_sub(1) {
            let divider_pos = pos + (y - y1) as u32;
            set_border_cell(
                buf,
                *divider_x,
                y,
                "│",
                palette_color(tick, divider_pos, perimeter),
            );
        }
    }
    for x in (x1 + 1)..x2 {
        set_border_cell(
            buf,
            x,
            footer_y.saturating_sub(1),
            if dividers.contains(&x) { "┴" } else { "─" },
            palette_color(tick, pos + u32::from(x - x1), perimeter),
        );
    }
}

fn set_border_cell(buf: &mut Buffer, x: u16, y: u16, symbol: &str, color: Color) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(symbol);
        cell.set_fg(color);
    }
}

fn draw_list(
    frame: &mut Frame,
    app: &App,
    names: Vec<String>,
    selected: usize,
    pane: FocusedPane,
    area: Rect,
) {
    let active = app.focused_pane == pane;
    let items: Vec<_> = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| ListItem::new(format!("{index} {name}")))
        .collect();
    let style = if active {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(GOLD.0, GOLD.1, GOLD.2))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray).bg(Color::DarkGray)
    };
    let show_input = active && !app.input_buffer.is_empty();
    let list_area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(u16::from(show_input)),
    );
    let list = List::new(items)
        .highlight_style(style)
        .highlight_symbol(if active { "> " } else { "  " });
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, list_area, &mut state);
    if show_input && area.height > 0 {
        frame.render_widget(
            Paragraph::new(format!("> {}", app.input_buffer))
                .style(Style::default().fg(Color::Yellow)),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
    }
}

fn draw_detail(frame: &mut Frame, app: &App, config: &Config, area: Rect) {
    let mut lines = Vec::new();
    if let Some(error) = &app.launch_error {
        lines.push(Line::styled(error.clone(), Style::default().fg(Color::Red)));
    }
    if let Some(error) = &app.discovery_error {
        lines.push(Line::styled(
            format!("Discovery failed: {error}"),
            Style::default().fg(Color::Red),
        ));
    }
    for error in &app.cleanup_errors {
        lines.push(Line::styled(error.clone(), Style::default().fg(Color::Red)));
    }
    match app.focused_pane {
        FocusedPane::Projects if app.selected_index == 0 => {
            lines.push(Line::from("Create an additional empty Zellij session."));
            if config.projects.is_empty() {
                lines.push(Line::from("No configured projects."));
            }
        }
        FocusedPane::Projects => {
            let index = app.selected_index - 1;
            if let Some(project) = config.projects.get(index) {
                lines.extend(build_detail_lines(project, app.project_infos.get(&index)));
            } else {
                lines.push(Line::from("Project unavailable."));
            }
        }
        FocusedPane::Sessions => lines.extend(session_detail_lines(app)),
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        area,
    );
}

fn session_detail_lines(app: &App) -> Vec<Line<'static>> {
    let Some(session) = app.selected_session() else {
        return vec![Line::from("No session selected.")];
    };
    let mut lines = vec![
        detail_line("Session", session.name.clone()),
        detail_line("Age", session.age.clone()),
        detail_line(
            "Current session",
            if session.current { "yes" } else { "no" }.into(),
        ),
    ];
    match &app.session_details {
        None => lines.push(Line::from("Loading session details...")),
        Some(Err(error)) => lines.push(Line::from(format!("Details unavailable: {error}"))),
        Some(Ok(details)) => {
            if details.tabs.is_empty() {
                lines.push(Line::from("No tabs available."));
            }
            for tab in &details.tabs {
                lines.push(detail_line("Tab", tab.name.clone()));
                for pane in details
                    .panes
                    .iter()
                    .filter(|pane| pane.tab_position == Some(tab.position))
                {
                    lines.push(detail_line(
                        "Directory",
                        pane.cwd.clone().unwrap_or_else(|| "unavailable".into()),
                    ));
                    lines.push(detail_line(
                        "Command",
                        pane.terminal_command
                            .clone()
                            .unwrap_or_else(|| "unavailable".into()),
                    ));
                }
            }
            for pane in details.panes.iter().filter(|pane| {
                !details
                    .tabs
                    .iter()
                    .any(|tab| pane.tab_position == Some(tab.position))
            }) {
                lines.push(detail_line(
                    "Directory",
                    pane.cwd.clone().unwrap_or_else(|| "unavailable".into()),
                ));
                lines.push(detail_line(
                    "Command",
                    pane.terminal_command
                        .clone()
                        .unwrap_or_else(|| "unavailable".into()),
                ));
            }
            if details.panes.is_empty() {
                lines.push(Line::from("Terminal pane details unavailable."));
            }
        }
    }
    lines
}

fn build_detail_lines(
    project: &crate::config::Project,
    info: Option<&ProjectInfo>,
) -> Vec<Line<'static>> {
    let path_str = project.expanded_path().display().to_string();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::Cyan)),
            Span::raw(path_str),
        ]),
        Line::from(""),
    ];

    match info {
        Some(ProjectInfo::Git(git)) => {
            lines.push(Line::from(Span::styled(
                "Git Repository",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(detail_line("Files", git.file_count.to_string()));
            lines.push(detail_line("Commits", git.commit_count.to_string()));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Last Commit",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(detail_line(
                "Author",
                format!("{} <{}>", git.last_commit_author, git.last_commit_email),
            ));
            lines.push(detail_line("Date", git.last_commit_date.clone()));
            lines.push(detail_line("ID", git.last_commit_id.clone()));
            lines.push(detail_line("Message", git.last_commit_message.clone()));
        }
        Some(ProjectInfo::NonGit(non_git)) => {
            lines.push(Line::from(Span::styled(
                "Not a Git Repository",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            lines.push(detail_line("Files", non_git.file_count.to_string()));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "Project info unavailable",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines
}

fn detail_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{}: ", label), Style::default().fg(Color::Cyan)),
        Span::raw(value),
    ])
}

fn palette_color(tick: u64, position: u32, perimeter: u32) -> Color {
    let p = if perimeter == 0 {
        1.0
    } else {
        perimeter as f64
    };
    let t = (tick as f64 * 0.05 + position as f64 * 4.0 / p) % 4.0;
    let idx = t as usize;
    let frac = t - idx as f64;
    lerp_color(PALETTE[idx % 4], PALETTE[(idx + 1) % 4], frac)
}

fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> Color {
    Color::Rgb(
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Project, Tab};
    use crate::zellij::{PaneInfo, Session, SessionDetails, TabInfo, Update};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn rendered(app: &App, config: &Config, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| draw(frame, app, config, PaddingMode::Normal))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text_in(buffer: &Buffer, area: Rect) -> String {
        (area.y..area.bottom())
            .map(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn fixture() -> (App, Config) {
        let config = Config {
            projects: vec![Project {
                name: "Editor".into(),
                path: "/tmp".into(),
                tabs: vec![Tab { launch: None }],
            }],
        };
        let mut app = App::new(&config);
        app.apply_update(Update::Sessions(Ok(vec![Session {
            name: "prawjector-web".into(),
            age: "2h".into(),
            current: true,
            exited: false,
        }])));
        app.session_details = Some(Ok(SessionDetails {
            tabs: vec![TabInfo {
                name: "Work".into(),
                position: 0,
            }],
            panes: vec![PaneInfo {
                is_plugin: false,
                tab_position: Some(0),
                cwd: Some("/tmp/work".into()),
                terminal_command: Some("nvim".into()),
            }],
        }));
        (app, config)
    }

    #[test]
    fn headings_footer_and_three_columns_render_at_representative_widths() {
        let (app, config) = fixture();
        for width in [60, 80, 120, 160] {
            let buffer = rendered(&app, &config, width, 30);
            let content = compute_content_box(buffer.area);
            let usable = content.width - 4;
            let first = content.x + 1 + usable * 30 / 100;
            let second = first + 1 + usable * 30 / 100;
            assert_eq!(buffer[(first, content.y)].symbol(), "┬");
            assert_eq!(buffer[(second, content.y)].symbol(), "┬");
            let project = text_in(
                &buffer,
                Rect::new(content.x + 1, content.y + 1, first - content.x - 1, 4),
            );
            let sessions = text_in(
                &buffer,
                Rect::new(first + 1, content.y + 1, second - first - 1, 4),
            );
            assert!(
                project
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("New session from project")
            );
            assert!(
                sessions
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("Existing sessions")
            );
            let screen = text_in(&buffer, buffer.area);
            assert!(screen.contains("Information"));
            let footer = text_in(
                &buffer,
                Rect::new(content.x + 2, content.bottom() - 4, content.width - 4, 3),
            );
            assert!(
                footer
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("Left/Right arrows to toggle projects/sessions. Enter to submit")
            );
            assert!(!screen.contains("(NEW)"));
        }
    }

    #[test]
    fn focus_and_remembered_selection_have_distinct_styles() {
        let (mut app, config) = fixture();
        let buffer = rendered(&app, &config, 80, 24);
        assert_eq!(buffer[(3, 5)].fg, Color::Yellow);
        assert_eq!(buffer[(25, 5)].fg, Color::Gray);
        assert_eq!(buffer[(3, 7)].bg, Color::Rgb(GOLD.0, GOLD.1, GOLD.2));
        assert_eq!(buffer[(25, 7)].bg, Color::DarkGray);
        app.focused_pane = FocusedPane::Sessions;
        let buffer = rendered(&app, &config, 80, 24);
        assert_eq!(buffer[(3, 5)].fg, Color::Gray);
        assert_eq!(buffer[(25, 5)].fg, Color::Yellow);
        assert_eq!(buffer[(3, 7)].bg, Color::DarkGray);
        assert_eq!(buffer[(25, 7)].bg, Color::Rgb(GOLD.0, GOLD.1, GOLD.2));
    }

    #[test]
    fn information_follows_active_selection_and_includes_live_panes() {
        let (mut app, config) = fixture();
        app.selected_index = 1;
        let buffer = rendered(&app, &config, 120, 30);
        let screen = text_in(&buffer, buffer.area);
        assert!(screen.contains("Path: /tmp"));
        assert!(!screen.contains("Current session:"));
        app.focused_pane = FocusedPane::Sessions;
        let buffer = rendered(&app, &config, 120, 30);
        let screen = text_in(&buffer, buffer.area);
        for expected in [
            "Session: prawjector-web",
            "Age: 2h",
            "Current session: yes",
            "Tab: Work",
            "Directory: /tmp/work",
            "Command: nvim",
        ] {
            assert!(screen.contains(expected), "{expected}");
        }
        assert!(!screen.contains("Path: /tmp"));
    }

    #[test]
    fn both_lists_keep_last_selected_row_visible() {
        let config = Config {
            projects: (0..100)
                .map(|index| Project {
                    name: format!("Project{index}"),
                    path: "/tmp".into(),
                    tabs: vec![],
                })
                .collect(),
        };
        let mut app = App::new(&config);
        app.selected_index = 100;
        app.apply_update(Update::Sessions(Ok((0..100)
            .map(|index| Session {
                name: format!("prawjector-{index}"),
                age: "1s".into(),
                current: false,
                exited: false,
            })
            .collect())));
        app.session_index = 99;
        for focus in [FocusedPane::Projects, FocusedPane::Sessions] {
            app.focused_pane = focus;
            let buffer = rendered(&app, &config, 80, 24);
            let screen = text_in(&buffer, buffer.area);
            assert!(screen.contains("100 Project99"));
            assert!(screen.contains("99 prawjector-99"));
            assert!(!screen.contains("0 Empty Session"));
        }
    }

    #[test]
    fn empty_and_failed_lists_have_explicit_distinct_messages() {
        let config = Config { projects: vec![] };
        let mut app = App::new(&config);
        app.apply_update(Update::Sessions(Ok(vec![])));
        let buffer = rendered(&app, &config, 120, 30);
        let screen = text_in(&buffer, buffer.area);
        assert!(screen.contains("No running managed sessions."));
        assert!(screen.contains("No configured projects."));
        app.apply_update(Update::Sessions(Err("offline".into())));
        let buffer = rendered(&app, &config, 120, 30);
        let screen = text_in(&buffer, buffer.area);
        assert!(screen.contains("Session discovery"));
        assert!(screen.contains("unavailable."));
        assert!(screen.contains("Discovery failed: offline"));
    }

    #[test]
    fn unavailable_details_render_explicitly() {
        let (mut app, config) = fixture();
        app.focused_pane = FocusedPane::Sessions;
        app.session_details = Some(Err("query failed".into()));
        let buffer = rendered(&app, &config, 120, 30);
        assert!(text_in(&buffer, buffer.area).contains("Details unavailable: query failed"));
    }

    #[test]
    fn long_information_can_scroll_to_the_last_terminal_command() {
        let (mut app, config) = fixture();
        app.focused_pane = FocusedPane::Sessions;
        app.session_details = Some(Ok(SessionDetails {
            tabs: (0..10)
                .map(|position| TabInfo {
                    name: format!("Tab{position}"),
                    position,
                })
                .collect(),
            panes: (0..10)
                .map(|position| PaneInfo {
                    is_plugin: false,
                    tab_position: Some(position),
                    cwd: Some("/tmp".into()),
                    terminal_command: Some(format!("command{position}")),
                })
                .collect(),
        }));
        let buffer = rendered(&app, &config, 120, 30);
        assert!(!text_in(&buffer, buffer.area).contains("command9"));
        app.detail_scroll = 20;
        let buffer = rendered(&app, &config, 120, 30);
        assert!(text_in(&buffer, buffer.area).contains("command9"));
    }

    #[test]
    fn tiny_terminals_do_not_panic() {
        let (app, config) = fixture();
        for width in [0, 1, 5, 10, 20, 40] {
            for height in [0, 1, 5, 10] {
                rendered(&app, &config, width, height);
            }
        }
    }

    #[test]
    fn shared_footer_preserves_exact_text_when_it_fits() {
        assert_eq!(wrap_heading(HELP_TEXT, 80), vec![HELP_TEXT]);
        assert!(wrap_heading(HELP_TEXT, 40).len() > 1);
    }

    #[test]
    fn test_palette_color_returns_rgb() {
        let color = palette_color(0, 0, 100);
        match color {
            Color::Rgb(_, _, _) => {}
            _ => panic!("Expected RGB color"),
        }
    }

    #[test]
    fn test_palette_color_varies_with_position() {
        let c1 = palette_color(0, 0, 100);
        let c2 = palette_color(0, 50, 100);
        assert_ne!(format!("{:?}", c1), format!("{:?}", c2));
    }

    #[test]
    fn test_palette_color_varies_with_tick() {
        let c1 = palette_color(0, 0, 100);
        let c2 = palette_color(40, 0, 100);
        assert_ne!(format!("{:?}", c1), format!("{:?}", c2));
    }

    #[test]
    fn test_lerp_color_endpoints() {
        let a = (0, 0, 0);
        let b = (255, 255, 255);
        assert_eq!(lerp_color(a, b, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(lerp_color(a, b, 1.0), Color::Rgb(255, 255, 255));
    }

    #[test]
    fn test_compute_content_box_small() {
        let terminal = Rect::new(0, 0, 80, 24);
        let cb = compute_content_box(terminal);
        assert_eq!(cb.width, 76);
        assert_eq!(cb.height, 18);
        assert_eq!(cb.x, 2);
        assert_eq!(cb.y, 4);
    }

    #[test]
    fn test_compute_content_box_large() {
        let terminal = Rect::new(0, 0, 160, 50);
        let cb = compute_content_box(terminal);
        assert_eq!(cb.width, 128);
        assert_eq!(cb.height, 34);
        assert_eq!(cb.x, 16);
        assert_eq!(cb.y, 14);
    }

    #[test]
    fn test_compute_header_area_large() {
        let terminal = Rect::new(0, 0, 160, 50);
        let cb = compute_content_box(terminal);
        let header = compute_header_area(terminal, cb);
        assert_eq!(header.x, 16);
        assert_eq!(header.y, 2);
        assert_eq!(header.width, 128);
        assert_eq!(header.height, 10);
    }

    #[test]
    fn compact_padding_preserves_three_columns() {
        for mode in [PaddingMode::Normal, PaddingMode::Small, PaddingMode::Tiny] {
            assert_eq!(inner_padding(Rect::new(0, 0, 76, 18), mode), (0, 0));
        }
    }

    #[test]
    fn test_large_header_switches_on_for_roomy_terminal() {
        assert!(use_large_header(Rect::new(0, 0, 160, 50)));
        assert!(!use_large_header(Rect::new(0, 0, 80, 50)));
        assert!(!use_large_header(Rect::new(0, 0, 120, 30)));
        assert!(!use_large_header(Rect::new(0, 0, 80, 24)));
    }
}
