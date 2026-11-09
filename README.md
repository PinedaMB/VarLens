<div align="center">

<img src="data/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg" width="128" height="128" alt="VarLens Logo" />

# VarLens

### The modern, blazing-fast native Linux developer lens for inspecting, visualizing, converting, and diffing structured data.

[![Release](https://img.shields.io/badge/release-v1.0.0-blue.svg)](https://github.com/PinedaMB/VarLens/releases)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org)
[![GTK4](https://img.shields.io/badge/GTK-4.12+-green.svg)](https://gtk.org)
[![Libadwaita](https://img.shields.io/badge/libadwaita-1.5+-purple.svg)](https://gnome.pages.gitlab.gnome.org/libadwaita)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows-lightgrey.svg)](https://github.com/PinedaMB/VarLens/releases)
[![License](https://img.shields.io/badge/license-MIT-informational.svg)](LICENSE)
[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-rainbowfacp-ffdd00?style=flat&logo=buy-me-a-coffee&logoColor=black)](https://buymeacoffee.com/rainbowfacp)

</div>

---

## 🌟 Why VarLens?

Modern backend and full-stack developers constantly deal with diverse structured formats: API JSON responses, legacy XML payloads, PHP debugging dumps (`var_dump()`, `print_r()`), and complex file differences. 

Web-based formatters compromise sensitive credentials and struggle with multi-megabyte payloads. Heavy IDEs are slow to open. 

**VarLens** is engineered specifically for Linux developers as a **zero-compromise, native desktop application**. Built with **Rust**, **GTK4**, and **Libadwaita**, it launches instantly, operates completely offline, runs heavy operations asynchronously, and adheres to GNOME's modern design standards.

---

## 📸 Screenshots

<div align="center">

### Side-by-Side Visual Diff Comparator
<img src="screenshots/diff-view.png" width="920" alt="VarLens Side-by-Side Diff View" />

<br/><br/>

| Interactive JSON Tree Explorer | JSON Inspector &amp; Live AST Metrics |
| :---: | :---: |
| <img src="screenshots/tree-view.png" width="450" alt="Tree View" /> | <img src="screenshots/inspector-json.png" width="450" alt="JSON Inspector" /> |

| PHP `var_dump()` &amp; `print_r()` Parser | XML Inspector &amp; Converter |
| :---: | :---: |
| <img src="screenshots/inspector-vardump.png" width="450" alt="PHP var_dump Inspector" /> | <img src="screenshots/inspector-xml.png" width="450" alt="XML Inspector" /> |

</div>

---

## ✨ Features at a Glance

### 🔍 Interactive JSON & XML Inspector
- **Dual View Modes:** Seamlessly toggle between an editable source view with code folding (`GtkSourceView 5`) and an **Interactive Tree Explorer**.
- **Collapsible Hierarchies:** Deep node unfolding and contracting with dedicated expand/collapse actions.
- **Deep Query Search:** Live case-insensitive search across keys and values with match navigation (`1/N`) and auto-expansion.
- **Instant Access Path Copying:** Click any node to generate and copy its programmatic access path (e.g., `data.users[0].credentials.api_key`) directly to your clipboard.
- **RFC 8259 & W3C Validation:** Live AST syntax validation badges, key counts, tree depth, and payload metrics.
- **Flexible Formatting:** Format with 2 spaces, 4 spaces, or tabs, or compact into a single line (Minify).

### 🐘 First-Class PHP `var_dump()` & `print_r()` Parser
- **Formal PEG Grammar:** Powered by a robust `pest` PEG parser that faithfully deserializes PHP dump outputs into valid JSON.
- **Handles PHP Complexity:** Supports typed primitives (`bool`, `int`, `float`, `string(N)`), visibility modifiers (`public`, `protected`, `private`), sequential arrays, associative arrays, objects, and nested recursion flags (`*RECURSION*`).
- **Raw Array Support:** Dedicated auto-detection and parsing for PHP `print_r()` / `Array raw` formats.
- **One-Click Conversion:** Transform raw PHP dumps into formatted JSON with a single shortcut.

### ⚖️ Side-by-Side Visual Diff Comparator
- **Synchronized Dual View:** Coordinated vertical scrolling and synchronized cursor tracking between base and modified editors.
- **Granular Change Detection:** Visualizes line additions (green) and deletions (red), complemented by **inline word/character-level highlight spans**.
- **Myers & Patience Algorithms:** Toggle between the standard Myers algorithm and the **Patience algorithm** in Preferences for clean, human-readable structural code diffs.
- **Whitespace Agnostic:** Ignore whitespace differences with a single switch.
- **Diff Chunk Navigation:** Jump quickly between changes with `Ctrl+[` and `Ctrl+]`.

### ⚡ True Non-Blocking Concurrency
- Payloads exceeding threshold limits are automatically offloaded to background threads (`std::thread`).
- The Libadwaita user interface stays fluid at 60 FPS with zero micro-stutters during heavy computation.

### 🌐 Native Internationalization (i18n)
- Comprehensive multilingual interface: **English** and **Spanish**.
- Dynamic, real-time language switching in Preferences without application restarts.
- Auto-detects system locale (`$LANG`) and persists user preferences.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action | Scope |
| :--- | :--- | :--- |
| <kbd>Ctrl</kbd> + <kbd>O</kbd> | Open File | Global |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd> | Format / Prettify Content | Inspector |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> | Minify Content | Inspector |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>J</kbd> | Convert `var_dump` / XML to JSON | Inspector |
| <kbd>Ctrl</kbd> + <kbd>Enter</kbd> | Recalculate Differences | Diff Viewer |
| <kbd>Ctrl</kbd> + <kbd>[</kbd> | Jump to Previous Difference | Diff Viewer |
| <kbd>Ctrl</kbd> + <kbd>]</kbd> | Jump to Next Difference | Diff Viewer |
| <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>C</kbd> | Clear All Diff Inputs | Diff Viewer |
| <kbd>Ctrl</kbd> + <kbd>?</kbd> | View Shortcuts Dialog | Global |

---

## 🛠️ Tech Stack

- **Core Language:** [Rust 2021 Edition](https://www.rust-lang.org/)
- **UI Toolkit:** [GTK 4.12+](https://gtk.org/) via [`gtk4-rs`](https://gtk-rs.org/)
- **Human Interface Guidelines:** [Libadwaita 1.5+](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Source Editor:** [GtkSourceView 5](https://gitlab.gnome.org/GNOME/gtksourceview) with custom light and dark color schemes
- **Diff Engine:** [`similar`](https://crates.io/crates/similar) with inline diff computation
- **Parser Engine:** [`pest`](https://pest.rs/) PEG grammar parser
- **Serialization:** [`serde_json`](https://crates.io/crates/serde_json) with key order preservation

---

## 🚀 Building & Running from Source

### Prerequisites

Ensure you have Rust and the required GTK4 / Libadwaita development libraries installed:

#### Fedora / RHEL
```bash
sudo dnf install -y rust cargo gtk4-devel libadwaita-devel gtksourceview5-devel
```

#### Ubuntu / Debian (23.04+)
```bash
sudo apt install -y cargo rustc libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev
```

#### Arch Linux
```bash
sudo pacman -S rust gtk4 libadwaita gtksourceview5
```

### Build & Run

```bash
# Clone the repository
git clone https://github.com/PinedaMB/VarLens.git
cd VarLens

# Run test suite
cargo test

# Build and execute in development mode
cargo run

# Build an optimized production release binary
cargo build --release
```

The optimized release executable will be available at `./target/release/varlens`.

---

## 📦 Desktop Integration

To install VarLens locally for your user account:

```bash
# Copy binary
install -Dm755 target/release/varlens ~/.local/bin/varlens

# Install application icon
install -Dm644 data/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg \
  ~/.local/share/icons/hicolor/scalable/apps/io.github.PinedaMB.VarLens.svg

# Install desktop entry and AppStream metadata
install -Dm644 data/io.github.PinedaMB.VarLens.desktop ~/.local/share/applications/io.github.PinedaMB.VarLens.desktop
install -Dm644 data/io.github.PinedaMB.VarLens.metainfo.xml ~/.local/share/metainfo/io.github.PinedaMB.VarLens.metainfo.xml

# Refresh icon cache and desktop database
gtk4-update-icon-cache -f -t ~/.local/share/icons/hicolor
update-desktop-database ~/.local/share/applications
```

### Windows Portable

Download `VarLens-windows-x86_64.zip` from [Releases](https://github.com/PinedaMB/VarLens/releases), extract the ZIP archive, and run `varlens.exe`. No installation or runtime dependencies required.

---

## 🤝 Contributing

Contributions, bug reports, and suggestions are welcome!
Feel free to open an issue or submit a pull request on [GitHub](https://github.com/PinedaMB/VarLens).

---

## 💖 Support the Project

VarLens is 100% free and open-source software built with dedication. If VarLens saves you time, simplifies your debugging, or enhances your daily development workflow, consider supporting its ongoing maintenance and new features:

<p align="left">
  <a href="https://buymeacoffee.com/rainbowfacp">
    <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" width="190" alt="Buy Me A Coffee" />
  </a>
</p>

- **[Buy Me a Coffee](https://buymeacoffee.com/rainbowfacp)** — Buy a coffee to support development.
- **Star the Repository** — A star on [GitHub](https://github.com/PinedaMB/VarLens) helps other Linux developers discover VarLens!

---

## 📄 License

VarLens is distributed under the open-source **[MIT License](LICENSE)**.
