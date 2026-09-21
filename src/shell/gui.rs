//! Native WinUI 3 shell. The application controller remains the sole business-state owner.
use crate::{
    controller::{AppController, Transport},
    editor::{EditorModel, RULER_HEIGHT},
    models::Options,
    theme::{Palette, Rgb, append_theme_log, theme_log_line},
};
use anyhow::Result;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use windows_canvas::{
    ColorF, DrawContext, Ellipse, GradientStop, Invalidator, ParagraphAlignment,
    Rect as CanvasRect, RoundedRect, TextAlignment, TextFormat, Vector2,
};
use windows_pickers::{FolderPicker, OpenFilePicker, SaveFilePicker};
use windows_reactor::*;

#[path = "gui/controls.rs"]
mod controls;
#[path = "gui/dialogs.rs"]
mod dialogs;
#[path = "gui/library_popup.rs"]
mod library_popup;
#[path = "gui/drawing.rs"]
mod drawing;
#[path = "gui/editor_page.rs"]
mod editor_page;
#[path = "gui/events.rs"]
mod events;
#[path = "gui/import_page.rs"]
mod import_page;
#[path = "gui/keyboard.rs"]
mod keyboard;
#[path = "gui/layout.rs"]
mod layout;
#[path = "gui/native.rs"]
mod native;
#[path = "gui/resources.rs"]
mod resources;
#[cfg(test)]
#[path = "gui/tests.rs"]
mod tests;

use controls::{card, check, icon_button, label, logo, tab_button, table_row, ui_button};
#[cfg(test)]
use drawing::key_label;
use drawing::{Timeline, draw_qr, draw_score, draw_timeline, format_time};
use keyboard::{editor_key, global_key};
use native::{Hwnd, LibraryWatch, WindowSizes, copy_text, install_close_hook, read_client_size};
use resources::{button_resources, dialog_resources, theme_resources};

#[derive(Clone, PartialEq)]
struct LaunchInput {
    open: Option<PathBuf>,
    remote: bool,
}
#[derive(Clone)]
enum Message {
    Tick,
    Open,
    PickedOpen(std::result::Result<Option<PathBuf>, String>),
    Save,
    PickedSave(std::result::Result<Option<PathBuf>, String>),
    SelectLibraryPath(PathBuf),
    LibrarySearch(String),
    LibraryMenu,
    DismissLibrary,
    LibraryFolder,
    SelectTab(bool),
    Refresh,
    SelectPart(Option<usize>),
    Convert,
    Listen,
    Game(bool),
    StopGame,
    Cancel,
    FilesDropped(DroppedData),
    Settings,
    SettingsSave,
    SettingsCancel,
    ResetLibrary,
    JumpMark,
    PitchZoom(bool),
    TimelineDown(PointerEventInfo),
    TimelineMove(PointerEventInfo),
    TimelineUp(PointerEventInfo),
    CopyRemote,
    RemoteAddress(Option<usize>),
    Phone,
    PhoneClosed,
    StopRemote,
    PickLibrary,
    PickedLibrary(std::result::Result<Option<PathBuf>, String>),
    Speed(Option<f64>),
    Transpose(Option<f64>),
    Mode(Option<usize>),
    AutoOctave(bool),
    Trim(bool),
    Phrase(bool),
    SkipRests(bool),
    HighlightStart(bool),
    Compact(bool),
    Theme(Option<usize>),
    Zoom(f64),
    Fit,
    Pan(f64),
    Undo,
    Redo,
    Delete,
    Mark,
    ClearMark,
    Nudge(f64, i32, f64),
    Wheel(PointerEventInfo),
    PointerDown(PointerEventInfo),
    PointerMove(PointerEventInfo),
    PointerUp(PointerEventInfo),
    PointerCancel,
    Close,
    CloseHookResult(std::result::Result<(), String>),
    CanvasError(String),
}
struct Studio {
    controller: Option<AppController>,
    editor: Rc<RefCell<EditorModel>>,
    invalidator: Invalidator,
    score_view: View,
    canvas_theme: Rc<RefCell<Palette>>,
    theme_version: u64,
    // Native WinUI controls keep template-resolved brushes. Bump this when
    // the custom palette changes so their keyed subtree is remounted instead
    // of relying on a ResourceDictionary replacement to repaint it.
    theme_revision: u64,
    committed_theme: String,
    error_sender: Option<LocalSender<Message>>,
    timer: Option<ComponentTimer>,
    options: Options,
    error: String,
    library_menu: bool,
    library_query: String,
    editor_tab: bool,
    settings_draft: Option<crate::preferences::Preferences>,
    timeline: Rc<RefCell<Timeline>>,
    timeline_view: View,
    timeline_invalidator: Invalidator,
    remote_address: usize,
    settings: bool,
    phone: bool,
    seen_document: u64,
    seen_revision: u64,
    library_watch: LibraryWatch,
    close_allowed: Rc<Cell<bool>>,
    native_window: Rc<Cell<Hwnd>>,
    window_sizes: WindowSizes,
    started: Instant,
    smoke_ms: Option<u64>,
    smoke: bool,
    last_pointer: Option<(Instant, f64, f64)>,
    save_version: Option<(u64, u64)>,
    part_rows: RefCell<(u64, usize, Vec<[String; 5]>)>,
}
impl Studio {
    fn perform(&mut self, action: impl FnOnce(&mut AppController) -> Result<()>) {
        if let Some(c) = self.controller.as_mut() {
            if let Err(e) = action(c) {
                self.error = e.to_string();
            } else {
                self.error.clear();
            }
        }
    }
    fn apply_edit(&mut self, result: Result<bool>) {
        match result {
            Ok(true) => {
                let notes = self.editor.borrow().notes.clone();
                self.perform(|c| c.set_notes(notes));
            }
            Ok(false) => {}
            Err(e) => self.error = e.to_string(),
        }
    }
    fn filtered_library(&self) -> Vec<usize> {
        self.controller
            .as_ref()
            .map_or_else(Vec::new, |c| {
                let query = self.library_query.trim().to_lowercase();
                c.library.iter().enumerate().filter(|(_, entry)| {
                    query.split_whitespace().all(|term| entry.title.to_lowercase().contains(term)
                        || entry.file.to_lowercase().contains(term))
                }).map(|(index, _)| index).collect()
            })
    }
    fn rebuild_canvases(&mut self) {
        if let Some(sender) = self.error_sender.clone() {
            self.score_view =
                make_score_view(&self.editor, &self.canvas_theme, &self.invalidator, &sender);
        }
        self.timeline_view = make_timeline_view(
            &self.timeline,
            &self.canvas_theme,
            &self.timeline_invalidator,
        );
    }
    fn apply_canvas_theme(&mut self, next: Palette) -> bool {
        if *self.canvas_theme.borrow() == next {
            return false;
        }
        *self.canvas_theme.borrow_mut() = next;
        self.theme_revision = self.theme_revision.wrapping_add(1);
        self.invalidator.invalidate();
        self.timeline_invalidator.invalidate();
        true
    }
    fn sync(&mut self) {
        {
            let Some(c) = self.controller.as_ref() else {
                return;
            };
            let mut e = self.editor.borrow_mut();
            if self.seen_document != c.state.document_id {
                if let Some(p) = &c.state.project {
                    e.set_document(&p.notes, p.highlight);
                } else {
                    e.set_document(&[], None);
                }
                self.editor_tab = c.state.project.is_some() || c.preferences.compact;
                self.seen_document = c.state.document_id;
                self.seen_revision = c.state.revision;
                self.options = c
                    .state
                    .project
                    .as_ref()
                    .and_then(|p| p.options.as_ref())
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_else(|| {
                        c.default_options()
                            .unwrap_or_else(|_| c.conversion_options())
                    });
            } else if self.seen_revision != c.state.revision && !e.is_dragging() {
                if let Some(p) = &c.state.project {
                    e.sync_notes(&p.notes, p.highlight);
                }
                self.seen_revision = c.state.revision;
            }
        }
        let requested = match self.controller.as_ref() {
            Some(c) => self
                .settings_draft
                .as_ref()
                .unwrap_or(&c.preferences)
                .theme
                .clone(),
            None => return,
        };
        let next_canvas_theme = Palette::for_theme(&requested);
        // Preview and cancel paths both go through the same palette update so
        // native controls and Canvas surfaces receive one shared revision.
        self.apply_canvas_theme(next_canvas_theme);
        {
            let Some(c) = self.controller.as_ref() else {
                return;
            };
            let mut e = self.editor.borrow_mut();
            e.set_compact(c.preferences.compact);
            e.read_only = c.busy() || c.state.transition.is_some();
            e.allow_note_edits = c.capabilities().can_edit;
            if !c.state.show_cursor && e.has_position() {
                e.reset_timeline();
            }
            if c.state.transport == Transport::Playing {
                e.follow_playback_position(c.state.logical_seek);
            } else if !e.is_dragging() {
                e.follow_position(c.state.logical_seek, c.state.show_cursor);
            }
            let mut timeline = self.timeline.borrow_mut();
            if !timeline.dragging {
                timeline.position = c.state.position;
            }
            timeline.duration = if c.state.preview_duration > 0.0 {
                c.state.preview_duration
            } else {
                c.state.score_duration
            };
            timeline.highlight = c
                .state
                .project
                .as_ref()
                .and_then(|p| p.highlight)
                .map(|value| {
                    c.state
                        .result
                        .as_ref()
                        .map_or(value, |r| r.to_audio.map(value))
                });
        }
    }
    fn schedule(&mut self, context: &ComponentContext<Self>) {
        // Sample the monotonic playback clock more often while playing so the visible track
        // advances smoothly; the controller remains the source of truth.
        let delay = if self
            .controller
            .as_ref()
            .is_some_and(|c| c.state.transport == Transport::Playing)
        {
            8
        } else {
            100
        };
        match context.set_timeout(Duration::from_millis(delay), Message::Tick) {
            Ok(t) => self.timer = Some(t),
            Err(e) => self.error = e.to_string(),
        }
    }
    fn save_picker(&mut self, context: &ComponentContext<Self>) {
        if let Some(c) = &self.controller {
            if !c.capabilities().can_save {
                return;
            }
            self.save_version = Some((c.state.document_id, c.state.revision));
        }
        let name = self
            .controller
            .as_ref()
            .and_then(|c| c.state.project.as_ref())
            .map_or("曲谱".to_owned(), |p| p.title.clone());
        if !SaveFilePicker::new()
            .title("另存口琴工程")
            .filter_extensions("口琴工坊工程", ["hstudio"])
            .suggested_name(name)
            .default_extension("hstudio")
            .overwrite_prompt(true)
            .request(context, |r| {
                Message::PickedSave(r.map_err(|e| e.to_string()))
            })
        {
            self.error = "请先关闭当前文件对话框。".into();
        }
    }
}
fn make_score_view(
    editor: &Rc<RefCell<EditorModel>>,
    canvas_theme: &Rc<RefCell<Palette>>,
    invalidator: &Invalidator,
    errors: &LocalSender<Message>,
) -> View {
    let draw_editor = Rc::clone(editor);
    let draw_theme = Rc::clone(canvas_theme);
    let canvas_errors = errors.clone();
    windows_canvas::Canvas::invalidated(invalidator, move |ctx| {
        let mut model = draw_editor.borrow_mut();
        model.resize(ctx.width as f64, ctx.height as f64);
        draw_score(ctx, &model, &draw_theme.borrow())
    })
    .on_error(move |e| {
        let _ = canvas_errors.send(Message::CanvasError(format!("曲谱绘制失败：{e:?}")));
    })
    .into()
}
fn make_timeline_view(
    timeline: &Rc<RefCell<Timeline>>,
    canvas_theme: &Rc<RefCell<Palette>>,
    invalidator: &Invalidator,
) -> View {
    let draw_timeline_state = timeline.clone();
    let timeline_theme = Rc::clone(canvas_theme);
    windows_canvas::Canvas::invalidated(invalidator, move |ctx| {
        let mut t = draw_timeline_state.borrow_mut();
        t.width = ctx.width as f64;
        draw_timeline(ctx, &t, &timeline_theme.borrow())
    })
    .into()
}
impl Component for Studio {
    type Input = LaunchInput;
    type Message = Message;
    fn create(input: &LaunchInput, context: &ComponentContext<Self>) -> Self {
        let initialized = if std::env::var_os("HARMONICA_STUDIO_SMOKE_MS").is_some() {
            AppController::silent(crate::paths::data_root())
        } else {
            AppController::new(None)
        };
        let (controller, error) = match initialized {
            Ok(c) => (Some(c), String::new()),
            Err(e) => (None, e.to_string()),
        };
        let smoke_ms = std::env::var("HARMONICA_STUDIO_SMOKE_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        let editor = Rc::new(RefCell::new(EditorModel::default()));
        let invalidator = Invalidator::new();
        let canvas_theme = Rc::new(RefCell::new(Palette::for_theme("paper")));
        let error_sender = context.sender();
        let score_view = make_score_view(&editor, &canvas_theme, &invalidator, &error_sender);
        let timeline = Rc::new(RefCell::new(Timeline::default()));
        let timeline_invalidator = Invalidator::new();
        let timeline_view = make_timeline_view(&timeline, &canvas_theme, &timeline_invalidator);
        let options = controller
            .as_ref()
            .map_or_else(Options::default, |c| c.conversion_options());
        let committed_theme = controller
            .as_ref()
            .map(|c| c.preferences.theme.clone())
            .unwrap_or_else(|| "paper".into());
        let mut app = Self {
            controller,
            editor,
            invalidator,
            score_view,
            canvas_theme,
            theme_version: 0,
            theme_revision: 0,
            committed_theme,
            error_sender: Some(error_sender),
            timer: None,
            options,
            error,
            library_menu: false,
            library_query: String::new(),
            editor_tab: false,
            settings_draft: None,
            timeline,
            timeline_view,
            timeline_invalidator,
            remote_address: 0,
            settings: smoke_ms.is_some()
                && std::env::var("HARMONICA_STUDIO_SMOKE_VIEW").as_deref() == Ok("settings"),
            phone: input.remote,
            seen_document: u64::MAX,
            seen_revision: u64::MAX,
            library_watch: LibraryWatch::default(),
            close_allowed: Rc::new(Cell::new(false)),
            native_window: Rc::new(Cell::new(std::ptr::null_mut())),
            window_sizes: WindowSizes::default(),
            started: Instant::now(),
            smoke_ms,
            smoke: smoke_ms.is_some(),
            last_pointer: None,
            save_version: None,
            part_rows: RefCell::new((u64::MAX, 0, Vec::new())),
        };
        if app.settings {
            app.settings_draft = app.controller.as_ref().map(|c| c.preferences.clone());
        }
        app.perform(|c| c.refresh_library());
        let open = input
            .open
            .clone()
            .or_else(|| std::env::var_os("HARMONICA_STUDIO_SMOKE_FILE").map(PathBuf::from));
        if let Some(path) = open {
            app.perform(|c| open_document(c, &path, None));
        }
        if input.remote {
            let isolated = app.smoke_ms.is_some();
            app.perform(|c| {
                if isolated { c.start_remote_local(0) } else { c.start_remote(47638) }.map(|_| ())
            });
        }
        let sender = context.sender();
        let allowed = Rc::clone(&app.close_allowed);
        let native_window = Rc::clone(&app.native_window);
        let _ = context.run_window(move |window| {
            native_window.set(window.as_raw());
            Message::CloseHookResult(install_close_hook(window.as_raw(), sender, allowed))
        });
        app.sync();
        app.schedule(context);
        app
    }
    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        self.handle_message(message, context);
    }
    fn view(&self, input: &LaunchInput, context: &mut ViewContext<Self>) -> View {
        self.shell_view(input, context)
    }
}
fn open_document(
    c: &mut AppController,
    path: &std::path::Path,
    options: Option<Options>,
) -> Result<()> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("hstudio"))
    {
        c.open_path(path)
    } else {
        c.load_file(path, options, c.preferences.compact, false)
    }
}
pub fn run(open: Option<PathBuf>, open_remote: bool) -> Result<()> {
    App::run_component::<Studio>(LaunchInput {
        open,
        remote: open_remote,
    })?;
    Ok(())
}
