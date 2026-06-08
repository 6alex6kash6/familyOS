use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use encoding_rs::WINDOWS_1251;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{AppService, CreateExpense, CreateGoal, ImportCsv};
use crate::csv_import::CsvImportSource;
use crate::models::AppState;
use crate::render;

const PAGES: [&str; 8] = [
    "Обзор",
    "Расходы",
    "Налоги",
    "Цели",
    "Активы",
    "Отчеты",
    "Интеграции",
    "Настройки",
];

pub fn run(service: &AppService) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, service);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    service: &AppService,
) -> Result<()> {
    let mut page = 0usize;
    let mut command_mode = false;
    let mut command_buffer = String::new();
    let mut message = "Ready".to_string();
    loop {
        let state = service.app_state()?;
        terminal
            .draw(|frame| draw(frame, &state, page, command_mode, &command_buffer, &message))?;
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            if command_mode {
                match key.code {
                    KeyCode::Esc => {
                        command_mode = false;
                        command_buffer.clear();
                        message = "Ready".to_string();
                    }
                    KeyCode::Enter => {
                        match execute_tui_command(service, &command_buffer) {
                            Ok(result) => message = result,
                            Err(error) => message = error.to_string(),
                        }
                        command_mode = false;
                        command_buffer.clear();
                    }
                    KeyCode::Backspace => {
                        command_buffer.pop();
                    }
                    KeyCode::Char(ch) => command_buffer.push(ch),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char(':') => {
                        command_mode = true;
                        command_buffer.clear();
                    }
                    KeyCode::Left | KeyCode::Char('h') => page = page.saturating_sub(1),
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                        page = (page + 1) % PAGES.len()
                    }
                    KeyCode::Char('1') => page = 0,
                    KeyCode::Char('2') => page = 1,
                    KeyCode::Char('3') => page = 2,
                    KeyCode::Char('4') => page = 3,
                    KeyCode::Char('5') => page = 4,
                    KeyCode::Char('6') => page = 5,
                    KeyCode::Char('7') => page = 6,
                    KeyCode::Char('8') => page = 7,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw(
    frame: &mut ratatui::Frame<'_>,
    state: &AppState,
    page: usize,
    command_mode: bool,
    command_buffer: &str,
    message: &str,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(frame.area());
    let nav = PAGES
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            let line = if idx == page {
                Line::styled(
                    format!("{} {}", idx + 1, label),
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else {
                Line::from(format!("{} {}", idx + 1, label))
            };
            ListItem::new(line)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(nav).block(Block::default().title("FamilyOS").borders(Borders::ALL)),
        layout[0],
    );
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(layout[1]);
    let body = match page {
        0 => render::render_overview(state),
        1 => render::render_expenses(state),
        2 => render::render_taxes(state),
        3 => render::render_goals(state),
        4 => render::render_assets(state),
        5 => render::render_reports(state),
        6 => render::render_integrations(state),
        _ => render::render_settings(state),
    };
    frame.render_widget(
        Paragraph::new(body)
            .block(Block::default().title(PAGES[page]).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        right[0],
    );
    let status = if command_mode {
        format!(":{command_buffer}")
    } else {
        message.to_string()
    };
    frame.render_widget(
        Paragraph::new(status).block(Block::default().title("Command").borders(Borders::ALL)),
        right[1],
    );
}

pub fn execute_tui_command(service: &AppService, command: &str) -> Result<String> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    let Some(first) = parts.first().copied() else {
        bail!("Empty command");
    };
    match first {
        "expense" => execute_expense_command(service, &parts),
        "goal" => execute_goal_command(service, &parts),
        "sync" => {
            let provider = parts.get(1).copied().unwrap_or("okx");
            service.sync_provider(provider)?;
            Ok(format!("Synced {provider}"))
        }
        "daily" => {
            service.generate_daily_brief()?;
            Ok("Daily brief generated".to_string())
        }
        "snapshot" => {
            service.create_monthly_snapshot()?;
            Ok("Monthly snapshot created".to_string())
        }
        "close" => {
            service.generate_monthly_report()?;
            Ok("Monthly close generated".to_string())
        }
        "import" => execute_import_command(service, &parts),
        "help" => {
            Ok("expense add | goal add | import csv | sync | daily | snapshot | close".to_string())
        }
        _ => bail!("Unsupported command"),
    }
}

fn execute_expense_command(service: &AppService, parts: &[&str]) -> Result<String> {
    if parts.get(1) != Some(&"add") || parts.len() < 6 {
        bail!("Usage: expense add <date> <amount> <currency> <category> [note]");
    }
    let amount = parts[3].parse::<f64>()?;
    let note = parts.get(6..).unwrap_or(&[]).join(" ");
    service.create_expense(CreateExpense {
        date: parts[2].to_string(),
        amount,
        currency: parts[4].to_string(),
        category: parts[5].to_string(),
        merchant_note: if note.is_empty() {
            parts[5].to_string()
        } else {
            note
        },
        source: "manual".to_string(),
    })?;
    Ok("Expense added".to_string())
}

fn execute_goal_command(service: &AppService, parts: &[&str]) -> Result<String> {
    if parts.get(1) != Some(&"add") || parts.len() < 4 {
        bail!("Usage: goal add <name> <target> [currency] [deadline]");
    }
    let deadline = parts
        .get(5)
        .filter(|value| **value != "-")
        .map(|value| (*value).to_string());
    service.create_goal(CreateGoal {
        name: parts[2].to_string(),
        target_amount: parts[3].parse::<f64>()?,
        currency: parts.get(4).copied().unwrap_or("USDT").to_string(),
        deadline,
        priority: 2,
        current_amount: 0.0,
        monthly_required_amount: None,
        bucket: "planned_purchase".to_string(),
    })?;
    Ok("Goal added".to_string())
}

fn execute_import_command(service: &AppService, parts: &[&str]) -> Result<String> {
    if parts.get(1) != Some(&"csv") || parts.len() < 4 {
        bail!("Usage: import csv <source> <path>");
    }
    let source = CsvImportSource::parse(parts[2])?;
    let path = PathBuf::from(parts[3]);
    let bytes = std::fs::read(path)?;
    let csv = if source == CsvImportSource::Priorbank {
        WINDOWS_1251.decode(&bytes).0.into_owned()
    } else {
        String::from_utf8(bytes)?
    };
    service.import_csv(ImportCsv { source, csv })?;
    Ok("CSV imported".to_string())
}
