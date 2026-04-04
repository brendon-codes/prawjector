use crate::app::App;
use crate::config::Config;
use crate::git_info::ProjectInfo;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

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
const TINY_H_PAD: u16 = 2;
const TINY_V_PAD: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaddingMode {
    Normal,
    Small,
    Tiny,
}

pub fn draw(frame: &mut Frame, app: &App, config: &Config, padding_mode: PaddingMode) {
    let terminal = frame.area();
    let content_box = compute_content_box(terminal);
    let header_area = compute_header_area(terminal, content_box);

    draw_header(frame, app.tick, header_area, use_large_header(terminal));

    let usable_width = content_box.width.saturating_sub(3);
    let left_width = usable_width / 2;
    let divider_x = content_box.x + 1 + left_width;

    draw_box_border(frame.buffer_mut(), content_box, divider_x, app.tick);

    let (h_pad, v_pad) = inner_padding(content_box, padding_mode);

    let inner_height = content_box.height.saturating_sub(2 + v_pad * 2);
    let inner_y = content_box.y + 1 + v_pad;

    let left_inner = Rect {
        x: content_box.x + 1 + h_pad,
        y: inner_y,
        width: left_width.saturating_sub(h_pad * 2),
        height: inner_height,
    };

    let right_width = usable_width.saturating_sub(left_width);
    let right_inner = Rect {
        x: divider_x + 1 + h_pad,
        y: inner_y,
        width: right_width.saturating_sub(h_pad * 2),
        height: inner_height,
    };

    draw_project_list(frame, app, config, left_inner);
    draw_project_detail(frame, app, config, right_inner);
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
    match mode {
        PaddingMode::Normal => (
            (content_box.width / 8).clamp(4, 12),
            (content_box.height / 4).clamp(2, 6),
        ),
        PaddingMode::Small => {
            let h = (content_box.width / 8).clamp(4, 12);
            let v = (content_box.height / 4).clamp(2, 6);
            (h / 2, v / 2)
        }
        PaddingMode::Tiny => (TINY_H_PAD, TINY_V_PAD),
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

fn draw_box_border(buf: &mut Buffer, area: Rect, divider_x: u16, tick: u64) {
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
        } else if x == divider_x {
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
        } else if x == divider_x {
            "\u{2534}"
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

    for y in (y1 + 1)..y2 {
        let divider_pos = pos + (y - y1) as u32;
        set_border_cell(
            buf,
            divider_x,
            y,
            "\u{2502}",
            palette_color(tick, divider_pos, perimeter),
        );
    }
}

fn set_border_cell(buf: &mut Buffer, x: u16, y: u16, symbol: &str, color: Color) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(symbol);
        cell.set_fg(color);
    }
}

fn draw_project_list(frame: &mut Frame, app: &App, config: &Config, area: Rect) {
    let new_suffix = if app.new_session { " (NEW)" } else { "" };

    let items: Vec<ListItem> = std::iter::once({
        let name = if app.selected_index == 0 {
            format!("Empty Session{}", new_suffix)
        } else {
            "Empty Session".to_string()
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<4}", 0), Style::default().fg(Color::DarkGray)),
            Span::raw(name),
        ]))
    })
    .chain(config.projects.iter().enumerate().map(|(i, project)| {
        let name = if app.selected_index == i + 1 {
            format!("{}{}", project.name, new_suffix)
        } else {
            project.name.clone()
        };
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("  {:<4}", i + 1),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(name),
        ]))
    }))
    .collect();

    let list_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1),
    };

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(0, 0, 0))
                .bg(Color::Rgb(GOLD.0, GOLD.1, GOLD.2))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    let mut list_state = ListState::default().with_selected(Some(app.selected_index));
    frame.render_stateful_widget(list, list_area, &mut list_state);

    if !app.input_buffer.is_empty() {
        let input_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(2),
            width: area.width,
            height: 1,
        };
        let input_line = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Yellow)),
            Span::raw(&app.input_buffer),
        ]));
        frame.render_widget(input_line, input_area);
    }

    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let help_line = Paragraph::new(Line::from(Span::styled(
        "Spacebar to start new session. Enter to submit.",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(help_line, help_area);
}

fn draw_project_detail(frame: &mut Frame, app: &App, config: &Config, area: Rect) {
    let title_block = Block::default();

    let detail_text = if app.selected_index == 0 {
        vec![Line::from("Start an empty Zellij session.")]
    } else {
        let project_idx = app.selected_index - 1;
        match config.projects.get(project_idx) {
            Some(project) => build_detail_lines(project, app.project_infos.get(&project_idx)),
            None => vec![Line::from("Unknown project.")],
        }
    };

    let paragraph = Paragraph::new(detail_text)
        .block(title_block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
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
    fn test_inner_padding_normal() {
        let cb = Rect::new(0, 0, 80, 24);
        assert_eq!(inner_padding(cb, PaddingMode::Normal), (10, 6));
    }

    #[test]
    fn test_inner_padding_small_halves() {
        let cb = Rect::new(0, 0, 80, 24);
        let (h_normal, v_normal) = inner_padding(cb, PaddingMode::Normal);
        let (h_small, v_small) = inner_padding(cb, PaddingMode::Small);
        assert_eq!(h_small, h_normal / 2);
        assert_eq!(v_small, v_normal / 2);
    }

    #[test]
    fn test_inner_padding_small_min_clamp() {
        let cb = Rect::new(0, 0, 20, 8);
        assert_eq!(inner_padding(cb, PaddingMode::Normal), (4, 2));
        assert_eq!(inner_padding(cb, PaddingMode::Small), (2, 1));
    }

    #[test]
    fn test_inner_padding_tiny_is_fixed_minimal() {
        let large = Rect::new(0, 0, 160, 50);
        let small = Rect::new(0, 0, 20, 8);
        assert_eq!(inner_padding(large, PaddingMode::Tiny), (2, 1));
        assert_eq!(inner_padding(small, PaddingMode::Tiny), (2, 1));
    }

    #[test]
    fn test_large_header_switches_on_for_roomy_terminal() {
        assert!(use_large_header(Rect::new(0, 0, 160, 50)));
        assert!(!use_large_header(Rect::new(0, 0, 80, 50)));
        assert!(!use_large_header(Rect::new(0, 0, 120, 30)));
        assert!(!use_large_header(Rect::new(0, 0, 80, 24)));
    }
}
