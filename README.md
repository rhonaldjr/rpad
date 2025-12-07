```markdown
# rpad  
A lightweight Rust notepad built with GTK4.  
The editor focuses on simplicity, fast startup, and a minimal codebase suitable for learning or extending GTK4 applications.

## Features
- Plain-text editing (Markdown and Rich Text modes planned)  
- Open, Save, Save As workflows  
- Unsaved-changes detection with confirmation dialog  
- Cut, Copy, Paste, Delete  
- Find and Replace  
- Keyboard shortcuts (Ctrl+S, Ctrl+F, etc.)  
- CLI launch with optional file and mode selection  
- Clean separation between UI, text buffer, and file I/O  

## CLI Usage
```

rpad [FILE] [--mode plain|markup|rich]

````

### Arguments
- **FILE**  
  Optional file path to open on launch.
- **--mode**  
  Selects the editing mode. Defaults to `plain`.

Modes are defined in code as:
```rust
enum Mode { Plain, Markup, Rich }
````

## Project Structure

```
src/
  main.rs          → Application bootstrap, menu actions, window setup
```

## Build & Run

### Requirements

* Rust stable
* GTK4 development libraries installed

### Build

```
cargo build
```

### Run

```
cargo run -- [options]
```

## Save Workflow

* If the file is new: Save shows a GTK file chooser, defaults to `Untitled.txt`
* If already saved: Save writes directly to disk
* Save As always opens a new chooser
* Supported formats:

  * `.txt` (default)
  * `.md`

## Close Confirmation

The window only closes when:

* No unsaved changes, or
* User selects **Save** or **Don't Save** in the dialog

## Find & Replace

The editor scans the entire buffer and replaces all matches.
Future enhancement: scoped replace (selection-only).

## Roadmap

* Syntax highlighting for Markdown
* Preferences dialog
* Session restore
* Cross-platform packaging (Flatpak, dmg, exe)
* Sudo mode indicator

## Development Notes

* Uses `glib::clone!` replacement patterns where needed
* GTK actions are centralized for menu and shortcuts
* TextBuffer wrapped with `Rc<RefCell<_>>` for controlled mutability

## License

MIT (or specify if different)

## Contributions

Open to improvements, refactoring, and feature PRs.

```

---

If you want this README tailored with badges, screenshots, a full architecture diagram, or a contribution guide, type **continue**.
```
