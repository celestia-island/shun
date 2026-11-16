//! The terminal face: the same wizard model (`shun::wizard`) rendered
//! with ratatui — a left step rail exactly like the GUI shells, the
//! location step with its candidate row, the paged license agreement,
//! the install gauge with the log tail, the done page with the
//! shortcut toggles, and the standalone uninstall/repair page.
//!
//! The face adapts to the host: the language opens on the system UI
//! language (`shun::env_probe::system_locale`), the palette degrades from
//! truecolor to mono, box characters fall back to ASCII on legacy
//! consoles, markdown images draw as half blocks where colors allow it,
//! and mouse input is captured only when the terminal reports it
//! (`crate::tui_caps`). Keyboard stays complete and authoritative.
//!
//! Keymap (also shown in the footer bar):
//! `↑/↓` move · `Enter` next/confirm · `Esc` back/exit · `Tab` switch
//! focus (path ↔ candidates) · `Space` toggle · `PgUp/PgDn` scroll the
//! license · typing edits the path · wheel scrolls · clicks act on the
//! rail, the candidates, the checkboxes and the primary button.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use image::GenericImageView;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, List, ListItem, ListState, Paragraph, Wrap};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::Resize;
use shun::config::ShunConfig;
use shun::failure::FinishStage;
use shun::flow::FlowEvent;
use shun::payload::ArchivePayload;
use shun::wizard::{InstallRequest, Step, UninstallPhase, WizardCore, WizardState};

use crate::tui_caps::{ImageSupport, TerminalCaps, probe};
use crate::tui_i18n::{LANGUAGES, Texts, normalize_locale, texts_for};
use crate::tui_image::{ImageCache, image_markers};
use crate::tui_theme::Theme;

/// Which element of the location step holds the cursor.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Focus {
    Path,
    Candidates,
}

/// A clickable region recorded during the last frame.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Hit {
    Rail(Step),
    Candidate(usize),
    Locale(usize),
    LicenseAgree,
    DoneDesktop,
    DoneLaunch,
    DonePrimary,
    UninstallPrimary,
    LicensePane,
}

/// Mutable render/input state shared by the loop and the draw calls.
struct Ui {
    caps: TerminalCaps,
    theme: Theme,
    images: ImageCache,
    /// The terminal's graphics-protocol picker (None = the queries failed
    /// AND forcing was declined - the halfblock lane covers images).
    picker: Option<Picker>,
    /// The last frame's clickable regions.
    hits: Vec<(Rect, Hit)>,
}

impl Ui {
    fn new(caps: TerminalCaps, picker: Option<Picker>) -> Self {
        let theme = Theme::new(&caps);
        Self {
            caps,
            theme,
            images: ImageCache::default(),
            picker,
            hits: Vec::new(),
        }
    }

    fn begin_frame(&mut self) {
        self.hits.clear();
    }

    fn clickable(&mut self, area: Rect, hit: Hit) {
        if area.width > 0 && area.height > 0 {
            self.hits.push((area, hit));
        }
    }
}

/// Runs the terminal wizard. `uninstall_mode` renders the standalone
/// uninstall/repair page instead (the TTY's answer to the ARP entry).
/// `relocate_from` seeds the relocation pass: the old folder is neither
/// offered nor preselected, the pane opens on location with the notice,
/// and the install request carries it.
pub fn run(
    config: ShunConfig,
    payload: ArchivePayload,
    uninstall_mode: bool,
    relocate_from: Option<String>,
) -> Result<(), String> {
    // The host language leads; the picker only overrides it. Windows has
    // no LANG, so the probe reads the user's default UI language there.
    let host_locale = shun::env_probe::system_locale().unwrap_or("en");
    let mut core = {
        let mut core = WizardCore::new(config, license_docs_view());
        core.set_locale(normalize_locale(host_locale));
        core
    };
    let mut ui = Ui::new(probe(), None);
    if uninstall_mode {
        core.state.step = Step::Done; // the uninstall page replaces the rail flow
    }

    // The relocation pass opens straight on the location step: the old
    // folder drops out of the quick picks (so the preselection — the
    // first writable candidate, the registered install leads the chain
    // — lands on a DIFFERING folder), and the pane leads with its
    // notice. Escaping at any point leaves the old install untouched.
    let relocating = relocate_from.is_some() && !uninstall_mode;
    if relocating {
        let old = relocate_from.clone().unwrap();
        core.state.step = Step::Location;
        core.state
            .candidates
            .retain(|candidate| !crate::same_install_dir(&candidate.path, &old));
        if crate::same_install_dir(core.state.dir.trim(), &old) {
            if let Some(pick) = core.state.candidates.iter().find(|c| c.writable).cloned() {
                core.set_dir(pick.path.clone());
                core.state.dir_writable = Some(pick.writable);
            }
        }
    }

    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = io::stdout();
    let _ = stdout.execute(EnterAlternateScreen);
    if ui.caps.mouse {
        let _ = stdout.execute(EnableMouseCapture);
    }
    // crossterm falls back to the console attribute API on Windows, which
    // keeps color codes out of the byte stream; the probe above already
    // enabled VT processing, so ask for ANSI SGR explicitly — real
    // terminals render the same palette and the e2e harness can assert it.
    crossterm::style::force_color_output(ui.caps.color != crate::tui_caps::ColorDepth::Mono);
    // Graphics-protocol probe (kitty / sixel / iTerm2 / halfblocks + the
    // font size in pixels): inside the alternate screen, raw mode on, and
    // BEFORE the event loop starts reading. Nothing answers in the ConPTY
    // harness, which resolves to the halfblocks default - that is exactly
    // the degradation the e2e asserts. SHUN_TUI_FORCE_PROTOCOL pins a lane
    // (kitty/sixel/iterm2/halfblocks/none) for tests and support.
    ui.picker = ratatui_image::picker::Picker::from_query_stdio().ok();
    ui.picker = match std::env::var("SHUN_TUI_FORCE_PROTOCOL").as_deref() {
        Ok("kitty") => {
            if let Some(picker) = ui.picker.as_mut() {
                picker.set_protocol_type(ProtocolType::Kitty);
            }
            ui.picker.take()
        }
        Ok("sixel") => {
            if let Some(picker) = ui.picker.as_mut() {
                picker.set_protocol_type(ProtocolType::Sixel);
            }
            ui.picker.take()
        }
        Ok("iterm2") => {
            if let Some(picker) = ui.picker.as_mut() {
                picker.set_protocol_type(ProtocolType::Iterm2);
            }
            ui.picker.take()
        }
        Ok("halfblocks") => {
            if let Some(picker) = ui.picker.as_mut() {
                picker.set_protocol_type(ProtocolType::Halfblocks);
            }
            ui.picker.take()
        }
        Ok("none") => None,
        _ => ui.picker.take(),
    };
    let backend = ratatui::backend::CrosstermBackend::new(&mut stdout);
    let mut terminal = ratatui::Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut focus = Focus::Path;
    let mut candidate_state = ListState::default();
    candidate_state.select(Some(0));
    let mut license_scroll: u16 = 0;
    // The shared directory-guard verdict for the location step's Enter
    // gate (drive-root refusal + check-dir hooks); rendered on the pane
    // while set, cleared by any path edit.
    let mut dir_fault: Option<shun::wizard::DirCheckFault> = None;
    // The picker lists the eight documented languages (native names); the
    // model's locale drives both the chrome and the license documents.
    let mut locale_choice = ListState::default();
    let selected_locale = normalize_locale(&core.state.locale);
    locale_choice.select(Some(
        LANGUAGES
            .iter()
            .position(|(locale, _)| *locale == selected_locale)
            .unwrap_or(0),
    ));
    // (install worker join, event channel) while Step::Install runs.
    let mut worker: Option<std::thread::JoinHandle<Result<(), String>>> = None;
    let mut events: Option<mpsc::Receiver<FlowEvent>> = None;
    // Whether the done page's failure line came from the FINISH answers
    // (shortcuts / the launch) rather than the install itself: Enter
    // retries a failed finish, while an install failure stays terminal
    // (nothing was delivered to finish).
    let mut finish_failed = false;
    // Which half of the finish failed (drives the done page's failure
    // title); None while the failure is the install's own.
    let mut finish_stage: Option<FinishStage> = None;

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
                        // The uninstall page has its own phase machine —
                        // leaving it on Running made a finished uninstall
                        // look hung (the wizard's Done step is meaningless
                        // in that mode).
                        Ok(()) => {
                            if uninstall_mode {
                                core.state.uninstall_phase = UninstallPhase::Done;
                            } else {
                                core.state.step = Step::Done;
                            }
                        }
                        Err(err) => {
                            if uninstall_mode {
                                core.state.uninstall_phase = UninstallPhase::Failed;
                                core.state.uninstall_error = err;
                            } else {
                                core.state.failure = Some(err);
                                core.state.step = Step::Done;
                            }
                        }
                    }
                }
            }

            let texts: &'static Texts = texts_for(&core.state.locale);
            ui.begin_frame();
            terminal
                .draw(|frame| {
                    if uninstall_mode {
                        draw_uninstall(frame, &core, texts, &mut ui);
                    } else {
                        draw_wizard(
                            frame,
                            &core,
                            texts,
                            &mut ui,
                            focus,
                            &mut candidate_state,
                            &mut locale_choice,
                            license_scroll,
                            dir_fault.as_ref(),
                            relocating,
                            &payload,
                            finish_failed,
                            finish_stage,
                        );
                    }
                })
                .map_err(|e| e.to_string())?;

            if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
                let event = event::read().map_err(|e| e.to_string())?;
                match event {
                    Event::Mouse(mouse) => {
                        match mouse.kind {
                            MouseEventKind::ScrollDown if core.state.step == Step::License => {
                                license_scroll = license_scroll.saturating_add(3);
                            }
                            MouseEventKind::ScrollUp if core.state.step == Step::License => {
                                license_scroll = license_scroll.saturating_sub(3);
                            }
                            MouseEventKind::Down(MouseButton::Left) => {
                                let hit = ui
                                    .hits
                                    .iter()
                                    .rev()
                                    .find(|(rect, _)| {
                                        mouse.column >= rect.x
                                            && mouse.column < rect.x + rect.width
                                            && mouse.row >= rect.y
                                            && mouse.row < rect.y + rect.height
                                    })
                                    .map(|(_, hit)| *hit);
                                let Some(hit) = hit else { continue };
                                if uninstall_mode {
                                    if matches!(hit, Hit::UninstallPrimary)
                                        && matches!(
                                            core.state.uninstall_phase,
                                            UninstallPhase::Idle | UninstallPhase::Failed
                                        )
                                    {
                                        core.state.uninstall_phase = UninstallPhase::Running;
                                        let uninstall_core =
                                            WizardCore::new(core.config.clone(), BTreeMap::new());
                                        worker = Some(std::thread::spawn(move || {
                                            shun::wizard::run_uninstall(&uninstall_core)
                                        }));
                                    }
                                    continue;
                                }
                                match hit {
                                    // Clicking an earlier rail step goes back there;
                                    // future steps stay inert (the flow gates them).
                                    Hit::Rail(step)
                                        if step_index(step) < step_index(core.state.step)
                                            && !matches!(
                                                core.state.step,
                                                Step::Install | Step::Done
                                            )
                                            && worker.is_none() =>
                                    {
                                        core.go(step);
                                    }
                                    Hit::Locale(index) => {
                                        if let Some((locale, _)) = LANGUAGES.get(index) {
                                            core.set_locale(normalize_locale(locale));
                                        }
                                        core.go(Step::Location);
                                    }
                                    Hit::Candidate(index) => {
                                        if let Some(pick) =
                                            core.state.candidates.get(index).cloned()
                                        {
                                            core.set_dir(pick.path.clone());
                                            core.state.dir_writable = Some(pick.writable);
                                            dir_fault = None;
                                        }
                                    }
                                    Hit::LicenseAgree => core.state.agreed = !core.state.agreed,
                                    Hit::DoneDesktop => {
                                        if core.state.failure.is_none() {
                                            core.state.desktop_shortcut =
                                                !core.state.desktop_shortcut;
                                        }
                                    }
                                    Hit::DoneLaunch => {
                                        if core.state.failure.is_none() {
                                            core.state.launch_after = !core.state.launch_after;
                                        }
                                    }
                                    Hit::DonePrimary => {
                                        if core.state.failure.is_none() || finish_failed {
                                            let dir = core.state.dir.trim().to_string();
                                            let desktop = core.state.desktop_shortcut;
                                            let launch = core.state.launch_after;
                                            let finish_core = WizardCore::new(
                                                core.config.clone(),
                                                BTreeMap::new(),
                                            );
                                            match shun::wizard::apply_finish_checked(
                                                &finish_core,
                                                &dir,
                                                Some(desktop),
                                                Some(true),
                                                launch,
                                            ) {
                                                Ok(()) => return Ok(()),
                                                Err(failure) => {
                                                    finish_failed = true;
                                                    finish_stage = Some(failure.stage);
                                                    core.state.failure = Some(failure.message);
                                                }
                                            }
                                        }
                                    }
                                    Hit::Rail(_) | Hit::LicensePane | Hit::UninstallPrimary => {}
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }
                    Event::Key(key) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }
                        if uninstall_mode {
                            match key.code {
                                KeyCode::Enter
                                    if matches!(
                                        core.state.uninstall_phase,
                                        UninstallPhase::Idle | UninstallPhase::Failed
                                    ) =>
                                {
                                    // shun's uninstall emits no events: the page runs
                                    // an indeterminate phase until the worker lands.
                                    core.state.uninstall_phase = UninstallPhase::Running;
                                    let uninstall_core =
                                        WizardCore::new(core.config.clone(), BTreeMap::new());
                                    worker = Some(std::thread::spawn(move || {
                                        shun::wizard::run_uninstall(&uninstall_core)
                                    }));
                                }
                                KeyCode::Char('r') | KeyCode::Char('R')
                                    if core.state.uninstall_phase == UninstallPhase::Failed =>
                                {
                                    core.state.uninstall_phase = UninstallPhase::Running;
                                    let uninstall_core =
                                        WizardCore::new(core.config.clone(), BTreeMap::new());
                                    worker = Some(std::thread::spawn(move || {
                                        shun::wizard::run_uninstall(&uninstall_core)
                                    }));
                                }
                                KeyCode::Esc | KeyCode::Char('q') => {
                                    match core.state.uninstall_phase {
                                        UninstallPhase::Idle
                                        | UninstallPhase::Done
                                        | UninstallPhase::Failed => return Ok(()),
                                        UninstallPhase::Running => {}
                                    }
                                }
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
                                locale_choice.select(Some((i + 1).min(LANGUAGES.len() - 1)));
                            }
                            (Step::Language, KeyCode::Enter, _) => {
                                if let Some((locale, _)) =
                                    locale_choice.selected().and_then(|i| LANGUAGES.get(i))
                                {
                                    core.set_locale(normalize_locale(locale));
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
                                    dir_fault = None;
                                }
                            }
                            // Enter on the path: the shared directory guards gate
                            // the step — a fault renders on the pane and the step
                            // stays. The nest call is the legacy pad lane (a no-op
                            // under the default root refusal).
                            (Step::Location, KeyCode::Enter, _) => {
                                core.nest_dir(&core.state.dir.clone());
                                dir_fault = shun::wizard::check_install_dir(
                                    &core.config,
                                    &payload,
                                    core.state.dir.trim(),
                                    Some(&core.state.locale),
                                    false,
                                );
                                if dir_fault.is_none() {
                                    core.go(Step::License);
                                }
                            }
                            (Step::Location, KeyCode::Esc, _) => core.go(Step::Language),
                            (Step::Location, KeyCode::Backspace, _) => {
                                core.state.dir.pop();
                                core.set_dir(core.state.dir.clone());
                                dir_fault = None;
                            }
                            (Step::Location, KeyCode::Char(c), KeyModifiers::NONE)
                            | (Step::Location, KeyCode::Char(c), KeyModifiers::SHIFT) => {
                                core.state.dir.push(c);
                                core.set_dir(core.state.dir.clone());
                                dir_fault = None;
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
                                let install_core =
                                    WizardCore::new(core.config.clone(), BTreeMap::new());
                                let payload = payload.clone();
                                let request = InstallRequest {
                                    mode: "local".into(),
                                    dir: core.state.dir.trim().to_string(),
                                    language: Some(core.state.locale.clone()),
                                    // The TUI exposes no scope toggle — per-user.
                                    machine: false,
                                    // The relocation handover (None for plain
                                    // installs): the flow sweeps only the
                                    // product's own files out of the old folder.
                                    relocate_from: relocate_from.clone(),
                                };
                                worker = Some(std::thread::spawn(move || {
                                    shun::wizard::run_install(
                                        &install_core,
                                        &payload,
                                        &request,
                                        &mut {
                                            |event| {
                                                let _ = tx.send(event.clone());
                                            }
                                        },
                                    )
                                }));
                            }
                            (Step::License, KeyCode::Esc, _) => core.go(Step::Location),

                            (Step::Install, KeyCode::Esc, _) if worker.is_none() => {
                                core.go(Step::License)
                            }
                            (Step::Install, _, _) => {}

                            (Step::Done, KeyCode::Char(' '), _) => {
                                // Cycle the done-page toggles in order — a failed
                                // FINISH keeps them live so the answers can change
                                // before the retry (an install failure does not).
                                if core.state.failure.is_none() || finish_failed {
                                    if !core.state.desktop_shortcut {
                                        core.state.desktop_shortcut = true;
                                    } else if !core.state.launch_after {
                                        core.state.launch_after = true;
                                    } else {
                                        core.state.desktop_shortcut = false;
                                    }
                                }
                            }
                            // The done page's answers: apply them and leave. A
                            // failure is NOT swallowed — the pane keeps the
                            // classified line (the red branch) and Enter retries
                            // while q/Esc quits (issue #168: this used to
                            // `let _ =` the result and exit as if the launch had
                            // worked). An INSTALL failure keeps its old, terminal
                            // meaning: Enter is inert there.
                            (Step::Done, KeyCode::Enter, _) => {
                                if core.state.failure.is_none() || finish_failed {
                                    let dir = core.state.dir.trim().to_string();
                                    let desktop = core.state.desktop_shortcut;
                                    let launch = core.state.launch_after;
                                    let finish_core =
                                        WizardCore::new(core.config.clone(), BTreeMap::new());
                                    match shun::wizard::apply_finish_checked(
                                        &finish_core,
                                        &dir,
                                        Some(desktop),
                                        Some(true),
                                        launch,
                                    ) {
                                        Ok(()) => return Ok(()),
                                        Err(failure) => {
                                            finish_failed = true;
                                            finish_stage = Some(failure.stage);
                                            core.state.failure = Some(failure.message);
                                        }
                                    }
                                }
                            }
                            (Step::Done, KeyCode::Esc, _) | (Step::Done, KeyCode::Char('q'), _) => {
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    disable_raw_mode().map_err(|e| e.to_string())?;
    if ui.caps.mouse {
        let _ = io::stdout().execute(DisableMouseCapture);
    }
    let _ = io::stdout().execute(LeaveAlternateScreen);
    result
}

/// The left rail: the same five steps every face shows, themed.
fn draw_rail(
    frame: &mut Frame,
    area: Rect,
    state: &WizardState,
    texts: &'static Texts,
    ui: &mut Ui,
) {
    let current = step_index(state.step);
    let mut items: Vec<ListItem> = Vec::new();
    for (index, step) in Step::RAIL.iter().enumerate() {
        let label = texts.rail[step_index(*step)];
        let (marker, style) = if index < current {
            (ui.theme.glyphs.done, ui.theme.ok)
        } else if index == current {
            (ui.theme.glyphs.active, ui.theme.accent)
        } else {
            (ui.theme.glyphs.todo, ui.theme.muted)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{marker} "), style),
            Span::styled(label, style),
        ])));
    }
    let list = List::new(items).block(ui.theme.rail_block(&format!(" {} ", product_of(state))));
    frame.render_widget(list, area);
    // Every rail row clicks (jumping BACK is allowed; the flow gates forward).
    for (index, step) in Step::RAIL.iter().enumerate() {
        let row = Rect {
            x: area.x,
            y: area.y + 1 + index as u16,
            width: area.width,
            height: 1,
        };
        if row.y < area.y + area.height {
            ui.clickable(row, Hit::Rail(*step));
        }
    }
}

/// The wizard panes (language / location / license / install / done).
#[allow(clippy::too_many_arguments)]
fn draw_wizard(
    frame: &mut Frame,
    core: &WizardCore,
    texts: &'static Texts,
    ui: &mut Ui,
    focus: Focus,
    candidate_state: &mut ListState,
    locale_choice: &mut ListState,
    license_scroll: u16,
    dir_fault: Option<&shun::wizard::DirCheckFault>,
    relocating: bool,
    payload: &ArchivePayload,
    finish_failed: bool,
    finish_stage: Option<FinishStage>,
) {
    set_product(&core.config.product.name);
    let [rail_area, pane_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(40)])
        .areas(frame.area());

    draw_rail(frame, rail_area, &core.state, texts, ui);

    let [pane, footer] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .areas(pane_area);

    match core.state.step {
        Step::Language => {
            let items: Vec<ListItem> = LANGUAGES
                .iter()
                .map(|(locale, name)| {
                    let mark = if normalize_locale(&core.state.locale) == *locale {
                        ui.theme.glyphs.active
                    } else {
                        ui.theme.glyphs.todo
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("{mark} "), ui.theme.accent),
                        Span::raw(*name),
                    ]))
                })
                .collect();
            // The hint gets its own row under the list instead of painting
            // over the list block's bottom border (master's layout did).
            let [list_area, hint_area] = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .areas(pane);
            let list = List::new(items)
                .block(ui.theme.block(texts.language.0))
                .highlight_style(ui.theme.selected);
            frame.render_stateful_widget(list, list_area, locale_choice);
            for index in 0..LANGUAGES.len() {
                let row = Rect {
                    x: list_area.x + 1,
                    y: list_area.y + 1 + index as u16,
                    width: list_area.width.saturating_sub(2),
                    height: 1,
                };
                if row.y < list_area.y + list_area.height {
                    ui.clickable(row, Hit::Locale(index));
                }
            }
            let hint = Paragraph::new(Span::styled(texts.language.1, ui.theme.muted))
                .wrap(Wrap { trim: true });
            frame.render_widget(hint, hint_area);
        }
        Step::Location => {
            // The relocation pass slots its notice between the heading
            // and the path row (title bold + wrapped body).
            let mut constraints = vec![Constraint::Length(3)];
            if relocating {
                constraints.push(Constraint::Length(4));
            }
            constraints.extend([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Length(2),
            ]);
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(pane);
            let mut next = 0;
            let head = rows[next];
            next += 1;
            let notice = relocating.then(|| {
                let area = rows[next];
                next += 1;
                area
            });
            let path_row = rows[next];
            let picks = rows[next + 1];
            let hint = rows[next + 2];
            frame.render_widget(
                Paragraph::new(Span::styled(texts.location.0, ui.theme.title))
                    .block(ui.theme.block("")),
                head,
            );
            if let Some(area) = notice {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled(texts.relocate.0, ui.theme.warn),
                        Line::raw(texts.relocate.1),
                    ])
                    .wrap(Wrap { trim: true }),
                    area,
                );
            }
            let path_style = if focus == Focus::Path {
                ui.theme.accent
            } else {
                ui.theme.plain
            };
            let writable_span = match core.state.dir_writable {
                Some(true) => Span::styled(format!(" {}", ui.theme.glyphs.ok), ui.theme.ok),
                Some(false) => Span::styled(format!(" {}", ui.theme.glyphs.bad), ui.theme.error),
                None => Span::raw(""),
            };
            let path = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(core.state.dir.clone(), path_style),
                writable_span,
            ]))
            .block(ui.theme.block(texts.location.1));
            frame.render_widget(path, path_row);

            let items: Vec<ListItem> = core
                .state
                .candidates
                .iter()
                .map(|candidate| {
                    let (mark, style) = if candidate.writable {
                        (ui.theme.glyphs.ok, ui.theme.ok)
                    } else {
                        (ui.theme.glyphs.bad, ui.theme.error)
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("{mark} "), style),
                        Span::raw(candidate.path.clone()),
                    ]))
                })
                .collect();
            let list = List::new(items)
                .block(ui.theme.block(texts.location.2))
                .highlight_style(ui.theme.selected);
            frame.render_stateful_widget(list, picks, candidate_state);
            for index in 0..core.state.candidates.len() {
                let row = Rect {
                    x: picks.x + 1,
                    y: picks.y + 1 + index as u16,
                    width: picks.width.saturating_sub(2),
                    height: 1,
                };
                if row.y < picks.y + picks.height {
                    ui.clickable(row, Hit::Candidate(index));
                }
            }

            // The guard verdict outranks the writability hint; a script
            // veto carries its own localized message, the built-in
            // faults render the English lib sentence (this face's live
            // hints are English-only, like the writability line).
            let hint_line = match dir_fault {
                Some(fault) => Line::styled(
                    format!("  {} {}", ui.theme.glyphs.warn, fault.message()),
                    ui.theme.error,
                ),
                None => match core.state.dir_writable {
                    Some(false) => Line::styled(
                        format!(
                            "  {} not writable — pick a highlighted candidate",
                            ui.theme.glyphs.warn
                        ),
                        ui.theme.warn,
                    ),
                    _ => Line::raw(""),
                },
            };
            frame.render_widget(Paragraph::new(hint_line).wrap(Wrap { trim: false }), hint);
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
            // Markdown image markers have two render lanes. NATIVE (the
            // picker detected kitty / sixel / iTerm2): every standalone
            // marker reserves its own slot under the text and renders
            // through ratatui-image's protocol backend. Otherwise half
            // blocks (16 colors and up) render INLINE in the pager - the
            // lane that keeps scrolling working everywhere - and mono
            // keeps a labelled placeholder (never a corrupted screen).
            let markers = image_markers(&body);
            let native = ui.picker.as_ref().is_some_and(|picker| {
                matches!(
                    picker.protocol_type(),
                    ProtocolType::Kitty | ProtocolType::Sixel | ProtocolType::Iterm2
                )
            });
            let mut lines: Vec<Line<'static>> = Vec::new();
            let mut img_slots: Vec<image::DynamicImage> = Vec::new();
            let mut img_rows: Vec<u16> = Vec::new();
            let mut marker_at = 0;
            for (line_index, text) in body.lines().enumerate() {
                let Some(marker) = markers.get(marker_at) else {
                    lines.push(Line::raw(text.to_string()));
                    continue;
                };
                if marker.line != line_index {
                    lines.push(Line::raw(text.to_string()));
                    continue;
                }
                marker_at += 1;
                let bytes = payload.read_file(std::path::Path::new(&marker.path));
                if native {
                    let placeholder =
                        Line::styled(format!("  [image: {}]", marker.alt), ui.theme.muted);
                    let Some(bytes) = bytes else {
                        lines.push(placeholder);
                        continue;
                    };
                    let Ok(dyn_img) = image::load_from_memory(bytes) else {
                        lines.push(placeholder);
                        continue;
                    };
                    let (font_w, font_h) = picker_font_size(ui);
                    if font_h == 0 {
                        lines.push(Line::styled(
                            format!("  [image: {}]", marker.alt),
                            ui.theme.muted,
                        ));
                        continue;
                    }
                    let cols = u64::from(pane.width.saturating_sub(2).max(1));
                    let (w_px, h_px) = dyn_img.dimensions();
                    if w_px == 0 || h_px == 0 {
                        lines.push(placeholder);
                        continue;
                    }
                    let disp_w_px = cols * u64::from(font_w);
                    let disp_h_px = u64::from(h_px) * disp_w_px / u64::from(w_px);
                    let rows = u16::try_from(disp_h_px / u64::from(font_h))
                        .unwrap_or(u16::MAX)
                        .clamp(1, 16);
                    img_slots.push(dyn_img);
                    img_rows.push(rows);
                    continue;
                }
                let cols = pane.width.saturating_sub(2);
                let rendered = match (ui.caps.images, bytes) {
                    (ImageSupport::None, _) | (_, None) => None,
                    (_, Some(bytes)) => {
                        let rows = crate::tui_image::suggested_rows(bytes, cols.max(1))
                            .unwrap_or(1)
                            .min(pane.height.saturating_sub(4));
                        ui.images
                            .render(&marker.path, Some(bytes), cols, rows, ui.caps.color)
                    }
                };
                match rendered {
                    Some(image) => lines.extend(image.lines),
                    None => lines.push(Line::styled(
                        format!("  [image: {}]", marker.alt),
                        ui.theme.muted,
                    )),
                }
            }
            // The pager scrolls the body; the accept row is pinned to the
            // pane's last interior line (rendering it into the whole pane
            // would overwrite the title border).
            let body_area = Rect {
                height: pane.height.saturating_sub(1),
                ..pane
            };
            if native && !img_slots.is_empty() {
                // Native lane: the text keeps the top (scrollable), every
                // reserved image gets its own slot below it, and the
                // protocol escapes flow through the frame buffer.
                let total_img_rows = img_rows
                    .iter()
                    .sum::<u16>()
                    .min(body_area.height.saturating_sub(4));
                let [text_area, images_area] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(4), Constraint::Length(total_img_rows)])
                    .areas(body_area);
                let paragraph = Paragraph::new(lines)
                    .block(ui.theme.block(&format!("{}{}", texts.license.0, pager)))
                    .scroll((license_scroll, 0));
                frame.render_widget(paragraph, text_area);
                let slots = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        img_rows
                            .iter()
                            .map(|rows| Constraint::Length(*rows))
                            .collect::<Vec<_>>(),
                    )
                    .split(images_area);
                let picker = ui.picker.as_ref().expect("native implies picker");
                for (slot_area, dyn_img) in slots.iter().zip(img_slots.drain(..)) {
                    if let Ok(protocol) =
                        picker.new_protocol(dyn_img, *slot_area, Resize::Fit(None))
                    {
                        frame.render_widget(ratatui_image::Image::new(&protocol), *slot_area);
                    }
                }
            } else {
                let paragraph = Paragraph::new(lines)
                    .block(ui.theme.block(&format!("{}{}", texts.license.0, pager)))
                    .scroll((license_scroll, 0));
                frame.render_widget(paragraph, body_area);
            }
            let agree_row = Rect {
                x: pane.x + 1,
                y: pane.y + pane.height.saturating_sub(1),
                width: pane.width.saturating_sub(2),
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(ui.theme.checkbox(core.state.agreed, texts.license.1)),
                agree_row,
            );
            // Registered AFTER the pane so a click on the row resolves to it
            // (the hit lookup takes the most specific = last region).
            ui.clickable(pane, Hit::LicensePane);
            ui.clickable(agree_row, Hit::LicenseAgree);
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
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(texts.install, ui.theme.title),
                    Span::raw(" "),
                    Span::raw(product_of(&core.state)),
                ]))
                .block(ui.theme.block("")),
                head,
            );
            let gauge = match core.state.progress {
                Some(percent) => Gauge::default()
                    .gauge_style(ui.theme.accent)
                    .percent(u16::from(percent))
                    .label(core.state.flow_step.clone()),
                None => Gauge::default()
                    .gauge_style(ui.theme.accent)
                    .label(core.state.flow_step.clone()),
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
                        shun::wizard::LogKind::Error => ui.theme.error,
                        shun::wizard::LogKind::Ok => ui.theme.ok,
                        shun::wizard::LogKind::Step => ui.theme.title,
                        shun::wizard::LogKind::Echo => ui.theme.muted,
                    };
                    Line::styled(format!("{} {}", line.time, line.text), style)
                })
                .collect();
            frame.render_widget(Paragraph::new(tail).block(ui.theme.block("")), log);
        }
        Step::Done => {
            // A failed FINISH keeps the answers editable (Space cycles, the
            // checkboxes stay clickable) so the retry can change them; an
            // install failure keeps the old terminal semantics.
            let mut lines: Vec<Line<'static>> = Vec::new();
            // (line offset inside the pane's inner area, hit) for the mouse.
            let mut click_rows: Vec<(u16, Hit)> = Vec::new();
            if let Some(err) = &core.state.failure {
                // A FINISH failure gets the stage's own title — the install
                // itself completed, so a bare `install failed` would lie.
                let title = match finish_stage {
                    Some(FinishStage::Shortcuts) => texts.stage_shortcuts,
                    Some(FinishStage::Launch) => texts.stage_launch,
                    _ => texts.done.1,
                };
                lines.push(Line::styled(
                    format!("  {} {}", ui.theme.glyphs.bad, title),
                    ui.theme.error,
                ));
                lines.push(Line::raw(format!("  {err}")));
                if finish_failed {
                    lines.push(Line::raw(""));
                    click_rows.push((lines.len() as u16, Hit::DoneDesktop));
                    lines.push(ui.theme.checkbox(core.state.desktop_shortcut, texts.done.2));
                    click_rows.push((lines.len() as u16, Hit::DoneLaunch));
                    lines.push(ui.theme.checkbox(core.state.launch_after, texts.done.3));
                }
                lines.push(Line::raw(""));
                lines.push(Line::styled(
                    format!(
                        "  {} Enter {} · q/Esc {}",
                        ui.theme.glyphs.arrow, texts.retry, texts.exit
                    ),
                    ui.theme.muted,
                ));
                if finish_failed {
                    lines.push(Line::raw(""));
                    click_rows.push((lines.len() as u16, Hit::DonePrimary));
                    lines.push(Line::styled(
                        format!("  [ {} ]", texts.retry),
                        ui.theme.selected,
                    ));
                }
            } else {
                lines.push(Line::styled(
                    format!("  {} {}", ui.theme.glyphs.ok, texts.done.0),
                    ui.theme.ok,
                ));
                lines.push(
                    ui.theme
                        .kv("", vec![Span::raw(core.state.dir.trim().to_string())]),
                );
                lines.push(Line::raw(""));
                click_rows.push((lines.len() as u16, Hit::DoneDesktop));
                lines.push(ui.theme.checkbox(core.state.desktop_shortcut, texts.done.2));
                click_rows.push((lines.len() as u16, Hit::DoneLaunch));
                lines.push(ui.theme.checkbox(core.state.launch_after, texts.done.3));
                lines.push(Line::raw(""));
                click_rows.push((lines.len() as u16, Hit::DonePrimary));
                lines.push(Line::styled(
                    format!("  [ {} ]", texts.nav.0),
                    ui.theme.selected,
                ));
            }
            frame.render_widget(Paragraph::new(lines).block(ui.theme.block("")), pane);
            for (offset, hit) in click_rows {
                let row = Rect {
                    x: pane.x + 1,
                    y: pane.y + 1 + offset,
                    width: pane.width.saturating_sub(2),
                    height: 1,
                };
                if row.y < pane.y + pane.height {
                    ui.clickable(row, hit);
                }
            }
        }
    }

    // Footer: the key hints plus what the host terminal offers (the probe's
    // own line — also the e2e assertion hook).
    let footer_line = Line::from(vec![
        Span::styled(
            format!(" [{}] ", ui.theme.capability_summary()),
            ui.theme.muted,
        ),
        Span::styled(format!("{} ", ui.theme.glyphs.sep), ui.theme.muted),
        Span::styled(format!("{} ", ui.theme.glyphs.arrow), ui.theme.accent),
        Span::raw(ui.theme.hint(texts.footer)),
    ]);
    frame.render_widget(Paragraph::new(footer_line).style(ui.theme.muted), footer);
}

fn draw_uninstall(frame: &mut Frame, core: &WizardCore, texts: &'static Texts, ui: &mut Ui) {
    set_product(&core.config.product.name);
    let area = frame.area();
    let block = ui.theme.block(&format!(
        " {} — {} ",
        product_of(&core.state),
        texts.uninstall.0
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line> = match core.state.uninstall_phase {
        UninstallPhase::Idle => vec![
            Line::raw(""),
            ui.theme.kv(
                "",
                vec![Span::raw(uninstall_target_dir().unwrap_or_default())],
            ),
            Line::raw(""),
            Line::styled(format!("  [ {} ]", texts.uninstall.0), ui.theme.selected),
            Line::styled(format!("  Esc: {}", texts.exit), ui.theme.muted),
        ],
        UninstallPhase::Running => vec![
            Line::raw(""),
            Line::styled(format!("  {}", texts.uninstall.2), ui.theme.accent),
        ],
        UninstallPhase::Done => vec![
            Line::raw(""),
            Line::styled(
                format!("  {} {}", ui.theme.glyphs.ok, texts.uninstall.3),
                ui.theme.ok,
            ),
            Line::raw(""),
            Line::styled(format!("  Esc: {}", texts.exit), ui.theme.muted),
        ],
        UninstallPhase::Failed => vec![
            Line::raw(""),
            Line::styled(
                format!("  {} {}", ui.theme.glyphs.bad, texts.uninstall.4),
                ui.theme.error,
            ),
            Line::raw(core.state.uninstall_error.clone()),
            Line::raw(""),
            Line::styled(format!("  [ {} ]", texts.retry), ui.theme.selected),
            Line::styled(format!("  Esc: {}", texts.exit), ui.theme.muted),
        ],
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
    if matches!(
        core.state.uninstall_phase,
        UninstallPhase::Idle | UninstallPhase::Failed
    ) {
        let button = Rect {
            x: inner.x,
            y: inner.y + 3,
            width: inner.width,
            height: 1,
        };
        ui.clickable(button, Hit::UninstallPrimary);
    }
}

/// The picker's font size in pixels (cell size for native image slots).
fn picker_font_size(ui: &Ui) -> (u16, u16) {
    ui.picker
        .as_ref()
        .map(|picker| picker.font_size())
        .unwrap_or((8, 16))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The native image lane (kitty protocol) draws unicode placeholder
    /// cells referencing the transmitted image - assertable through a
    /// TestBackend without a real kitty terminal (whose escapes ConPTY
    /// would swallow in the pty harness).
    #[test]
    fn native_kitty_lane_draws_placeholder_cells() {
        let mut picker = Picker::from_fontsize((8, 16));
        picker.set_protocol_type(ProtocolType::Kitty);
        let dyn_img = image::DynamicImage::new_rgb8(24, 16);
        let protocol = picker
            .new_protocol(dyn_img, Rect::new(0, 0, 20, 8), Resize::Fit(None))
            .expect("kitty protocol builds");
        let backend = ratatui::backend::TestBackend::new(40, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(ratatui_image::Image::new(&protocol), frame.area());
            })
            .unwrap();
        // The kitty lane transmits the image as an OSC/APC escape carried in
        // a buffer cell's symbol (the base64 RGBA payload named by f=32).
        let transmitted = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol().starts_with('\u{1b}') && cell.symbol().contains("_G"));
        assert!(transmitted, "kitty transmit escape expected in the buffer");
    }

    /// The halfblocks lane draws upper-half-block glyphs with colored
    /// cells - the universal fallback for color terminals.
    #[test]
    fn halfblocks_lane_draws_block_glyphs() {
        let mut picker = Picker::from_fontsize((8, 16));
        picker.set_protocol_type(ProtocolType::Halfblocks);
        let dyn_img = image::DynamicImage::new_rgb8(24, 16);
        let protocol = picker
            .new_protocol(dyn_img, Rect::new(0, 0, 20, 8), Resize::Fit(None))
            .expect("halfblocks protocol builds");
        let backend = ratatui::backend::TestBackend::new(40, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(ratatui_image::Image::new(&protocol), frame.area());
            })
            .unwrap();
        // The halfblock primitive paints spaces with colored backgrounds
        // (visually identical to glyph rendering) - assert painted cells.
        let painted = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .filter(|cell| cell.bg != ratatui::style::Color::Reset)
            .count();
        assert!(
            painted > 0,
            "colored (painted) cells expected in the buffer"
        );
    }
}
