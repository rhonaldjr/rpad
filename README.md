A lightweight, cross-platform notepad application written in Rust.

rpad is a simple desktop text editor for Linux and macOS, inspired by classic Windows Notepad but extended with support for multiple editing modes: Plain Text, Markup, and Rich Text (planned).
It includes both a graphical application and a command-line entry point.

Features (Current Stage)

Native GTK4 desktop UI

Menu bar with:

File → New, Open (stub), Save (stub), Save As (stub), Quit

Mode → Plain, Markup, Rich (stubs)

Resizable text editor window

CLI launcher:

rpad [FILE] --mode plain|markup|rich


Linux (Pop!_OS / Ubuntu) support implemented

macOS support planned after core features stabilize

Project Goals
Short-Term

File open/save functionality

Status bar (cursor position, modified indicator)

Document state handling (mode + file path)

Medium-Term

Markup mode (Markdown/HTML) with syntax highlighting

Rich text support using GTK text attributes

Auto-reload + autosave options

Preferences window

Long-Term

Plugin architecture (syntax highlighters, format converters)

Multi-tab support

Cross-platform packaging

.deb for Linux

.app bundle for macOS

Installation (Linux)
System dependencies

GTK4 development libraries:

sudo apt update
sudo apt install -y libgtk-4-dev

Build
git clone https://github.com/<yourname>/rpad.git
cd rpad
cargo build

Run
cargo run --

Optional global install
cargo install --path .
rpad

Command Line Usage
rpad <FILE> [--mode plain|markup|rich]


Examples:

rpad notes.txt
rpad index.md --mode markup
rpad --mode rich

Development Notes

The UI layer is built with GTK4 using the gtk4 Rust bindings.

All GTK actions (File / Mode menu entries) are registered via gio::SimpleAction.

Missing or incomplete actions currently log debug output.

The code currently uses a single TextView in a ScrolledWindow, with plans to abstract a Document struct later.

To run static analysis:

cargo check


To apply automatic fixes:

cargo fix

Platform Roadmap
Platform	Status	Notes
Linux (GTK4)	✅ Working	Primary development target
macOS	🔄 Planned	Requires Homebrew GTK4 + bundle integration
Windows	❌ Not planned	Focus is Linux/macOS Notepad-like editor
Project Structure
src/
 └── main.rs          # CLI, GTK app initialization, menu, text editor
Cargo.toml
README.md


Future structure will include:

src/
 ├── app.rs
 ├── ui/
 │    ├── window.rs
 │    ├── menus.rs
 │    └── actions.rs
 └── document/
      ├── mod.rs
      ├── plain.rs
      ├── markup.rs
      └── rich.rs

License

MIT.