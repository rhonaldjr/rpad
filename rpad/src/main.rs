use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use gtk::glib;

use gtk4 as gtk;

use sourceview5 as sv;
use sourceview5::prelude::*;

use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "rpad", version, about = "rpad – A simple Rust notepad")]
struct Args {
    /// Optional file to open
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Editing mode: plain, markup, rich
    #[arg(long, value_enum, default_value_t = ModeArg::Plain)]
    mode: ModeArg,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ModeArg {
    Plain,
    Markup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Plain,
    Markup,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Plain => Mode::Plain,
            ModeArg::Markup => Mode::Markup,
        }
    }
}

// Simple app state for now: mode + optional file path
#[derive(Debug, Clone)]
struct AppConfig {
    mode: Mode,
    file: Option<PathBuf>,
}

#[derive(Debug)]
struct DocumentState {
    path: RefCell<Option<PathBuf>>,
    mode: RefCell<Mode>, // 🔹 NEW
    undo_stack: RefCell<Vec<String>>,
    redo_stack: RefCell<Vec<String>>,
    last_text: RefCell<String>,
    is_programmatic: RefCell<bool>,
    dirty: RefCell<bool>,
    find_text: RefCell<String>,
    match_case: RefCell<bool>,
    zoom: RefCell<u32>,
    css_provider: gtk::CssProvider,
    label_line_col: gtk::Label,
    label_words_chars: gtk::Label,
    label_mode: gtk::Label,
    label_sudo: gtk::Label,
    status_box: gtk::Box,

    // Sudo Mode
    sudo_password: RefCell<Option<String>>,
    sudo_expiry: RefCell<Option<std::time::Instant>>,
}

impl DocumentState {
    fn new(initial: Option<PathBuf>, initial_mode: Mode) -> Self {
        // 🔹 CHANGED
        Self {
            path: RefCell::new(initial),
            mode: RefCell::new(initial_mode), // 🔹 NEW
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            last_text: RefCell::new(String::new()),
            is_programmatic: RefCell::new(false),
            dirty: RefCell::new(false),
            find_text: RefCell::new(String::new()),
            match_case: RefCell::new(false),
            zoom: RefCell::new(100),
            css_provider: gtk::CssProvider::new(),
            label_line_col: gtk::Label::new(Some("Ln 1, Col 1")),
            label_words_chars: gtk::Label::new(Some("0 words, 0 chars")),
            label_mode: gtk::Label::new(Some(match initial_mode {
                Mode::Plain => "Plain Text",
                Mode::Markup => "Markdown",
            })),
            label_sudo: {
                let l = gtk::Label::new(Some("SUDO"));
                // Style it: bold red
                let attr_list = gtk::pango::AttrList::new();
                attr_list.insert(gtk::pango::AttrColor::new_foreground(65535, 0, 0)); // Red
                attr_list.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
                l.set_attributes(Some(&attr_list));
                l.set_visible(false); // Hidden by default
                l
            },
            status_box: gtk::Box::new(gtk::Orientation::Horizontal, 12),
            sudo_password: RefCell::new(None),
            sudo_expiry: RefCell::new(None),
        }
    }

    fn set_path(&self, new_path: Option<PathBuf>) {
        *self.path.borrow_mut() = new_path;
    }

    fn path(&self) -> Option<PathBuf> {
        self.path.borrow().clone()
    }

    fn mode(&self) -> Mode {
        // 🔹 NEW
        *self.mode.borrow()
    }

    fn set_mode(&self, value: Mode) {
        // 🔹 NEW
        *self.mode.borrow_mut() = value;
    }

    fn set_dirty(&self, value: bool) {
        *self.dirty.borrow_mut() = value;
    }

    fn is_dirty(&self) -> bool {
        *self.dirty.borrow()
    }
}

fn main() {
    // 1. Parse CLI args
    let args = Args::parse();
    let initial_mode: Mode = args.mode.into();

    let config = AppConfig {
        mode: initial_mode,
        file: args.file,
    };

    // 2. Create GTK application
    let app = gtk::Application::builder()
        .application_id("dev.rpad.app")
        .build();

    // 3. Pass config into the activate handler (clone into closure)
    let config_clone = config.clone();
    app.connect_activate(move |app| {
        build_ui(app, config_clone.clone());
    });

    // 4. Run
    app.run();
}

fn build_ui(app: &gtk::Application, config: AppConfig) {
    // Window
    let title = match &config.file {
        Some(path) => format!("rpad - {}", path.display()),
        None => "rpad - Untitled".to_string(),
    };

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(&title)
        .default_width(900)
        .default_height(700)
        .build();

    // Register custom icon
    if let Some(display) = gtk::gdk::Display::default() {
        let icon_theme = gtk::IconTheme::for_display(&display);
        if let Ok(cwd) = std::env::current_dir() {
            icon_theme.add_search_path(cwd.join("assets"));
        } else {
            // Fallback if current_dir fails?
            icon_theme.add_search_path("assets");
        }
    }
    window.set_icon_name(Some("rpad_icon"));

    // Track current file path + mode in window data
    let doc_state = DocumentState::new(config.file.clone(), config.mode);
    unsafe {
        window.set_data("rpad-doc-state", doc_state);
    }

    // Main text area using GtkSourceView5
    let buffer = sv::Buffer::new(None); // no language yet
    let text_view = sv::View::with_buffer(&buffer);

    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    apply_language_for_mode(&buffer, config.mode);

    // Apply initial zoom
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            text_view.style_context().add_provider(
                &doc_state.css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            update_zoom_css(doc_state);
        }
    }

    // Store the editor view on the window so helpers can find its buffer
    unsafe {
        window.set_data("rpad-text-view", text_view.clone());
    }

    // Padding inside the editor
    text_view.set_left_margin(12);
    text_view.set_right_margin(12);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);

    // Track edits for undo/redo *and* dirty flag
    {
        let window_clone_1 = window.clone();
        let window_clone_2 = window.clone();
        buffer.connect_changed(move |buf| unsafe {
            if let Some(doc_state_ptr) = window_clone_1.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();

                if *doc_state.is_programmatic.borrow() {
                    return;
                }

                let (start, end) = buf.bounds();
                let text = buf.text(&start, &end, false).to_string();

                let mut last_text = doc_state.last_text.borrow_mut();
                if text != *last_text {
                    doc_state.undo_stack.borrow_mut().push(last_text.clone());
                    doc_state.redo_stack.borrow_mut().clear();
                    *last_text = text;
                    doc_state.set_dirty(true);
                    update_counts(doc_state, buf.upcast_ref());
                }
            }
        });

        // 2) Track cursor movement for Line/Col
        buffer.connect_mark_set(move |buf, _iter, mark| {
            unsafe {
                if let Some(doc_state_ptr) = window_clone_2.data::<DocumentState>("rpad-doc-state")
                {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();
                    // Only update if "insert" mark moved
                    if mark.name().as_deref() == Some("insert") {
                        update_cursor(doc_state, buf.upcast_ref());
                    }
                }
            }
        });
    }

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .hexpand(true)
        .vexpand(true)
        .build();

    scrolled.set_margin_top(4);
    scrolled.set_margin_bottom(4);
    scrolled.set_margin_start(4);
    scrolled.set_margin_end(4);

    // Menu bar
    let menubar = build_menubar();

    // Main container (vertical: menubar on top, editor below, status bar bottom)
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.append(&menubar);
    vbox.append(&scrolled);

    // Status Bar (retrieved from State)
    if let Some(doc_state_ptr) = unsafe { window.data::<DocumentState>("rpad-doc-state") } {
        unsafe {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let status_box = &doc_state.status_box;

            status_box.set_margin_start(6);
            status_box.set_margin_end(6);
            status_box.set_margin_top(2);
            status_box.set_margin_bottom(2);

            // Add items to status box
            status_box.append(&doc_state.label_sudo);
            status_box.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            status_box.append(&doc_state.label_mode);
            status_box.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            status_box.append(&doc_state.label_line_col);
            status_box.append(&gtk::Box::new(gtk::Orientation::Horizontal, 0)); // spacer

            // Push words/chars to the right
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.set_hexpand(true);
            status_box.append(&spacer);

            status_box.append(&doc_state.label_words_chars);

            vbox.append(status_box);
        }
    }

    window.set_child(Some(&vbox));

    // Ask for confirmation when closing if there are unsaved changes
    {
        let _window_clone = window.clone();
        window.connect_close_request(move |win| {
            unsafe {
                if let Some(doc_state_ptr) = win.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();

                    // If not dirty, allow normal close
                    if !doc_state.is_dirty() {
                        return glib::Propagation::Proceed;
                    }

                    // Document is dirty → prompt
                    let win_for_dialog = win.clone();

                    let dialog = gtk::MessageDialog::builder()
                        .transient_for(&win_for_dialog)
                        .modal(true)
                        .message_type(gtk::MessageType::Question)
                        .buttons(gtk::ButtonsType::None)
                        .text("Do you want to save changes to this document before closing?")
                        .secondary_text("If you don’t save, your changes will be lost.")
                        .build();

                    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
                    dialog.add_button("Don't Save", gtk::ResponseType::Reject);
                    dialog.add_button("Save", gtk::ResponseType::Accept);

                    dialog.connect_response(move |dialog, response| {
                        match response {
                            gtk::ResponseType::Accept => {
                                // Save then close
                                if let Some(doc_state_ptr) =
                                    win_for_dialog.data::<DocumentState>("rpad-doc-state")
                                {
                                    let doc_state: &DocumentState = doc_state_ptr.as_ref();

                                    if let Some(path) = doc_state.path() {
                                        // Save to existing path
                                        if let Err(err) =
                                            save_buffer_to_path(&win_for_dialog, &path)
                                        {
                                            eprintln!("Error saving file: {err}");
                                        } else {
                                            win_for_dialog.close();
                                        }
                                    } else {
                                        // No path yet → Save As + close
                                        save_as_with_dialog_and_then_close(&win_for_dialog);
                                    }
                                }
                            }
                            gtk::ResponseType::Reject => {
                                // Don't save: mark as clean so close_request won't re-prompt
                                if let Some(doc_state_ptr) =
                                    win_for_dialog.data::<DocumentState>("rpad-doc-state")
                                {
                                    let doc_state: &DocumentState = doc_state_ptr.as_ref();
                                    doc_state.set_dirty(false);
                                }

                                win_for_dialog.close();
                            }
                            _ => {
                                // Cancel → do nothing, keep window open
                            }
                        }

                        dialog.close();
                    });

                    dialog.show();

                    // We handled the event asynchronously; prevent immediate close
                    glib::Propagation::Stop
                } else {
                    // No doc state → just close
                    glib::Propagation::Proceed
                }
            }
        });
    }

    // If a file was passed via CLI, load it now
    if let Some(ref path) = config.file {
        if let Err(err) = load_file_into_window(&window, path) {
            eprintln!("Error loading file {}: {err}", path.display());
        }
    }

    // Register actions
    register_actions(app, &window, &text_view);

    window.present();
}

fn build_menubar() -> gtk::PopoverMenuBar {
    use gtk::gio;

    // Top-level menu model
    let root = gio::Menu::new();

    // ----- File menu -----
    let file_menu = gio::Menu::new();
    file_menu.append(Some("New"), Some("app.new"));
    file_menu.append(Some("New Window"), Some("app.new_window"));
    file_menu.append(Some("Open…"), Some("app.open"));
    file_menu.append(Some("Save"), Some("app.save"));
    file_menu.append(Some("Save As…"), Some("app.save_as"));
    file_menu.append(Some("Print…"), Some("app.print"));
    file_menu.append(Some("Exit"), Some("app.quit"));
    root.append_submenu(Some("File"), &file_menu);

    // ----- Edit menu -----
    let edit_menu = gio::Menu::new();

    //
    // Group 1: Undo / Redo
    //
    let group1 = gio::Menu::new();
    group1.append(Some("Undo"), Some("app.undo"));
    group1.append(Some("Redo"), Some("app.redo"));
    edit_menu.append_section(None, &group1);

    //
    // Group 2: Cut / Copy / Paste / Delete
    //
    let group2 = gio::Menu::new();
    group2.append(Some("Cut"), Some("app.cut"));
    group2.append(Some("Copy"), Some("app.copy"));
    group2.append(Some("Paste"), Some("app.paste"));
    group2.append(Some("Delete"), Some("app.delete"));
    edit_menu.append_section(None, &group2);

    //
    // Group 3: Find / Find Next / Find Previous / Replace / Go To
    //
    let group3 = gio::Menu::new();
    group3.append(Some("Find…"), Some("app.find"));
    group3.append(Some("Find Next"), Some("app.find_next"));
    group3.append(Some("Find Previous"), Some("app.find_prev"));
    group3.append(Some("Replace…"), Some("app.replace"));
    group3.append(Some("Go To…"), Some("app.goto"));
    edit_menu.append_section(None, &group3);

    //
    // Group 4: Select All / Time/Date
    //
    let group4 = gio::Menu::new();
    group4.append(Some("Select All"), Some("app.select_all"));
    group4.append(Some("Time/Date"), Some("app.time_date"));
    edit_menu.append_section(None, &group4);

    root.append_submenu(Some("Edit"), &edit_menu);

    // ----- View menu -----
    let view_menu = gio::Menu::new();

    let zoom_menu = gio::Menu::new();
    zoom_menu.append(Some("Zoom In"), Some("app.zoom_in"));
    zoom_menu.append(Some("Zoom Out"), Some("app.zoom_out"));
    zoom_menu.append(Some("Restore Default Zoom"), Some("app.zoom_reset"));

    view_menu.append_submenu(Some("Zoom"), &zoom_menu);
    view_menu.append(Some("Status Bar"), Some("app.status_bar"));
    root.append_submenu(Some("View"), &view_menu);

    // ----- Mode menu (your custom feature) -----
    let mode_menu = gio::Menu::new();
    mode_menu.append(Some("Plain Text"), Some("app.mode('plain')"));
    mode_menu.append(Some("Markup"), Some("app.mode('markup')"));
    mode_menu.append(Some("Sudo Mode"), Some("app.sudo_mode"));
    root.append_submenu(Some("Mode"), &mode_menu);

    // ----- Help menu -----
    let help_menu = gio::Menu::new();
    help_menu.append(Some("About rpad"), Some("app.about"));
    root.append_submenu(Some("Help"), &help_menu);

    gtk::PopoverMenuBar::from_model(Some(&root))
}

fn get_text_buffer_from_window(window: &gtk::ApplicationWindow) -> Option<gtk::TextBuffer> {
    unsafe {
        if let Some(view_ptr) = window.data::<sv::View>("rpad-text-view") {
            let view: &sv::View = view_ptr.as_ref();
            return Some(view.buffer().upcast::<gtk::TextBuffer>());
        }
    }
    None
}

fn buffer_is_empty<P: IsA<gtk::TextBuffer>>(buffer: &P) -> bool {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, true).is_empty()
}

fn save_buffer_to_path(
    window: &gtk::ApplicationWindow,
    path: &std::path::Path,
) -> Result<(), String> {
    let buffer = get_text_buffer_from_window(window)
        .ok_or_else(|| "Could not find text buffer".to_string())?;
    let (start, end) = buffer.bounds();
    let text = buffer.text(&start, &end, false);

    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();

            // Check Sudo Mode
            let mut use_sudo = false;
            let mut sudo_pass = None;

            if let Some(pass) = doc_state.sudo_password.borrow().clone() {
                // Check expiry
                let expired = if let Some(expiry) = *doc_state.sudo_expiry.borrow() {
                    std::time::Instant::now() > expiry
                } else {
                    true
                };

                if expired {
                    // Re-prompt
                    if let Some(new_pass) = prompt_for_password(window) {
                        if validate_sudo_password(&new_pass) {
                            *doc_state.sudo_password.borrow_mut() = Some(new_pass.clone());
                            *doc_state.sudo_expiry.borrow_mut() = Some(
                                std::time::Instant::now() + std::time::Duration::from_secs(300),
                            );
                            use_sudo = true;
                            sudo_pass = Some(new_pass);
                        } else {
                            return Err("Sudo re-authentication failed".to_string());
                        }
                    } else {
                        return Err("Sudo re-authentication cancelled".to_string());
                    }
                } else {
                    use_sudo = true;
                    sudo_pass = Some(pass);
                }
            }

            if use_sudo {
                if let Some(pass) = sudo_pass {
                    perform_sudo_save(path, &text, &pass)?;
                } else {
                    return Err("Sudo password missing logic error".to_string());
                }
            } else {
                // Normal Save
                if let Err(e) = fs::write(path, &text) {
                    return Err(format!("Failed to write file: {}", e));
                }
            }

            // Mark as not dirty only on success
            doc_state.set_dirty(false);
            *doc_state.last_text.borrow_mut() = text.to_string();

            // Reset title (preserving [SUDO] tag if active)
            let base_title = format!("rpad - {}", path.display());
            let suffix = if doc_state.sudo_password.borrow().is_some() {
                " [SUDO]"
            } else {
                ""
            };

            // Also append mode
            let mode_suffix = match doc_state.mode() {
                Mode::Plain => " [Plain]",
                Mode::Markup => " [Markdown]",
            };

            window.set_title(Some(&format!("{}{}{}", base_title, suffix, mode_suffix)));

            return Ok(());
        }
    }

    // Fallback if doc_state missing (shouldn't happen)
    if let Err(e) = fs::write(path, text.as_str()) {
        Err(format!("Failed to write file: {}", e))
    } else {
        Ok(())
    }
}

fn load_file_into_window(
    window: &gtk::ApplicationWindow,
    path: &Path,
) -> Result<(), std::io::Error> {
    let contents = fs::read_to_string(path)?;

    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();

            *doc_state.is_programmatic.borrow_mut() = true;

            if let Some(buffer) = get_text_buffer_from_window(window) {
                buffer.set_text(&contents);
            }

            // reset undo/redo and last_text for this new file
            doc_state.undo_stack.borrow_mut().clear();
            doc_state.redo_stack.borrow_mut().clear();
            *doc_state.last_text.borrow_mut() = contents.clone();

            doc_state.set_path(Some(path.to_path_buf()));
            doc_state.set_dirty(false);
            *doc_state.last_text.borrow_mut() = contents.clone();

            // Reset Sudo
            *doc_state.sudo_password.borrow_mut() = None;
            *doc_state.sudo_expiry.borrow_mut() = None;

            // Update UI state
            set_sudo_state(window, false);

            window.set_title(Some(&format!("rpad - {}", path.display())));

            *doc_state.is_programmatic.borrow_mut() = false;
        }
    }

    Ok(())
}

fn open_with_dialog(window: &gtk::ApplicationWindow) {
    use gtk::{FileChooserAction, FileFilter, ResponseType};

    let dialog = gtk::FileChooserDialog::new(
        Some("Open File"),
        Some(window),
        FileChooserAction::Open,
        &[
            ("_Cancel", ResponseType::Cancel),
            ("_Open", ResponseType::Accept),
        ],
    );

    let text_filter = FileFilter::new();
    text_filter.set_name(Some("Text Files (*.txt, *.md, *.markdown)"));
    text_filter.add_pattern("*.txt");
    text_filter.add_pattern("*.md");
    text_filter.add_pattern("*.markdown");
    dialog.add_filter(&text_filter);

    let all_filter = FileFilter::new();
    all_filter.set_name(Some("All Files"));
    all_filter.add_pattern("*");
    dialog.add_filter(&all_filter);

    let window_clone = window.clone();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(file) = dialog.file() {
                if let Some(path) = file.path() {
                    if let Err(err) = load_file_into_window(&window_clone, path.as_ref()) {
                        eprintln!("Error opening file: {err}");
                    }
                }
            }
        }

        dialog.close();
    });

    dialog.show();
}

fn register_actions(app: &gtk::Application, window: &gtk::ApplicationWindow, text_view: &sv::View) {
    use gtk::gio::SimpleAction;

    // ----- File actions -----

    // Quit / Exit: go through window.close() so dirty-check runs
    let quit = SimpleAction::new("quit", None);
    let window_for_quit = window.clone();
    quit.connect_activate(move |_, _| {
        window_for_quit.close();
    });
    app.add_action(&quit);

    // New (clear current document)
    let new_doc = SimpleAction::new("new", None);
    let window_clone = window.clone();
    new_doc.connect_activate(move |_, _| unsafe {
        if let Some(text_buffer) = get_text_buffer_from_window(&window_clone) {
            text_buffer.set_text("");

            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();
                doc_state.set_path(None);
                doc_state.set_dirty(false);
                *doc_state.last_text.borrow_mut() = String::new();
                // Reset Sudo
                *doc_state.sudo_password.borrow_mut() = None;
                *doc_state.sudo_expiry.borrow_mut() = None;

                // Update UI state
                set_sudo_state(&window_clone, false);

                window_clone.set_title(Some("rpad - Untitled"));

                // Also clear undo/redo stacks
                doc_state.undo_stack.borrow_mut().clear();
                doc_state.redo_stack.borrow_mut().clear();
                *doc_state.is_programmatic.borrow_mut() = false;
            }
        }
    });
    app.add_action(&new_doc);

    // New Window – spawn a new rpad process
    let new_window = SimpleAction::new("new_window", None);
    new_window.connect_activate(|_, _| {
        // Try to get the current executable path
        match std::env::current_exe() {
            Ok(exe_path) => {
                if let Err(err) = Command::new(exe_path).spawn() {
                    eprintln!("Failed to open new window: {err}");
                }
            }
            Err(err) => {
                eprintln!("Could not determine current executable for New Window: {err}");
            }
        }
    });
    app.add_action(&new_window);

    // Save
    let save = SimpleAction::new("save", None);
    let window_clone = window.clone();
    save.connect_activate(move |_, _| {
        unsafe {
            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();
                if let Some(path) = doc_state.path() {
                    if let Err(err) = save_buffer_to_path(&window_clone, &path) {
                        eprintln!("Error saving file: {err}");
                    }
                } else {
                    // No path yet → behave like "Save As"
                    save_as_with_dialog(&window_clone);
                }
            } else {
                // No state stored? Fallback to "Save As"
                save_as_with_dialog(&window_clone);
            }
        }
    });
    app.add_action(&save);

    // Save As…
    let save_as = SimpleAction::new("save_as", None);
    let window_clone = window.clone();
    save_as.connect_activate(move |_, _| {
        save_as_with_dialog(&window_clone);
    });
    app.add_action(&save_as);

    // Open
    let open = SimpleAction::new("open", None);
    let window_clone = window.clone();
    open.connect_activate(move |_, _| {
        open_with_dialog(&window_clone);
    });
    app.add_action(&open);

    // Print
    let print = SimpleAction::new("print", None);
    {
        let window_clone = window.clone();
        let text_view_clone = text_view.clone();
        print.connect_activate(move |_, _| {
            let op = gtk::PrintOperation::new();
            op.set_job_name("rpad-print-job");

            let compositor = sv::PrintCompositor::from_view(&text_view_clone);

            let compositor_clone = compositor.clone();
            op.connect_begin_print(move |op, context| {
                let compositor = compositor_clone.clone();
                while !compositor.paginate(context) {
                    // spin loop or rely on internal iterations?
                    // Documentation suggests paginate() does a chunk of work.
                    // Usually we need to keep calling it until TRUE.
                }
                op.set_n_pages(compositor.n_pages());
            });

            let compositor_clone = compositor.clone();
            op.connect_draw_page(move |_op, context, page_nr| {
                compositor_clone.draw_page(context, page_nr);
            });

            let res = op.run(gtk::PrintOperationAction::PrintDialog, Some(&window_clone));
            if let Err(e) = res {
                eprintln!("Error printing: {}", e);
            }
        });
    }
    app.add_action(&print);

    // ----- Edit actions (stubs) -----
    // Undo
    let undo = SimpleAction::new("undo", None);
    let window_clone = window.clone();
    undo.connect_activate(move |_, _| {
        unsafe {
            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();

                let mut undo_stack = doc_state.undo_stack.borrow_mut();
                if let Some(prev_text) = undo_stack.pop() {
                    let current_text = doc_state.last_text.borrow().clone();

                    // Push current text to redo stack
                    doc_state.redo_stack.borrow_mut().push(current_text.clone());

                    // Apply previous text without recording as a new undo entry
                    *doc_state.is_programmatic.borrow_mut() = true;
                    if let Some(buffer) = get_text_buffer_from_window(&window_clone) {
                        buffer.set_text(&prev_text);
                    }
                    *doc_state.last_text.borrow_mut() = prev_text;
                    *doc_state.is_programmatic.borrow_mut() = false;
                }
            }
        }
    });
    app.add_action(&undo);

    // Redo
    let redo = SimpleAction::new("redo", None);
    let window_clone = window.clone();
    redo.connect_activate(move |_, _| {
        unsafe {
            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();

                let mut redo_stack = doc_state.redo_stack.borrow_mut();
                if let Some(next_text) = redo_stack.pop() {
                    let current_text = doc_state.last_text.borrow().clone();

                    // Push current text back to undo stack
                    doc_state.undo_stack.borrow_mut().push(current_text.clone());

                    // Apply next text without recording as a new undo entry
                    *doc_state.is_programmatic.borrow_mut() = true;
                    if let Some(buffer) = get_text_buffer_from_window(&window_clone) {
                        buffer.set_text(&next_text);
                    }
                    *doc_state.last_text.borrow_mut() = next_text;
                    *doc_state.is_programmatic.borrow_mut() = false;
                }
            }
        }
    });
    app.add_action(&redo);

    // ----- Clipboard actions -----
    // CUT
    let cut = SimpleAction::new("cut", None);
    {
        let text_view = text_view.clone();
        cut.connect_activate(move |_, _| {
            text_view.emit_by_name::<()>("cut-clipboard", &[]);
        });
    }
    app.add_action(&cut);

    // COPY
    let copy = SimpleAction::new("copy", None);
    {
        let text_view = text_view.clone();
        copy.connect_activate(move |_, _| {
            text_view.emit_by_name::<()>("copy-clipboard", &[]);
        });
    }
    app.add_action(&copy);

    // PASTE
    let paste = SimpleAction::new("paste", None);
    {
        let text_view = text_view.clone();
        paste.connect_activate(move |_, _| {
            text_view.emit_by_name::<()>("paste-clipboard", &[]);
        });
    }
    app.add_action(&paste);

    // DELETE selection
    let delete = SimpleAction::new("delete", None);
    {
        let text_view = text_view.clone();
        delete.connect_activate(move |_, _| {
            let buffer = text_view.buffer();
            buffer.delete_selection(true, true);
        });
    }
    app.add_action(&delete);

    // Keyboard shortcuts for these
    app.set_accels_for_action("app.undo", &["<Primary>z"]);
    app.set_accels_for_action("app.redo", &["<Primary>y"]);
    app.set_accels_for_action("app.cut", &["<Primary>X"]);
    app.set_accels_for_action("app.copy", &["<Primary>C"]);
    app.set_accels_for_action("app.paste", &["<Primary>V"]);
    app.set_accels_for_action("app.delete", &["Delete"]);

    // File shortcuts
    app.set_accels_for_action("app.new", &["<Primary>n"]);
    app.set_accels_for_action("app.save", &["<Primary>s"]);
    app.set_accels_for_action("app.open", &["<Primary>o"]);
    app.set_accels_for_action("app.save_as", &["<Primary><Shift>s"]);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);

    // ----- Find / Replace / Go To -----
    // Find…
    let find = SimpleAction::new("find", None);
    {
        let window_clone = window.clone();
        let text_view = text_view.clone();
        find.connect_activate(move |_, _| {
            open_find_dialog(&window_clone, &text_view);
        });
    }
    app.add_action(&find);

    // Find Next
    let find_next = SimpleAction::new("find_next", None);
    {
        let window_clone = window.clone();
        let text_view = text_view.clone();
        find_next.connect_activate(move |_, _| {
            do_find_next(&window_clone, &text_view);
        });
    }
    app.add_action(&find_next);

    // Find Previous
    let find_prev = SimpleAction::new("find_prev", None);
    {
        let window_clone = window.clone();
        let text_view = text_view.clone();
        find_prev.connect_activate(move |_, _| {
            do_find_prev(&window_clone, &text_view);
        });
    }
    app.add_action(&find_prev);

    // Replace…
    let replace = SimpleAction::new("replace", None);
    {
        let window_clone = window.clone();
        let text_view = text_view.clone();
        replace.connect_activate(move |_, _| {
            open_replace_dialog(&window_clone, &text_view);
        });
    }
    app.add_action(&replace);

    // Go To…
    let goto = SimpleAction::new("goto", None);
    {
        let window_clone = window.clone();
        let text_view = text_view.clone();
        goto.connect_activate(move |_, _| {
            open_goto_dialog(&window_clone, &text_view);
        });
    }
    app.add_action(&goto);

    app.set_accels_for_action("app.find", &["<Primary>F"]);
    app.set_accels_for_action("app.find_next", &["F3"]);
    app.set_accels_for_action("app.find_prev", &["<Shift>F3"]);
    app.set_accels_for_action("app.replace", &["<Primary>H"]);
    app.set_accels_for_action("app.goto", &["<Primary>G"]);

    // Select All
    let select_all = SimpleAction::new("select_all", None);
    {
        let text_view = text_view.clone();
        select_all.connect_activate(move |_, _| {
            let buffer = text_view.buffer();
            let (start, end) = buffer.bounds();
            buffer.select_range(&start, &end);
        });
    }
    app.add_action(&select_all);

    // Time/Date (insert at cursor, like Notepad's F5)
    let time_date = SimpleAction::new("time_date", None);
    {
        let text_view = text_view.clone();
        time_date.connect_activate(move |_, _| {
            let buffer = text_view.buffer();

            // now_local -> Result<DateTime, BoolError>
            if let Ok(now) = glib::DateTime::now_local() {
                // format -> Result<GString, BoolError>
                if let Ok(stamp) = now.format("%Y-%m-%d %H:%M") {
                    // GString derefs to &str, so this is fine
                    buffer.insert_at_cursor(&stamp);
                } else {
                    buffer.insert_at_cursor("0000-00-00 00:00");
                }
            } else {
                buffer.insert_at_cursor("0000-00-00 00:00");
            }
        });
    }
    app.add_action(&time_date);

    app.set_accels_for_action("app.select_all", &["<Primary>A"]);
    app.set_accels_for_action("app.time_date", &["F5"]);

    // ----- View actions (stubs) -----
    // Zoom In
    let zoom_in = SimpleAction::new("zoom_in", None);
    let window_clone = window.clone();
    zoom_in.connect_activate(move |_, _| unsafe {
        if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let current = *doc_state.zoom.borrow();
            if current < 500 {
                *doc_state.zoom.borrow_mut() = current + 10;
                update_zoom_css(doc_state);
            }
        }
    });
    app.add_action(&zoom_in);

    // Zoom Out
    let zoom_out = SimpleAction::new("zoom_out", None);
    let window_clone = window.clone();
    zoom_out.connect_activate(move |_, _| unsafe {
        if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let current = *doc_state.zoom.borrow();
            if current > 20 {
                *doc_state.zoom.borrow_mut() = current - 10;
                update_zoom_css(doc_state);
            }
        }
    });
    app.add_action(&zoom_out);

    // Zoom Reset
    let zoom_reset = SimpleAction::new("zoom_reset", None);
    let window_clone = window.clone();
    zoom_reset.connect_activate(move |_, _| unsafe {
        if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            *doc_state.zoom.borrow_mut() = 100;
            update_zoom_css(doc_state);
        }
    });
    app.add_action(&zoom_reset);

    // Add shortcuts
    app.set_accels_for_action("app.zoom_in", &["<Primary>plus", "<Primary>equal"]);
    app.set_accels_for_action("app.zoom_out", &["<Primary>minus"]);
    app.set_accels_for_action("app.zoom_reset", &["<Primary>0"]);

    let status_bar = SimpleAction::new_stateful(
        "status_bar",
        None,
        &true.to_variant(), // Default to true (visible)
    );
    let window_clone = window.clone();
    status_bar.connect_change_state(move |action, state| unsafe {
        if let Some(state) = state {
            action.set_state(state); // Update action state
            let visible = state.get::<bool>().unwrap_or(true);

            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();
                doc_state.status_box.set_visible(visible);
            }
        }
    });
    app.add_action(&status_bar);

    // ----- Mode actions -----
    // ----- Mode actions -----
    // Stateful "mode" action
    let initial_mode_str = unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            match doc_state.mode() {
                Mode::Plain => "plain",
                Mode::Markup => "markup",
            }
        } else {
            "plain"
        }
    };

    let mode_action = SimpleAction::new_stateful(
        "mode",
        Some(glib::VariantTy::STRING),
        &initial_mode_str.to_variant(),
    );

    {
        let window_clone = window.clone();
        let text_view_clone = text_view.clone();
        mode_action.connect_change_state(move |action, value| unsafe {
            if let Some(value) = value {
                let requested_mode_str = value.str().unwrap_or("plain");
                let requested_mode = match requested_mode_str {
                    "markup" => Mode::Markup,
                    _ => Mode::Plain,
                };

                if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();

                    // If already in that mode, just ensure state is sync and return
                    if doc_state.mode() == requested_mode {
                        action.set_state(value);
                        return;
                    }

                    // Check buffer
                    let sv_buffer = text_view_clone
                        .buffer()
                        .downcast::<sv::Buffer>()
                        .expect("Buffer is not sv::Buffer");

                    if !buffer_is_empty(&sv_buffer) {
                        // Show dialog
                        let dialog = gtk::MessageDialog::builder()
                            .transient_for(&window_clone)
                            .modal(true)
                            .message_type(gtk::MessageType::Info)
                            .buttons(gtk::ButtonsType::Ok)
                            .text("Cannot change mode while the document has content.")
                            .secondary_text(
                                "Create a new file or clear all text before switching between Plain and Markup.",
                            )
                            .build();

                        dialog.connect_response(|d, _| d.close());
                        dialog.show();
                        return; // Abort state change
                    }

                    // Apply changes
                    doc_state.set_mode(requested_mode);

                    // Update label
                    let label = match requested_mode {
                        Mode::Plain => "Plain Text",
                        Mode::Markup => "Markdown",
                    };
                    doc_state.label_mode.set_text(label);

                    // Apply language
                    apply_language_for_mode(&sv_buffer, requested_mode);

                    // Update title
                    let base_title = match doc_state.path() {
                        Some(path) => format!("rpad - {}", path.display()),
                        None => "rpad - Untitled".to_string(),
                    };
                    let suffix = match requested_mode {
                        Mode::Plain => " [Plain]",
                        Mode::Markup => " [Markdown]",
                    };
                    window_clone.set_title(Some(&format!("{}{}", base_title, suffix)));

                    // Update action state
                    action.set_state(value);
                }
            }
        });
    }
    app.add_action(&mode_action);

    // Sudo Mode Toggle
    let sudo_mode = SimpleAction::new_stateful("sudo_mode", None, &false.to_variant());
    {
        let window_clone = window.clone();
        sudo_mode.connect_change_state(move |action, value| unsafe {
            // "value" is the requested new state (Some(bool))
            if let Some(requested_state_variant) = value {
                let new_state = requested_state_variant.get::<bool>().unwrap_or(false);

                if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();

                    if new_state {
                        // Enable
                        if let Some(password) = prompt_for_password(&window_clone) {
                            if validate_sudo_password(&password) {
                                *doc_state.sudo_password.borrow_mut() = Some(password);
                                *doc_state.sudo_expiry.borrow_mut() = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(300),
                                );

                                // Success: apply state
                                action.set_state(&new_state.into());

                                // Update UI manually (or let set_sudo_state do it, but we already set action state above)
                                // set_sudo_state does: Title, Label, Action State.
                                // We can just call set_sudo_state(&window_clone, true);
                                // BUT set_sudo_state sets action state too. It's safe if it checks value,
                                // but simpler to just do UI updates here or call a UI-only helper.
                                // Let's use set_sudo_state but rely on its check (it won't hurt to set state again to same value).
                                set_sudo_state(&window_clone, true);
                            } else {
                                // Invalid password: do NOT set state.
                                // Menu item remains unchecked (reverts).
                                let dialog = gtk::MessageDialog::builder()
                                    .transient_for(&window_clone)
                                    .modal(true)
                                    .message_type(gtk::MessageType::Error)
                                    .buttons(gtk::ButtonsType::Ok)
                                    .text("Invalid Password")
                                    .build();
                                dialog.connect_response(|d, _| d.close());
                                dialog.show();
                            }
                        }
                        // If cancelled, do nothing (state remains false)
                    } else {
                        // Disable (unchecked)
                        *doc_state.sudo_password.borrow_mut() = None;
                        *doc_state.sudo_expiry.borrow_mut() = None;

                        action.set_state(&new_state.into());
                        set_sudo_state(&window_clone, false);

                        let dialog = gtk::MessageDialog::builder()
                            .transient_for(&window_clone)
                            .modal(true)
                            .message_type(gtk::MessageType::Info)
                            .buttons(gtk::ButtonsType::Ok)
                            .text("Sudo Mode Disabled")
                            .build();
                        dialog.connect_response(|d, _| d.close());
                        dialog.show();
                    }
                }
            }
        });
    }
    app.add_action(&sudo_mode);

    // ----- Help actions -----
    let about = SimpleAction::new("about", None);
    let window_clone = window.clone();
    about.connect_activate(move |_, _| {
        let dialog = gtk::AboutDialog::builder()
            .transient_for(&window_clone)
            .modal(true)
            .program_name("Rust Pad (rpad)")
            .version("0.1.0")
            .authors(vec!["Rhonald John Rose".to_string()])
            .website("https://github.com/rhonaldjr/rpad")
            .logo_icon_name("text-editor") // Use a generic icon name
            .build();

        dialog.present();
    });
    app.add_action(&about);
}

fn save_as_with_dialog(window: &gtk::ApplicationWindow) {
    use gtk::{FileChooserAction, FileFilter, ResponseType};

    let dialog = gtk::FileChooserDialog::new(
        Some("Save File"),
        Some(window),
        FileChooserAction::Save,
        &[
            ("_Cancel", ResponseType::Cancel),
            ("_Save", ResponseType::Accept),
        ],
    );

    // Decide default name based on current mode
    let mode = current_mode(window);
    let default_name = match mode {
        Mode::Plain => "Untitled.txt",
        Mode::Markup => "Untitled.md",
    };
    dialog.set_current_name(default_name);

    // Optional: filters match mode, but allow both
    let text_filter = FileFilter::new();
    text_filter.set_name(Some("Text Files (*.txt)"));
    text_filter.add_pattern("*.txt");
    dialog.add_filter(&text_filter);

    let md_filter = FileFilter::new();
    md_filter.set_name(Some("Markdown Files (*.md, *.markdown)"));
    md_filter.add_pattern("*.md");
    md_filter.add_pattern("*.markdown");
    dialog.add_filter(&md_filter);

    let all_filter = FileFilter::new();
    all_filter.set_name(Some("All Files"));
    all_filter.add_pattern("*");
    dialog.add_filter(&all_filter);

    let window_clone = window.clone();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(file) = dialog.file() {
                if let Some(path) = file.path() {
                    if let Err(err) = save_buffer_to_path(&window_clone, path.as_ref()) {
                        eprintln!("Error saving file: {err}");
                    }
                }
            }
        }

        dialog.close();
    });

    dialog.show();
}

fn current_mode(window: &gtk::ApplicationWindow) -> Mode {
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            doc_state.mode()
        } else {
            Mode::Plain
        }
    }
}

// Sudo Helpers

fn prompt_for_password(window: &gtk::ApplicationWindow) -> Option<String> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dialog = gtk::MessageDialog::builder()
        .transient_for(window)
        .modal(true)
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::OkCancel)
        .text("Enter Sudo Password")
        .build();

    let content_area = dialog.content_area();
    let entry = gtk::PasswordEntry::new();
    entry.set_hexpand(true);
    entry.set_margin_start(10);
    entry.set_margin_end(10);

    // Modern clone usage
    let dialog_weak = dialog.downgrade();
    entry.connect_activate(move |_| {
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.response(gtk::ResponseType::Ok);
        }
    });

    content_area.append(&entry);
    entry.grab_focus();
    dialog.show();

    // Block until response using iteration loop
    let response = Rc::new(RefCell::new(None));
    let response_clone = response.clone();
    dialog.connect_response(move |d, res| {
        *response_clone.borrow_mut() = Some(res);
        d.close();
    });

    let context = glib::MainContext::default();
    while response.borrow().is_none() {
        context.iteration(true);
    }

    if *response.borrow() == Some(gtk::ResponseType::Ok) {
        let text = entry.text();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    None
}

fn validate_sudo_password(password: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // sudo -S -v reads password from stdin and validates/updates timestamp
    let child = Command::new("sudo")
        .arg("-S")
        .arg("-v")
        .arg("-k") // -k ignores cached credentials, forcing validation of the provided password
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match child {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(format!("{}\n", password).as_bytes());
            }
            match child.wait() {
                Ok(status) => status.success(),
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn perform_sudo_save(path: &Path, content: &str, password: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // 1. Write to temp file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join("rpad_sudo_save.tmp");
    if let Err(e) = fs::write(&temp_file, content) {
        return Err(format!("Failed to write temp file: {}", e));
    }

    let status = Command::new("sudo")
        .arg("-S")
        .arg("cp")
        .arg(&temp_file)
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped()) // Capture error if any
        .spawn();

    match status {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(format!("{}\n", password).as_bytes());
            }
            match child.wait() {
                Ok(status) => {
                    let _ = fs::remove_file(temp_file);
                    if status.success() {
                        Ok(())
                    } else {
                        Err("Sudo save failed".to_string())
                    }
                }
                Err(e) => Err(format!("Failed to wait on sudo: {}", e)),
            }
        }
        Err(e) => Err(format!("Failed to spawn sudo: {}", e)),
    }
}

fn set_sudo_state(window: &gtk::ApplicationWindow, active: bool) {
    use gtk::gio;

    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();

            // Update Title
            let base_title = match doc_state.path() {
                Some(path) => format!("rpad - {}", path.display()),
                None => "rpad - Untitled".to_string(),
            };

            let suffix = if active { " [SUDO]" } else { "" };
            let mode_suffix = match doc_state.mode() {
                Mode::Plain => " [Plain]",
                Mode::Markup => " [Markdown]",
            };
            window.set_title(Some(&format!("{}{}{}", base_title, suffix, mode_suffix)));

            // Update Status Label
            doc_state.label_sudo.set_visible(active);

            // Update Action State
            if let Some(app) = window.application() {
                if let Some(action) = app.lookup_action("sudo_mode") {
                    if let Some(stateful_action) = action.downcast_ref::<gio::SimpleAction>() {
                        stateful_action.set_state(&active.into());
                    }
                }
            }
        }
    }
}

fn save_as_with_dialog_and_then_close(window: &gtk::ApplicationWindow) {
    use gtk::{FileChooserAction, FileFilter, ResponseType};

    let dialog = gtk::FileChooserDialog::new(
        Some("Save File"),
        Some(window),
        FileChooserAction::Save,
        &[
            ("_Cancel", ResponseType::Cancel),
            ("_Save", ResponseType::Accept),
        ],
    );

    let mode = current_mode(window);
    let default_name = match mode {
        Mode::Plain => "Untitled.txt",
        Mode::Markup => "Untitled.md",
    };
    dialog.set_current_name(default_name);

    let text_filter = FileFilter::new();
    text_filter.set_name(Some("Text Files (*.txt)"));
    text_filter.add_pattern("*.txt");
    dialog.add_filter(&text_filter);

    let md_filter = FileFilter::new();
    md_filter.set_name(Some("Markdown Files (*.md, *.markdown)"));
    md_filter.add_pattern("*.md");
    md_filter.add_pattern("*.markdown");
    dialog.add_filter(&md_filter);

    let all_filter = FileFilter::new();
    all_filter.set_name(Some("All Files"));
    all_filter.add_pattern("*");
    dialog.add_filter(&all_filter);

    let window_clone = window.clone();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(file) = dialog.file() {
                if let Some(path) = file.path() {
                    match save_buffer_to_path(&window_clone, path.as_ref()) {
                        Ok(()) => {
                            window_clone.close();
                        }
                        Err(err) => {
                            eprintln!("Error saving file: {err}");
                        }
                    }
                }
            }
        }

        dialog.close();
    });

    dialog.show();
}

fn apply_language_for_mode(buffer: &sv::Buffer, mode: Mode) {
    let lm = sv::LanguageManager::default();

    match mode {
        Mode::Plain => {
            buffer.set_language(None::<&sv::Language>);
        }
        Mode::Markup => {
            if let Some(lang) = lm.language("markdown") {
                buffer.set_language(Some(&lang));
            } else {
                buffer.set_language(None::<&sv::Language>);
            }
        }
    }
}

fn search_in_buffer(
    buffer: &sv::Buffer,
    text_view: &sv::View,
    pattern: &str,
    forward: bool,
    match_case: bool,
) -> Option<(gtk::TextIter, gtk::TextIter)> {
    if pattern.is_empty() {
        return None;
    }

    let mut flags = gtk::TextSearchFlags::TEXT_ONLY;
    if !match_case {
        flags |= gtk::TextSearchFlags::CASE_INSENSITIVE;
    }

    let insert_mark = buffer.get_insert();
    let iter = buffer.iter_at_mark(&insert_mark);

    let result = if forward {
        iter.forward_search(pattern, flags, None).or_else(|| {
            let start = buffer.start_iter();
            start.forward_search(pattern, flags, None)
        })
    } else {
        iter.backward_search(pattern, flags, None).or_else(|| {
            let end = buffer.end_iter();
            end.backward_search(pattern, flags, None)
        })
    };

    if let Some((mut match_start, match_end)) = result {
        buffer.select_range(&match_start, &match_end);
        text_view.scroll_to_iter(&mut match_start, 0.1, false, 0.0, 0.0);
        Some((match_start, match_end))
    } else {
        None
    }
}

fn do_find_next(window: &gtk::ApplicationWindow, text_view: &sv::View) {
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let pattern = doc_state.find_text.borrow().clone();
            if pattern.is_empty() {
                return;
            }
            let match_case = *doc_state.match_case.borrow();
            let buffer = text_view
                .buffer()
                .downcast::<sv::Buffer>()
                .expect("Buffer is not sv::Buffer");
            let _ = search_in_buffer(&buffer, text_view, &pattern, true, match_case);
        }
    }
}

fn do_find_prev(window: &gtk::ApplicationWindow, text_view: &sv::View) {
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let pattern = doc_state.find_text.borrow().clone();
            if pattern.is_empty() {
                return;
            }
            let match_case = *doc_state.match_case.borrow();
            let buffer = text_view
                .buffer()
                .downcast::<sv::Buffer>()
                .expect("Buffer is not sv::Buffer");
            let _ = search_in_buffer(&buffer, text_view, &pattern, false, match_case);
        }
    }
}

fn open_find_dialog(window: &gtk::ApplicationWindow, text_view: &sv::View) {
    let dialog = gtk::Dialog::builder()
        .transient_for(window)
        .modal(true)
        .title("Find")
        .build();

    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Find Next", gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(6);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.set_margin_start(6);
    content.set_margin_end(6);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(Some("Find what:"));
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    hbox.append(&label);
    hbox.append(&entry);

    let match_case_cb = gtk::CheckButton::with_label("Match case");

    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            entry.set_text(&doc_state.find_text.borrow());
            match_case_cb.set_active(*doc_state.match_case.borrow());
        }
    }

    content.append(&hbox);
    content.append(&match_case_cb);

    let win_clone = window.clone();
    let text_view_clone = text_view.clone();
    let entry_clone = entry.clone();
    let match_case_cb_clone = match_case_cb.clone();

    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let text = entry_clone.text().to_string();
            let match_case = match_case_cb_clone.is_active();

            unsafe {
                if let Some(doc_state_ptr) = win_clone.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();
                    *doc_state.find_text.borrow_mut() = text.clone();
                    *doc_state.match_case.borrow_mut() = match_case;
                }
            }

            let buffer = text_view_clone
                .buffer()
                .downcast::<sv::Buffer>()
                .expect("Buffer is not sv::Buffer");
            let _ = search_in_buffer(&buffer, &text_view_clone, &text, true, match_case);
        }
        dialog.close();
    });

    dialog.show();
}

fn open_replace_dialog(window: &gtk::ApplicationWindow, text_view: &sv::View) {
    let dialog = gtk::Dialog::builder()
        .transient_for(window)
        .modal(true)
        .title("Replace")
        .build();

    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Replace", gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(6);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.set_margin_start(6);
    content.set_margin_end(6);

    // Find row
    let find_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let find_label = gtk::Label::new(Some("Find what:"));
    let find_entry = gtk::Entry::new();
    find_entry.set_hexpand(true);
    find_box.append(&find_label);
    find_box.append(&find_entry);

    // Replace row
    let replace_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let replace_label = gtk::Label::new(Some("Replace with:"));
    let replace_entry = gtk::Entry::new();
    replace_entry.set_hexpand(true);
    replace_box.append(&replace_label);
    replace_box.append(&replace_entry);

    let match_case_cb = gtk::CheckButton::with_label("Match case");

    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            find_entry.set_text(&doc_state.find_text.borrow());
            match_case_cb.set_active(*doc_state.match_case.borrow());
        }
    }

    content.append(&find_box);
    content.append(&replace_box);
    content.append(&match_case_cb);

    let win_clone = window.clone();
    let text_view_clone = text_view.clone();
    let find_entry_clone = find_entry.clone();
    let replace_entry_clone = replace_entry.clone();
    let match_case_cb_clone = match_case_cb.clone();

    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let find_text = find_entry_clone.text().to_string();
            let replace_text = replace_entry_clone.text().to_string();
            let match_case = match_case_cb_clone.is_active();

            unsafe {
                if let Some(doc_state_ptr) = win_clone.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();
                    *doc_state.find_text.borrow_mut() = find_text.clone();
                    *doc_state.match_case.borrow_mut() = match_case;
                }
            }

            let buffer = text_view_clone
                .buffer()
                .downcast::<sv::Buffer>()
                .expect("Buffer is not sv::Buffer");

            if let Some((mut start, mut end)) =
                search_in_buffer(&buffer, &text_view_clone, &find_text, true, match_case)
            {
                buffer.begin_user_action();
                buffer.delete(&mut start, &mut end);
                buffer.insert(&mut start, &replace_text);
                buffer.end_user_action();
            }
        }
        dialog.close();
    });

    dialog.show();
}

fn open_goto_dialog(window: &gtk::ApplicationWindow, text_view: &sv::View) {
    let dialog = gtk::Dialog::builder()
        .transient_for(window)
        .modal(true)
        .title("Go To Line")
        .build();

    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Go To", gtk::ResponseType::Accept);

    let content = dialog.content_area();
    content.set_spacing(6);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.set_margin_start(6);
    content.set_margin_end(6);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(Some("Line number:"));
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    hbox.append(&label);
    hbox.append(&entry);
    content.append(&hbox);

    let text_view_clone = text_view.clone();
    let entry_clone = entry.clone();

    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Ok(line_num) = entry_clone.text().parse::<i32>() {
                let buffer = text_view_clone.buffer().upcast::<gtk::TextBuffer>();
                let mut line = line_num - 1;
                let max_lines = buffer.line_count();

                if max_lines > 0 {
                    if line < 0 {
                        line = 0;
                    }
                    if line >= max_lines {
                        line = max_lines - 1;
                    }

                    let mut iter = buffer.start_iter();
                    if line > 0 {
                        iter.forward_lines(line);
                    }

                    buffer.place_cursor(&iter);
                    text_view_clone.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
                }
            }
        }

        dialog.close();
    });

    dialog.show();
}

fn update_zoom_css(doc_state: &DocumentState) {
    let zoom = *doc_state.zoom.borrow();
    let css = format!("textview {{ font-size: {}%; }}", zoom);
    doc_state.css_provider.load_from_data(&css);
}

fn update_counts(doc_state: &DocumentState, buffer: &gtk::TextBuffer) {
    let (start, end) = buffer.bounds();
    let text = buffer.text(&start, &end, false);

    let char_count = text.chars().count();
    let word_count = text.split_whitespace().count();

    doc_state
        .label_words_chars
        .set_text(&format!("{} words, {} chars", word_count, char_count));
}

fn update_cursor(doc_state: &DocumentState, buffer: &gtk::TextBuffer) {
    let insert = buffer.get_insert();
    let iter = buffer.iter_at_mark(&insert);
    let line = iter.line() + 1;
    let col = iter.line_offset() + 1;

    doc_state
        .label_line_col
        .set_text(&format!("Ln {}, Col {}", line, col));
}
