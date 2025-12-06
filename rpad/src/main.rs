use std::cell::RefCell;
use std::fs;
use std::path::{PathBuf, Path};

use clap::{Parser, ValueEnum};
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::gio;
use gtk::glib::{self, Propagation, clone};

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
    Rich,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Plain,
    Markup,
    Rich,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Plain => Mode::Plain,
            ModeArg::Markup => Mode::Markup,
            ModeArg::Rich => Mode::Rich,
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
    undo_stack: RefCell<Vec<String>>,
    redo_stack: RefCell<Vec<String>>,
    last_text: RefCell<String>,
    is_programmatic: RefCell<bool>,
    dirty: RefCell<bool>,           // 🔹 NEW
    find_text: RefCell<String>,   // 🔍 last search text
    match_case: RefCell<bool>,    // 🔍 match case flag
}

impl DocumentState {
    fn new(initial: Option<PathBuf>) -> Self {
        Self {
            path: RefCell::new(initial),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            last_text: RefCell::new(String::new()),
            is_programmatic: RefCell::new(false),
            dirty: RefCell::new(false),   // 🔹 NEW
            find_text: RefCell::new(String::new()),
            match_case: RefCell::new(false),
        }
    }

    fn set_path(&self, new_path: Option<PathBuf>) {
        *self.path.borrow_mut() = new_path;
    }

    fn path(&self) -> Option<PathBuf> {
        self.path.borrow().clone()
    }
    
    // 🔹 NEW
    fn set_dirty(&self, value: bool) {
        *self.dirty.borrow_mut() = value;
    }

    // 🔹 NEW
    fn is_dirty(&self) -> bool {
        *self.dirty.borrow()
    }
}

fn main() {
    // 1. Parse CLI args
    let args = Args::parse();
    let config = AppConfig {
        mode: args.mode.into(),
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

    // Track current file path in window data
    let doc_state = DocumentState::new(config.file.clone());
    unsafe {
        window.set_data("rpad-doc-state", doc_state);
    }

    // Main text area (plain text for now)
    let text_view = gtk::TextView::new();
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    // 🔹 Add padding inside the editor
    text_view.set_left_margin(12);
    text_view.set_right_margin(12);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);

    // 🔹 Track edits for undo/redo *and* dirty flag
    let buffer = text_view.buffer();
    {
        let window_clone = window.clone();
        buffer.connect_changed(move |buf| {
            unsafe {
                if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                    let doc_state: &DocumentState = doc_state_ptr.as_ref();

                    // Skip changes we make programmatically (Open, Undo, Redo, New)
                    if *doc_state.is_programmatic.borrow() {
                        return;
                    }

                    let (start, end) = buf.bounds();
                    let text = buf.text(&start, &end, false).to_string();

                    let mut last_text = doc_state.last_text.borrow_mut();
                    if text != *last_text {
                        // push previous content into undo stack
                        doc_state.undo_stack.borrow_mut().push(last_text.clone());
                        // clear redo stack on new edit
                        doc_state.redo_stack.borrow_mut().clear();
                        // update last_text
                        *last_text = text;

                        // mark document dirty
                        doc_state.set_dirty(true);
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

    // Menu bar stub (File + Mode)
    let menubar = build_menubar();

    // Pack menu + editor vertically
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.append(&menubar);
    vbox.append(&scrolled);

    window.set_child(Some(&vbox));

        // Ask for confirmation when closing if there are unsaved changes
    {
        let window_clone = window.clone();
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
                                unsafe {
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
                            }
                            gtk::ResponseType::Reject => {
                                // Don't save: mark as clean so close_request won't re-prompt
                                unsafe {
                                    if let Some(doc_state_ptr) =
                                        win_for_dialog.data::<DocumentState>("rpad-doc-state")
                                    {
                                        let doc_state: &DocumentState = doc_state_ptr.as_ref();
                                        doc_state.set_dirty(false);
                                    }
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
    file_menu.append(Some("New"),          Some("app.new"));
    file_menu.append(Some("New Window"),   Some("app.new_window"));
    file_menu.append(Some("Open…"),        Some("app.open"));
    file_menu.append(Some("Save"),         Some("app.save"));
    file_menu.append(Some("Save As…"),     Some("app.save_as"));
    file_menu.append(Some("Page Setup…"),  Some("app.page_setup"));
    file_menu.append(Some("Print…"),       Some("app.print"));
    file_menu.append(Some("Exit"),         Some("app.quit"));
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
    group2.append(Some("Cut"),   Some("app.cut"));
    group2.append(Some("Copy"),  Some("app.copy"));
    group2.append(Some("Paste"), Some("app.paste"));
    group2.append(Some("Delete"), Some("app.delete"));
    edit_menu.append_section(None, &group2);

    //
    // Group 3: Find / Find Next / Find Previous / Replace / Go To
    //
    let group3 = gio::Menu::new();
    group3.append(Some("Find…"),          Some("app.find"));
    group3.append(Some("Find Next"),      Some("app.find_next"));
    group3.append(Some("Find Previous"),  Some("app.find_prev"));
    group3.append(Some("Replace…"),       Some("app.replace"));
    group3.append(Some("Go To…"),         Some("app.goto"));
    edit_menu.append_section(None, &group3);

    //
    // Group 4: Select All / Time/Date
    //
    let group4 = gio::Menu::new();
    group4.append(Some("Select All"), Some("app.select_all"));
    group4.append(Some("Time/Date"),  Some("app.time_date"));
    edit_menu.append_section(None, &group4);

    root.append_submenu(Some("Edit"), &edit_menu);

    // ----- Format menu -----
    let format_menu = gio::Menu::new();
    format_menu.append(Some("Word Wrap"), Some("app.word_wrap"));
    format_menu.append(Some("Font…"),     Some("app.font"));
    root.append_submenu(Some("Format"), &format_menu);

    // ----- View menu -----
    let view_menu = gio::Menu::new();

    let zoom_menu = gio::Menu::new();
    zoom_menu.append(Some("Zoom In"),            Some("app.zoom_in"));
    zoom_menu.append(Some("Zoom Out"),           Some("app.zoom_out"));
    zoom_menu.append(Some("Restore Default Zoom"), Some("app.zoom_reset"));

    view_menu.append_submenu(Some("Zoom"), &zoom_menu);
    view_menu.append(Some("Status Bar"), Some("app.status_bar"));
    root.append_submenu(Some("View"), &view_menu);

    // ----- Mode menu (your custom feature) -----
    let mode_menu = gio::Menu::new();
    mode_menu.append(Some("Plain Text"), Some("app.mode_plain"));
    mode_menu.append(Some("Markup"),     Some("app.mode_markup"));
    mode_menu.append(Some("Rich Text"),  Some("app.mode_rich"));
    root.append_submenu(Some("Mode"), &mode_menu);

    // ----- Help menu -----
    let help_menu = gio::Menu::new();
    help_menu.append(Some("View Help"), Some("app.view_help"));
    help_menu.append(Some("About rpad"), Some("app.about"));
    root.append_submenu(Some("Help"), &help_menu);

    gtk::PopoverMenuBar::from_model(Some(&root))
}

fn get_text_buffer_from_window(window: &gtk::ApplicationWindow) -> Option<gtk::TextBuffer> {
    if let Some(child) = window.child() {
        if let Ok(box_container) = child.downcast::<gtk::Box>() {
            // TextView is inside ScrolledWindow → inside vbox
            if let Some(scrolled) = box_container.last_child() {
                if let Ok(scrolled) = scrolled.downcast::<gtk::ScrolledWindow>() {
                    if let Some(text_view) = scrolled.child() {
                        if let Ok(text_view) = text_view.downcast::<gtk::TextView>() {
                            return Some(text_view.buffer());
                        }
                    }
                }
            }
        }
    }
    None
}

fn save_buffer_to_path(
    window: &gtk::ApplicationWindow,
    path: &Path,
) -> Result<(), std::io::Error> {
    if let Some(buffer) = get_text_buffer_from_window(window) {
        let (start, end) = buffer.bounds();
        let text = buffer.text(&start, &end, false); // GString
        fs::write(path, text.as_str())?;
    }

    // Update window title
    window.set_title(Some(&format!("rpad - {}", path.display())));

    // Update stored path + mark clean
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            doc_state.set_path(Some(path.to_path_buf()));
            doc_state.set_dirty(false);   // 🔹 here
        }
    }

    Ok(())
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
            doc_state.set_dirty(false);   // 🔹 clean

            *doc_state.is_programmatic.borrow_mut() = false;
        }
    }

    window.set_title(Some(&format!("rpad - {}", path.display())));
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

    // Filter: Text Files (*.txt)
    let text_filter = FileFilter::new();
    text_filter.set_name(Some("Text Files (*.txt)"));
    text_filter.add_pattern("*.txt");
    dialog.add_filter(&text_filter);

    // Filter: All Files (*)
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

fn register_actions(
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    text_view: &gtk::TextView,) {
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
    new_doc.connect_activate(move |_, _| {
        window_clone.set_title(Some("rpad - Untitled"));

        unsafe {
            if let Some(doc_state_ptr) = window_clone.data::<DocumentState>("rpad-doc-state") {
                let doc_state: &DocumentState = doc_state_ptr.as_ref();

                *doc_state.is_programmatic.borrow_mut() = true;

                if let Some(buffer) = get_text_buffer_from_window(&window_clone) {
                    buffer.set_text("");
                }

                doc_state.set_path(None);
                doc_state.undo_stack.borrow_mut().clear();
                doc_state.redo_stack.borrow_mut().clear();
                *doc_state.last_text.borrow_mut() = String::new();
                doc_state.set_dirty(false);   // 🔹 clean new doc

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

    // File: Open / Page Setup / Print (still stubs)
    for (name, label) in [
        ("page_setup", "Page Setup"),
        ("print",      "Print"),
    ] {
        let action = SimpleAction::new(name, None);
        let label = label.to_string();
        action.connect_activate(move |_, _| {
            eprintln!("{} not implemented yet.", label);
        });
        app.add_action(&action);
    }

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
    app.set_accels_for_action("app.cut", &["<Primary>X"]);
    app.set_accels_for_action("app.copy", &["<Primary>C"]);
    app.set_accels_for_action("app.paste", &["<Primary>V"]);
    app.set_accels_for_action("app.delete", &["Delete"]);

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

    // ----- Format actions (stubs) -----
    let word_wrap = SimpleAction::new("word_wrap", None);
    word_wrap.connect_activate(|_, _| {
        eprintln!("Word Wrap toggle not implemented yet.");
    });
    app.add_action(&word_wrap);

    let font = SimpleAction::new("font", None);
    font.connect_activate(|_, _| {
        eprintln!("Font dialog not implemented yet.");
    });
    app.add_action(&font);

    // ----- View actions (stubs) -----
    for (name, label) in [
        ("zoom_in",    "Zoom In"),
        ("zoom_out",   "Zoom Out"),
        ("zoom_reset", "Restore Default Zoom"),
    ] {
        let action = SimpleAction::new(name, None);
        let label = label.to_string();
        action.connect_activate(move |_, _| {
            eprintln!("{} not implemented yet.", label);
        });
        app.add_action(&action);
    }

    let status_bar = SimpleAction::new("status_bar", None);
    status_bar.connect_activate(|_, _| {
        eprintln!("Status Bar toggle not implemented yet.");
    });
    app.add_action(&status_bar);

    // ----- Mode actions (your custom modes) -----
    let mode_plain = SimpleAction::new("mode_plain", None);
    mode_plain.connect_activate(|_, _| {
        eprintln!("Mode switched to Plain Text (not wired yet).");
    });
    app.add_action(&mode_plain);

    let mode_markup = SimpleAction::new("mode_markup", None);
    mode_markup.connect_activate(|_, _| {
        eprintln!("Mode switched to Markup (not wired yet).");
    });
    app.add_action(&mode_markup);

    let mode_rich = SimpleAction::new("mode_rich", None);
    mode_rich.connect_activate(|_, _| {
        eprintln!("Mode switched to Rich Text (not wired yet).");
    });
    app.add_action(&mode_rich);

    // ----- Help actions (stubs) -----
    let view_help = SimpleAction::new("view_help", None);
    view_help.connect_activate(|_, _| {
        eprintln!("View Help not implemented yet.");
    });
    app.add_action(&view_help);

    let about = SimpleAction::new("about", None);
    about.connect_activate(|_, _| {
        eprintln!("About dialog not implemented yet.");
    });
    app.add_action(&about);
}


fn save_as_with_dialog(window: &gtk::ApplicationWindow) {
    use gtk::{FileChooserAction, ResponseType};

    let dialog = gtk::FileChooserDialog::new(
        Some("Save File"),
        Some(window),
        FileChooserAction::Save,
        &[
            ("_Cancel", ResponseType::Cancel),
            ("_Save", ResponseType::Accept),
        ],
    );

    // Default filename
    dialog.set_current_name("Untitled.txt");

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

fn save_as_with_dialog_and_then_close(window: &gtk::ApplicationWindow) {
    use gtk::{FileChooserAction, ResponseType};

    let dialog = gtk::FileChooserDialog::new(
        Some("Save File"),
        Some(window),
        FileChooserAction::Save,
        &[
            ("_Cancel", ResponseType::Cancel),
            ("_Save", ResponseType::Accept),
        ],
    );

    dialog.set_current_name("Untitled.txt");

    let window_clone = window.clone();
    dialog.connect_response(move |dialog, response| {
        if response == ResponseType::Accept {
            if let Some(file) = dialog.file() {
                if let Some(path) = file.path() {
                    match save_buffer_to_path(&window_clone, path.as_ref()) {
                        Ok(()) => {
                            // After successful save, close the window
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

fn search_in_buffer(
    buffer: &gtk::TextBuffer,
    text_view: &gtk::TextView,
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
    let mut iter = buffer.iter_at_mark(&insert_mark);

    let result = if forward {
        iter.forward_search(pattern, flags, None)
            .or_else(|| {
                let mut start = buffer.start_iter();
                start.forward_search(pattern, flags, None)
            })
    } else {
        iter.backward_search(pattern, flags, None)
            .or_else(|| {
                let mut end = buffer.end_iter();
                end.backward_search(pattern, flags, None)
            })
    };

    if let Some((mut match_start, mut match_end)) = result {
        buffer.select_range(&match_start, &match_end);
        text_view.scroll_to_iter(&mut match_start, 0.1, false, 0.0, 0.0);
        Some((match_start, match_end))
    } else {
        None
    }
}

fn do_find_next(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let pattern = doc_state.find_text.borrow().clone();
            if pattern.is_empty() {
                return;
            }
            let match_case = *doc_state.match_case.borrow();
            let buffer = text_view.buffer();
            let _ = search_in_buffer(&buffer, text_view, &pattern, true, match_case);
        }
    }
}

fn do_find_prev(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            let pattern = doc_state.find_text.borrow().clone();
            if pattern.is_empty() {
                return;
            }
            let match_case = *doc_state.match_case.borrow();
            let buffer = text_view.buffer();
            let _ = search_in_buffer(&buffer, text_view, &pattern, false, match_case);
        }
    }
}

fn open_find_dialog(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
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

    // pre-fill from last state if any
    unsafe {
        if let Some(doc_state_ptr) = window.data::<DocumentState>("rpad-doc-state") {
            let doc_state: &DocumentState = doc_state_ptr.as_ref();
            entry.set_text(&doc_state.find_text.borrow());
            match_case_cb.set_active(*doc_state.match_case.borrow());
        }
    }

    content.append(&hbox);
    content.append(&match_case_cb);

    dialog.connect_response(clone!(@weak window as win, @weak text_view, @weak entry, @weak match_case_cb =>
        move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let text = entry.text().to_string();
                let match_case = match_case_cb.is_active();

                unsafe {
                    if let Some(doc_state_ptr) = win.data::<DocumentState>("rpad-doc-state") {
                        let doc_state: &DocumentState = doc_state_ptr.as_ref();
                        *doc_state.find_text.borrow_mut() = text.clone();
                        *doc_state.match_case.borrow_mut() = match_case;
                    }
                }

                let buffer = text_view.buffer();
                let _ = search_in_buffer(&buffer, &text_view, &text, true, match_case);
            }
            dialog.close();
        }
    ));

    dialog.show();
}

fn open_replace_dialog(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
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

    // pre-fill from state
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

    dialog.connect_response(clone!(@weak window as win, @weak text_view, @weak find_entry, @weak replace_entry, @weak match_case_cb =>
        move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let find_text = find_entry.text().to_string();
                let replace_text = replace_entry.text().to_string();
                let match_case = match_case_cb.is_active();

                unsafe {
                    if let Some(doc_state_ptr) = win.data::<DocumentState>("rpad-doc-state") {
                        let doc_state: &DocumentState = doc_state_ptr.as_ref();
                        *doc_state.find_text.borrow_mut() = find_text.clone();
                        *doc_state.match_case.borrow_mut() = match_case;
                    }
                }

                let buffer = text_view.buffer();

                if let Some((mut start, mut end)) =
                    search_in_buffer(&buffer, &text_view, &find_text, true, match_case)
                {
                    buffer.begin_user_action();
                    buffer.delete(&mut start, &mut end);
                    buffer.insert(&mut start, &replace_text);
                    buffer.end_user_action();
                }
            }
            dialog.close();
        }
    ));

    dialog.show();
}

fn open_goto_dialog(window: &gtk::ApplicationWindow, text_view: &gtk::TextView) {
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

    // Clone widgets into the closure instead of using clone! macro
    let text_view_clone = text_view.clone();
    let entry_clone = entry.clone();

    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Ok(line_num) = entry_clone.text().parse::<i32>() {
                let buffer = text_view_clone.buffer();
                let mut line = line_num - 1; // 1-based → 0-based
                let max_lines = buffer.line_count();

                if max_lines > 0 {
                    if line < 0 {
                        line = 0;
                    }
                    if line >= max_lines {
                        line = max_lines - 1;
                    }

                    // Compute iter manually
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

