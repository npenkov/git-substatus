//! Rendering. All view logic reads from `App`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, InputMode, Row, Target};
use crate::diff::DiffKind;
use crate::model::{RepoState, RepoStatus};

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Min(1),    // body
            Constraint::Length(1), // help / input
        ])
        .split(f.area());

    render_title(f, app, chunks[0]);

    if app.show_detail {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[1]);
        render_list(f, app, body[0]);
        render_detail(f, app, body[1]);
    } else {
        render_list(f, app, chunks[1]);
    }
    render_help(f, app, chunks[2]);

    if app.input_mode == InputMode::Actions {
        render_actions_popup(f, app);
    }
}

fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let total = app.repos.len();
    let dirty = app
        .repos
        .iter()
        .filter(|r| !r.clean || r.error.is_some())
        .count();
    let scanning = app.scan_count();
    let mut spans = vec![
        Span::styled("git-substatus", Style::default().fg(Color::Cyan).bold()),
        Span::raw("  "),
        Span::styled(app.root.display().to_string(), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::raw(format!("[{total} repos, {dirty} dirty]")),
    ];
    if scanning > 0 {
        spans.push(Span::styled(
            format!("  ⟳ scanning {scanning}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    if app.dirty_only {
        spans.push(Span::styled("  (dirty-only)", Style::default().fg(Color::Magenta)));
    }
    f.render_widget(Line::from(spans), area);
}

fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::List;
    let border = if focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(" repos ");

    let rows = app.rows();
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            Row::Repo(ri) => repo_line(&app.repos[*ri], app.expanded.contains(&app.repos[*ri].path)),
            Row::File { repo, file } => file_line(&app.repos[*repo], *file),
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn repo_line(repo: &RepoStatus, expanded: bool) -> ListItem<'static> {
    let marker = if repo.entries.is_empty() {
        " "
    } else if expanded {
        "▾"
    } else {
        "▸"
    };
    let branch = repo.branch.clone().unwrap_or_else(|| "?".into());

    let mut spans = vec![
        Span::styled(format!("{marker} "), Style::default().fg(Color::DarkGray)),
        Span::styled(repo.name.clone(), Style::default().fg(Color::Cyan).bold()),
        Span::styled(format!(" ({branch})"), Style::default().fg(Color::DarkGray)),
    ];
    if repo.upstream && repo.ahead > 0 {
        spans.push(Span::styled(format!(" ↑{}", repo.ahead), Style::default().fg(Color::Green)));
    }
    if repo.upstream && repo.behind > 0 {
        spans.push(Span::styled(format!(" ↓{}", repo.behind), Style::default().fg(Color::Yellow)));
    }
    spans.push(Span::raw("  "));
    if repo.state == RepoState::Scanning {
        spans.push(Span::styled("⟳", Style::default().fg(Color::DarkGray)));
    } else if let Some(err) = &repo.error {
        spans.push(Span::styled(format!("✗ {err}"), Style::default().fg(Color::Red)));
    } else if repo.clean {
        spans.push(Span::styled("✓", Style::default().fg(Color::Green)));
    } else {
        spans.push(Span::styled(
            format!("● {}", repo.entries.len()),
            Style::default().fg(Color::Yellow),
        ));
    }
    ListItem::new(Line::from(spans))
}

fn file_line(repo: &RepoStatus, file: usize) -> ListItem<'static> {
    let entry = &repo.entries[file];
    let [x, y] = entry.xy();
    let code: String = [x, y].iter().collect();
    let color = if x == '?' {
        Color::Red
    } else if x != ' ' {
        Color::Green // staged
    } else {
        Color::Yellow // unstaged
    };
    ListItem::new(Line::from(vec![
        Span::raw("    "),
        Span::styled(code, Style::default().fg(color)),
        Span::raw(" "),
        Span::raw(entry.rel_path.clone()),
    ]))
}

fn render_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let border = if focused { Color::Cyan } else { Color::DarkGray };

    // File rows show a diff (needs &mut app); repo/none rows show a summary.
    let is_file = matches!(app.current_row(), Some(Row::File { .. }));
    let (title, lines) = if is_file {
        let title = match app.current_target() {
            Target::File { repo, rel } => format!("{} · {}", repo.name, rel),
            _ => "diff".to_string(),
        };
        (title, diff_lines(app))
    } else {
        detail_content(app)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(format!(" {title} "));

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, area);
}

fn detail_content(app: &mut App) -> (String, Vec<Line<'static>>) {
    match app.current_target() {
        Target::None => ("detail".into(), vec![Line::raw("no selection")]),
        Target::Repo(repo) => {
            let title = repo.name.clone();
            let mut lines = vec![Line::from(vec![
                Span::styled("branch ", Style::default().fg(Color::DarkGray)),
                Span::raw(repo.branch.clone().unwrap_or_else(|| "?".into())),
            ])];
            if repo.upstream {
                lines.push(Line::raw(format!("ahead {}  behind {}", repo.ahead, repo.behind)));
            }
            if let Some(err) = &repo.error {
                lines.push(Line::styled(format!("error: {err}"), Style::default().fg(Color::Red)));
            }
            lines.push(Line::raw(""));
            if repo.entries.is_empty() {
                lines.push(Line::styled("clean", Style::default().fg(Color::Green)));
            } else {
                for (i, _) in repo.entries.iter().enumerate() {
                    lines.push(file_plain_line(repo, i));
                }
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    "↵ expand · select a file for its diff",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            (title, lines)
        }
        // File rows are handled in render_detail (they need a mutable borrow).
        Target::File { repo, rel } => (format!("{} · {}", repo.name, rel), Vec::new()),
    }
}

fn file_plain_line(repo: &RepoStatus, file: usize) -> Line<'static> {
    let entry = &repo.entries[file];
    let [x, y] = entry.xy();
    let code: String = [x, y].iter().collect();
    Line::from(vec![
        Span::styled(code, Style::default().fg(Color::Yellow)),
        Span::raw(" "),
        Span::raw(entry.rel_path.clone()),
    ])
}

fn render_help(f: &mut Frame, app: &App, area: Rect) {
    if app.input_mode == InputMode::Filter {
        let line = Line::from(vec![
            Span::styled("/", Style::default().fg(Color::Cyan)),
            Span::raw(app.filter.clone()),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
            Span::styled("  (Enter: keep · Esc: clear)", Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(line, area);
        return;
    }
    if let Some(msg) = &app.status_msg {
        f.render_widget(
            Line::styled(msg.clone(), Style::default().fg(Color::Green)),
            area,
        );
        return;
    }
    let help = "j/k move · l/↵ expand/diff · h collapse · Tab panel · d dirty · / filter · a actions · r rescan · q quit";
    f.render_widget(
        Line::styled(help, Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_actions_popup(f: &mut Frame, app: &App) {
    let target = app
        .selected_repo_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "no repo".into());

    let mut lines: Vec<Line> = Vec::new();
    for a in &app.actions {
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", a.key), Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("  "),
            Span::raw(a.name.clone()),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Esc to cancel", Style::default().fg(Color::DarkGray)));

    let height = (lines.len() as u16) + 2;
    let width = 50u16;
    let area = centered_rect(width, height, f.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" actions · {target} "));

    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Build the colored diff lines for the current file selection.
pub fn diff_lines(app: &mut App) -> Vec<Line<'static>> {
    app.current_diff()
        .into_iter()
        .map(|d| {
            let style = match d.kind {
                DiffKind::Header => Style::default().fg(Color::Cyan).bold(),
                DiffKind::Hunk => Style::default().fg(Color::Magenta),
                DiffKind::Add => Style::default().fg(Color::Green),
                DiffKind::Del => Style::default().fg(Color::Red),
                DiffKind::Context => Style::default().fg(Color::Gray),
            };
            Line::styled(d.text, style)
        })
        .collect()
}
