use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;


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

    // Main text area (plain text for now)
    let text_view = gtk::TextView::new();
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .hexpand(true)
        .vexpand(true)
        .build();

    // Menu bar stub (File + Mode)
    let menubar = build_menubar();

    // Pack menu + editor vertically
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.append(&menubar);
    vbox.append(&scrolled);

    window.set_child(Some(&vbox));

    // Register actions (only Quit wired for now)
    register_actions(app, &window);

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
    edit_menu.append(Some("Undo"),          Some("app.undo"));
    edit_menu.append(Some("Redo"),          Some("app.redo"));
    edit_menu.append(Some("Cut"),           Some("app.cut"));
    edit_menu.append(Some("Copy"),          Some("app.copy"));
    edit_menu.append(Some("Paste"),         Some("app.paste"));
    edit_menu.append(Some("Delete"),        Some("app.delete"));
    edit_menu.append(Some("Find…"),         Some("app.find"));
    edit_menu.append(Some("Find Next"),     Some("app.find_next"));
    edit_menu.append(Some("Find Previous"), Some("app.find_previous"));
    edit_menu.append(Some("Replace…"),      Some("app.replace"));
    edit_menu.append(Some("Go To…"),        Some("app.goto"));
    edit_menu.append(Some("Select All"),    Some("app.select_all"));
    edit_menu.append(Some("Time/Date"),     Some("app.time_date"));
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


fn register_actions(app: &gtk::Application, window: &gtk::ApplicationWindow) {
    use gtk::gio::SimpleAction;

    // ----- File actions -----

    // Quit / Exit
    let quit = SimpleAction::new("quit", None);
    let app_clone = app.clone();
    quit.connect_activate(move |_, _| {
        app_clone.quit();
    });
    app.add_action(&quit);

    // New (clear current document)
    let new_doc = SimpleAction::new("new", None);
    let window_clone = window.clone();
    new_doc.connect_activate(move |_, _| {
        window_clone.set_title(Some("rpad - Untitled"));
        if let Some(child) = window_clone.child() {
            if let Ok(box_container) = child.downcast::<gtk::Box>() {
                if let Some(scrolled) = box_container.last_child() {
                    if let Ok(scrolled) = scrolled.downcast::<gtk::ScrolledWindow>() {
                        if let Some(text_view) = scrolled.child() {
                            if let Ok(text_view) = text_view.downcast::<gtk::TextView>() {
                                text_view.buffer().set_text("");
                            }
                        }
                    }
                }
            }
        }
    });
    app.add_action(&new_doc);

    // New Window (stub: later spawn a new process or instance)
    let new_window = SimpleAction::new("new_window", None);
    new_window.connect_activate(|_, _| {
        eprintln!("New Window not implemented yet.");
    });
    app.add_action(&new_window);

    // File: Open/Save/Save As/Page Setup/Print (stubs for now)
    for (name, label) in [
        ("open",        "Open"),
        ("save",        "Save"),
        ("save_as",     "Save As"),
        ("page_setup",  "Page Setup"),
        ("print",       "Print"),
    ] {
        let action = SimpleAction::new(name, None);
        let label = label.to_string();
        action.connect_activate(move |_, _| {
            eprintln!("{} not implemented yet.", label);
        });
        app.add_action(&action);
    }

    // ----- Edit actions (stubs) -----
    for (name, label) in [
        ("undo",         "Undo"),
        ("redo",         "Redo"),
        ("cut",          "Cut"),
        ("copy",         "Copy"),
        ("paste",        "Paste"),
        ("delete",       "Delete"),
        ("find",         "Find"),
        ("find_next",    "Find Next"),
        ("find_prev",    "Find Previous"),
        ("replace",      "Replace"),
        ("goto",         "Go To"),
        ("select_all",   "Select All"),
        ("time_date",    "Time/Date"),
    ] {
        let action = SimpleAction::new(name, None);
        let label = label.to_string();
        action.connect_activate(move |_, _| {
            eprintln!("{} not implemented yet.", label);
        });
        app.add_action(&action);
    }

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
