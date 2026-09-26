//! The terminal face: the same wizard model (`shun::wizard`) rendered
//! with ratatui — a left step rail exactly like the GUI shells, the
//! location step with its candidate row, the paged license agreement,
//! the install gauge with the log tail, the done page with the
//! shortcut toggles, and the standalone uninstall/repair page.
//!
//! Keymap (also shown in the footer bar):
//! `↑/↓` move · `Enter` next/confirm · `Esc` back/exit · `Tab` switch
//! focus (path ↔ candidates) · `Space` toggle · `PgUp/PgDn` scroll the
//! license · typing edits the path.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use shun::config::ShunConfig;
use shun::flow::FlowEvent;
use shun::payload::ArchivePayload;
use shun::wizard::{InstallRequest, Step, UninstallPhase, WizardCore, WizardState};

/// The face's labels. Faces localize from their own tables — the model
/// carries step keys, never display strings.
struct Texts {
    rail: [&'static str; 5],
    language: (&'static str, &'static str),
    location: (&'static str, &'static str, &'static str),
    license: (&'static str, &'static str),
    install: &'static str,
    done: (&'static str, &'static str, &'static str, &'static str),
    uninstall: (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    ),
    nav: (&'static str, &'static str),
    footer: &'static str,
}

const ZH: Texts = Texts {
    rail: ["安装语言", "安装位置", "用户协议", "安装", "完成"],
    language: ("选择安装语言", "后续步骤将以所选语言显示"),
    location: ("选择安装位置", "安装到：", "预设位置："),
    license: ("用户协议", "我已阅读并接受上述协议"),
    install: "正在安装",
    done: ("安装完成", "安装失败", "桌面快捷方式", "立即启动"),
    uninstall: ("卸载", "修复安装", "正在卸载", "卸载完成", "卸载失败"),
    nav: ("下一步", "上一步"),
    footer: "↑/↓ 移动 · Enter 下一步/确认 · Esc 上一步/退出 · Tab 切换焦点 · Space 勾选 · PgUp/PgDn 滚动协议",
};

const EN: Texts = Texts {
    rail: ["Language", "Location", "License", "Install", "Done"],
    language: ("Choose the language", "Later steps render in it"),
    location: ("Choose the install location", "Install to:", "Quick picks:"),
    license: ("License agreement", "I have read and accept the agreement"),
    install: "Installing",
    done: (
        "Install complete",
        "Install failed",
        "Desktop shortcut",
        "Launch now",
    ),
    uninstall: (
        "Uninstall",
        "Repair",
        "Uninstalling",
        "Uninstalled",
        "Uninstall failed",
    ),
    nav: ("Next", "Back"),
    footer: "↑/↓ move · Enter next/confirm · Esc back/exit · Tab focus · Space toggle · PgUp/PgDn scroll license",
};

/// Which element of the location step holds the cursor.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Focus {
    Path,
    Candidates,
}

/// Runs the terminal wizard. `uninstall_mode` renders the standalone
/// uninstall/repair page instead (the TTY's answer to the ARP entry).
pub fn run(
    config: ShunConfig,
    payload: ArchivePayload,
    uninstall_mode: bool,
) -> Result<(), String> {
    let locale_hint = std::env::var("LANG").unwrap_or_default();
    let mut core = {
        let mut core = WizardCore::new(config, license_docs_view());
        if locale_hint.starts_with("zh") {
            core.set_locale("zh-Hans");
        }
        core
    };
    if uninstall_mode {
        core.state.step = Step::Done; // the uninstall page replaces the rail flow
    }

    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    let _ = stdout.execute(EnterAlternateScreen);
    let backend = ratatui::backend::CrosstermBackend::new(&mut stdout);
    let mut terminal = ratatui::Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut focus = Focus::Path;
    let mut candidate_state = ListState::default();
    candidate_state.select(Some(0));
    let mut license_scroll: u16 = 0;
    let mut locale_choice = ListState::default();
    locale_choice.select(Some(0));
    let locales: Vec<String> = core.license_docs.keys().cloned().collect();
    // (install worker join, event channel) while Step::Install runs.
    let mut worker: Option<std::thread::JoinHandle<Result<(), String>>> = None;
    let mut events: Option<mpsc::Receiver<FlowEvent>> = None;

    let result = (|| -> Result<(), String> {
        loop {
            // Drain flow events into the shared model between frames.
            if let Some(rx) = &events {
                while let Ok(event) = rx.try_recv() {
                    core.apply_event(&event);
                }
            }
            // A finished worker closes the run.
            if let Some(handle) = &worker {
                if handle.is_finished() {
                    let handle = worker.take().unwrap();
                    events = None;
                    match handle.join().unwrap_or_else(|_| Err("panic".into())) {
                        Ok(()) => core.state.step = Step::Done,
                        Err(err) => {
                            core.state.failure = Some(err);
                            core.state.step = Step::Done;
                        }
                    }
                }
            }

            let texts = if core.state.locale.starts_with("zh") {
                &ZH
            } else {
                &EN
            };
            terminal
                .draw(|frame| {
                    if uninstall_mode {
                        draw_uninstall(frame, &core, texts);
                    } else {
                        draw_wizard(
                            frame,
                            &core,
                            texts,
                            focus,
                            &mut candidate_state,
                            &locales,
                            &mut locale_choice,
                            license_scroll,
                        );
                    }
                })
                .map_err(|e| e.to_string())?;

            if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
                let Event::Key(key) = event::read().map_err(|e| e.to_string())? else {
                    continue;
                };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if uninstall_mode {
                    match key.code {
                        KeyCode::Enter if core.state.uninstall_phase == UninstallPhase::Idle => {
                            // shun's uninstall emits no events: the page runs
                            // an indeterminate phase until the worker lands.
                            core.state.uninstall_phase = UninstallPhase::Running;
                            let uninstall_core =
                                WizardCore::new(core.config.clone(), BTreeMap::new());
                            worker = Some(std::thread::spawn(move || {
                                shun::wizard::run_uninstall(&uninstall_core)
                            }));
                        }
                        KeyCode::Esc | KeyCode::Char('q') => match core.state.uninstall_phase {
                            UninstallPhase::Idle
                            | UninstallPhase::Done
                            | UninstallPhase::Failed => return Ok(()),
                            UninstallPhase::Running => {}
                        },
                        _ => {}
                    }
                    continue;
                }
                match (core.state.step, key.code, key.modifiers) {
                    (Step::Language, KeyCode::Up, _) => {
                        let i = locale_choice.selected().unwrap_or(0);
                        locale_choice.select(Some(i.saturating_sub(1)));
                    }
                    (Step::Language, KeyCode::Down, _) => {
                        let i = locale_choice.selected().unwrap_or(0);
                        locale_choice.select(Some((i + 1).min(locales.len().saturating_sub(1))));
                    }
                    (Step::Language, KeyCode::Enter, _) => {
                        if let Some(pick) = locale_choice.selected().and_then(|i| locales.get(i)) {
                            core.set_locale(pick.clone());
                        }
                        core.go(Step::Location);
                    }
                    (Step::Language, KeyCode::Esc, _) => return Ok(()),

                    (Step::Location, KeyCode::Tab, _) => {
                        focus = if focus == Focus::Path {
                            Focus::Candidates
                        } else {
                            Focus::Path
                        };
                    }
                    (Step::Location, KeyCode::Up, _) if focus == Focus::Candidates => {
                        let i = candidate_state.selected().unwrap_or(0);
                        candidate_state.select(Some(i.saturating_sub(1)));
                    }
                    (Step::Location, KeyCode::Down, _) if focus == Focus::Candidates => {
                        let i = candidate_state.selected().unwrap_or(0);
                        candidate_state.select(Some(
                            (i + 1).min(core.state.candidates.len().saturating_sub(1)),
                        ));
                    }
                    (Step::Location, KeyCode::Enter, _) if focus == Focus::Candidates => {
                        let picked = candidate_state
                            .selected()
                            .and_then(|i| core.state.candidates.get(i).cloned());
                        if let Some(pick) = picked {
                            core.set_dir(pick.path.clone());
                            core.state.dir_writable = Some(pick.writable);
                        }
                    }
                    (Step::Location, KeyCode::Enter, _) => {
                        core.nest_dir(&core.state.dir.clone());
                        core.go(Step::License);
                    }
                    (Step::Location, KeyCode::Esc, _) => core.go(Step::Language),
                    (Step::Location, KeyCode::Backspace, _) => {
                        core.state.dir.pop();
                        core.set_dir(core.state.dir.clone());
                    }
                    (Step::Location, KeyCode::Char(c), KeyModifiers::NONE)
                    | (Step::Location, KeyCode::Char(c), KeyModifiers::SHIFT) => {
                        core.state.dir.push(c);
                        core.set_dir(core.state.dir.clone());
                    }

                    (Step::License, KeyCode::PageUp, _) => {
                        license_scroll = license_scroll.saturating_sub(10);
                    }
                    (Step::License, KeyCode::PageDown, _) => {
                        license_scroll = license_scroll.saturating_add(10);
                    }
                    (Step::License, KeyCode::Char(' '), _) => {
                        core.state.agreed = !core.state.agreed;
                    }
                    (Step::License, KeyCode::Enter, _) if core.state.agreed => {
                        // The language is already on the model — the
                        // install request carries it from state.
                        core.go(Step::Install);
                        let (tx, rx) = mpsc::channel();
                        events = Some(rx);
                        let install_core = WizardCore::new(core.config.clone(), BTreeMap::new());
                        let payload = payload.clone();
                        let request = InstallRequest {
                            mode: "local".into(),
                            dir: core.state.dir.trim().to_string(),
                            language: Some(core.state.locale.clone()),
                        };
                        worker = Some(std::thread::spawn(move || {
                            shun::wizard::run_install(&install_core, &payload, &request, &mut {
                                |event| {
                                    let _ = tx.send(event.clone());
                                }
                            })
                        }));
                    }
                    (Step::License, KeyCode::Esc, _) => core.go(Step::Location),

                    (Step::Install, KeyCode::Esc, _) if worker.is_none() => core.go(Step::License),
                    (Step::Install, _, _) => {}

                    (Step::Done, KeyCode::Char(' '), _) => {
                        // Cycle the done-page toggles in order.
                        if core.state.failure.is_none() {
                            if !core.state.desktop_shortcut {
                                core.state.desktop_shortcut = true;
                            } else if !core.state.launch_after {
                                core.state.launch_after = true;
                            } else {
                                core.state.desktop_shortcut = false;
                            }
                        }
                    }
                    (Step::Done, KeyCode::Enter, _) if core.state.failure.is_none() => {
                        let dir = core.state.dir.trim().to_string();
                        let desktop = core.state.desktop_shortcut;
                        let launch = core.state.launch_after;
                        let finish_core = WizardCore::new(core.config.clone(), BTreeMap::new());
                        let _ = shun::wizard::apply_finish(
                            &finish_core,
                            &dir,
                            Some(desktop),
                            Some(true),
                            launch,
                        );
                        return Ok(());
                    }
                    (Step::Done, KeyCode::Esc, _) | (Step::Done, KeyCode::Char('q'), _) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    })();

    disable_raw_mode().map_err(|e| e.to_string())?;
    let _ = io::stdout().execute(LeaveAlternateScreen);
    result
}

/// The license documents as the wizard model consumes them (locale-keyed
/// from the embedded build-time map — the TUI re-uses the same artifact
/// the GUI faces embed).
fn license_docs_view() -> BTreeMap<String, Vec<shun::wizard::LicenseDoc>> {
    // The embedded map lives in the crate's OUT_DIR (written by build.rs).
    const LICENSE_DOCS_JSON: &str =
        include_str!(concat!(env!("OUT_DIR"), "/shun-license-docs.json"));
    let raw: BTreeMap<String, Vec<shun::config::ResolvedLicenseDoc>> =
        serde_json::from_str(LICENSE_DOCS_JSON).unwrap_or_default();
    raw.into_iter()
        .map(|(locale, docs)| {
            (
                locale,
                docs.into_iter()
                    .map(|doc| shun::wizard::LicenseDoc {
                        title: doc.title.unwrap_or_default(),
                        body: doc.body,
                    })
                    .collect(),
            )
        })
        .collect()
}

/// The left rail: the same five steps every face shows.
fn rail_items(state: &WizardState, texts: &Texts) -> List<'static> {
    let current = step_index(state.step);
    let items: Vec<ListItem> = Step::RAIL
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let label = texts.rail[step_index(*step)];
            let (marker, style) = if index < current {
                ("✓ ", Style::default().fg(Color::Green))
            } else if index == current {
                (
                    "● ",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Cyan),
                )
            } else {
                ("○ ", Style::default().fg(Color::DarkGray))
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(label, style),
            ]))
        })
        .collect();
    List::new(items).block(Block::default().borders(Borders::RIGHT).title(Span::styled(
        state_title(state, texts),
        Style::default().add_modifier(Modifier::BOLD),
    )))
}

fn state_title(state: &WizardState, _texts: &Texts) -> String {
    // Product identity from the model — the same manifest-derived name
    // the GUI title bars show.
    format!(" {} ", product_of(state))
}

/// The product name carried alongside the state for rendering (the
/// wizard state itself is product-agnostic; the runner injects it).
fn product_of(_state: &WizardState) -> String {
    PRODUCT.with(|p| p.borrow().clone())
}

thread_local! {
    static PRODUCT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Sets the render-time product identity (called once at startup).
fn set_product(name: &str) {
    PRODUCT.with(|p| *p.borrow_mut() = name.to_string());
}

#[allow(clippy::too_many_arguments)]
fn draw_wizard(
    frame: &mut Frame,
    core: &WizardCore,
    texts: &Texts,
    focus: Focus,
    candidate_state: &mut ListState,
    locales: &[String],
    locale_choice: &mut ListState,
    license_scroll: u16,
) {
    set_product(&core.config.product.name);
    let [rail_area, pane_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(18), Constraint::Min(40)])
        .areas(frame.area());

    frame.render_stateful_widget(
        rail_items(&core.state, texts),
        rail_area,
        &mut ListState::default(),
    );

    let [pane, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .areas(pane_area);

    match core.state.step {
        Step::Language => {
            let items: Vec<ListItem> = locales.iter().map(|l| ListItem::new(l.clone())).collect();
            let list = List::new(items)
                .block(title_block(texts.language.0))
                .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));
            frame.render_stateful_widget(list, pane, locale_choice);
            let hint = Paragraph::new(texts.language.1).wrap(Wrap { trim: true });
            let [_, hint_area] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .areas(pane);
            frame.render_widget(hint, hint_area);
        }
        Step::Location => {
            let [head, path_row, picks, hint] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(5),
                    Constraint::Length(2),
                ])
                .areas(pane);
            frame.render_widget(
                Paragraph::new(texts.location.0).block(title_block("")),
                head,
            );
            let path_style = if focus == Focus::Path {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            let writable_span = match core.state.dir_writable {
                Some(true) => Span::styled(" ✓", Style::default().fg(Color::Green)),
                Some(false) => Span::styled(" ✗", Style::default().fg(Color::Red)),
                None => Span::raw(""),
            };
            let path = Paragraph::new(Line::from(vec![
                Span::styled(core.state.dir.clone(), path_style),
                writable_span,
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(texts.location.1),
            );
            frame.render_widget(path, path_row);

            let items: Vec<ListItem> = core
                .state
                .candidates
                .iter()
                .map(|candidate| {
                    let mark = if candidate.writable { "✓ " } else { "✗ " };
                    ListItem::new(format!("{mark}{}", candidate.path))
                })
                .collect();
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(texts.location.2),
                )
                .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));
            frame.render_stateful_widget(list, picks, candidate_state);

            let hint_text = match core.state.dir_writable {
                Some(false) => "⚠ not writable — pick a highlighted candidate".to_string(),
                _ => String::new(),
            };
            frame.render_widget(Paragraph::new(hint_text), hint);
        }
        Step::License => {
            let docs = core.license_docs_for(&core.state.locale);
            let index = core.state.license_index.min(docs.len().saturating_sub(1));
            let doc = docs.get(index);
            let body = doc.map(|d| d.body.clone()).unwrap_or_default();
            let pager = if docs.len() > 1 {
                format!(
                    " [{}/{}] {}",
                    index + 1,
                    docs.len(),
                    doc.map(|d| d.title.clone()).unwrap_or_default()
                )
            } else {
                String::new()
            };
            let paragraph = Paragraph::new(body)
                .block(title_block(&format!("{}{}", texts.license.0, pager)))
                .wrap(Wrap { trim: false })
                .scroll((license_scroll, 0));
            frame.render_widget(paragraph, pane);
            let footer_line = Line::from(if core.state.agreed {
                vec![
                    Span::styled("[x] ", Style::default().fg(Color::Green)),
                    Span::raw(texts.license.1),
                ]
            } else {
                vec![Span::raw("[  ] "), Span::raw(texts.license.1)]
            });
            frame.render_widget(
                Paragraph::new(footer_line),
                Rect {
                    y: pane.height.saturating_sub(1).saturating_add(pane.y),
                    ..pane
                },
            );
        }
        Step::Install => {
            let [head, gauge_area, log] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(3),
                ])
                .areas(pane);
            frame.render_widget(
                Paragraph::new(format!("{} {}", texts.install, product_of(&core.state)))
                    .block(title_block("")),
                head,
            );
            let gauge = match core.state.progress {
                Some(percent) => Gauge::default()
                    .percent(u16::from(percent))
                    .label(core.state.flow_step.clone()),
                None => Gauge::default().label(core.state.flow_step.clone()),
            };
            frame.render_widget(gauge, gauge_area);
            let tail: Vec<Line> = core
                .state
                .log
                .iter()
                .rev()
                .take(pane_log_lines(&log))
                .map(|line| {
                    let style = match line.kind {
                        shun::wizard::LogKind::Error => Style::default().fg(Color::Red),
                        shun::wizard::LogKind::Ok => Style::default().fg(Color::Green),
                        shun::wizard::LogKind::Step => {
                            Style::default().add_modifier(Modifier::BOLD)
                        }
                        shun::wizard::LogKind::Echo => Style::default(),
                    };
                    Line::styled(format!("{} {}", line.time, line.text), style)
                })
                .collect();
            frame.render_widget(
                Paragraph::new(tail).block(Block::default().borders(Borders::ALL)),
                log,
            );
        }
        Step::Done => {
            let failure = core.state.failure.clone().unwrap_or_default();
            let lines: Vec<Line> = if let Some(err) = &core.state.failure {
                vec![
                    Line::styled(
                        texts.done.1,
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Line::raw(err.clone()),
                    Line::raw(format!("({})", texts.nav.0)),
                ]
            } else {
                vec![
                    Line::styled(
                        texts.done.0,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Line::raw(core.state.dir.trim().to_string()),
                    Line::from(
                        if core.state.desktop_shortcut {
                            "[x] "
                        } else {
                            "[  ] "
                        }
                        .to_string()
                            + texts.done.2,
                    ),
                    Line::from(
                        if core.state.launch_after {
                            "[x] "
                        } else {
                            "[  ] "
                        }
                        .to_string()
                            + texts.done.3,
                    ),
                ]
            };
            let _ = failure;
            frame.render_widget(
                Paragraph::new(lines)
                    .block(title_block(""))
                    .wrap(Wrap { trim: true }),
                pane,
            );
        }
    }

    frame.render_widget(Paragraph::new(texts.footer), footer);
}

fn draw_uninstall(frame: &mut Frame, core: &WizardCore, texts: &Texts) {
    set_product(&core.config.product.name);
    let area = frame.area();
    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" {} — {}", product_of(&core.state), texts.uninstall.0),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line> = match core.state.uninstall_phase {
        UninstallPhase::Idle => vec![
            Line::raw(""),
            Line::raw(uninstall_target_dir().unwrap_or_default()),
            Line::raw(""),
            Line::styled(
                format!(
                    "Enter: {}   R: {}   Esc: 退出",
                    texts.uninstall.0, texts.uninstall.1
                ),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ],
        UninstallPhase::Running => vec![
            Line::raw(""),
            Line::styled(texts.uninstall.2, Style::default().fg(Color::Cyan)),
        ],
        UninstallPhase::Done => vec![
            Line::raw(""),
            Line::styled(texts.uninstall.3, Style::default().fg(Color::Green)),
            Line::raw(""),
            Line::raw("Esc: 退出"),
        ],
        UninstallPhase::Failed => vec![
            Line::raw(""),
            Line::styled(texts.uninstall.4, Style::default().fg(Color::Red)),
            Line::raw(core.state.uninstall_error.clone()),
            Line::raw(""),
            Line::raw("Esc: 退出"),
        ],
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn title_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_string())
}

fn pane_log_lines(area: &Rect) -> usize {
    area.height.saturating_sub(2) as usize
}

/// The install dir the uninstaller copy sits in (for the uninstall
/// page's display).
fn uninstall_target_dir() -> Option<String> {
    shun::wizard::current_exe_dir().map(|dir| dir.to_string_lossy().into_owned())
}

/// Rail order index of a step (the highlight comparison).
fn step_index(step: Step) -> usize {
    match step {
        Step::Language => 0,
        Step::Location => 1,
        Step::License => 2,
        Step::Install => 3,
        Step::Done => 4,
    }
}
