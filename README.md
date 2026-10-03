# CryoDB

CryoDB is a cross-platform database toolkit built with Rust.

It provides multiple interfaces for managing, exploring, and querying databases:

* GUI (Desktop Application)
* TUI (Terminal User Interface)
* CLI (Command Line Interface)

All interfaces are powered by a shared core engine, providing a consistent experience across desktop, terminal, and automation workflows.

https://github.com/user-attachments/assets/ab1c78b3-e296-41f8-907d-4e35154d31a0

---

## Features

* Cross-platform support
* Desktop GUI built with iced.rs
* Terminal-based TUI
* Powerful CLI for scripting and automation
* Shared database engine across all frontends
* Multiple rendering backends
* Native performance with Rust
* Modern database management and SQL querying

---

## Interfaces

| Interface | Description                                                                      |
| --------- | -------------------------------------------------------------------------------- |
| GUI       | Full-featured desktop experience for database administration and query execution |
| TUI       | Keyboard-driven terminal interface optimized for productivity                    |
| CLI       | Command-line tooling for automation, scripting, and CI/CD workflows              |

---

## Architecture

All CryoDB frontends share the same core engine.

This allows the GUI, TUI, and CLI to use the same:

* Database drivers
* Connection management
* Query execution engine
* Database introspection
* Configuration system

```text
CryoDB
├── GUI
├── TUI
├── CLI
└── Core Engine
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the detailed GUI structure and contribution rules.

---

## Supported Platforms

| Platform                    | Renderer Feature               |
| --------------------------- | ------------------------------ |
| Linux (Wayland + Vulkan)    | `renderer-wgpu-wayland-vulkan` |
| Linux (Wayland + OpenGL ES) | `renderer-wgpu-wayland-gles`   |
| Linux (X11)                 | `renderer-wgpu-x11`            |
| Windows (DirectX 12)        | `renderer-wgpu-windows-dx12`   |
| macOS (Metal)               | `renderer-wgpu-macos-metal`    |

---

## Download

Installers for Linux, Windows and macOS are on the [Releases](https://github.com/znxr/cryodb/releases) page.

---

## Running

Run CryoDB using the renderer backend appropriate for your platform:

```bash
cargo run --release --no-default-features --features <renderer-feature>
```

Example:

```bash
cargo run --release --no-default-features --features renderer-wgpu-wayland-vulkan
```

---

## Building

Detailed build instructions can be found in [docs/BUILD.md](docs/BUILD.md).

---

## Changelog

Release notes live in [CHANGELOG.md](./CHANGELOG.md). They are compiled into the
binary, so the app shows them under Settings → About → **What's new**, and
announces them once after a feature update.

---

## Configuration

CryoDB searches for its configuration directory in the following order:

| Priority | Location                   |
| -------- | -------------------------- |
| 1        | `CRYODB_CONFIG_DIR`       |
| 2        | `$XDG_CONFIG_HOME/cryodb` |
| 3        | `$HOME/.config/cryodb`    |

---

## Development

Run a debug build:

```bash
cargo run --no-default-features --features <renderer-feature>
```

Automatically rebuild and rerun on file changes:

```bash
cargo watch -x "run --no-default-features --features <renderer-feature>"
```

---

## Technology Stack

* Rust
* iced.rs
* wgpu

---

## Project Status

CryoDB is under active development.

Features, configuration formats, and user interfaces may change between releases.

---

## License

See [LICENSE](./LICENSE).
