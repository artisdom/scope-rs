# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`scope-monitor` (binary name `scope`) is a multi-platform serial/RTT monitor written in Rust. The primary UI is a ratatui/crossterm TUI; an experimental egui/eframe GUI is being added (see the `gui` subcommand and `src/gui/`). Plugins are Lua scripts loaded at runtime via `mlua`.

- Edition: 2024, MSRV `1.92.0`.
- `src/main.rs` has `#![deny(warnings)]` — warnings break the build. Fix them, do not silence them.

## Common commands

```shell
# Build / run
cargo build --release
cargo run -- serial COM3 115200      # start TUI on a serial port
cargo run -- rtt <target> <channel>  # start TUI on RTT
cargo run -- list -v                 # list serial ports
cargo run -- gui                     # experimental egui GUI

# CI gates (must pass)
cargo fmt --all -- --check           # rustfmt is enforced (.github/workflows/fmt-check.yml)
cargo build --release --verbose      # built on Ubuntu/Windows/macOS in CI

# Tests
cargo test
cargo test <name_substring>          # run a single test
cargo test -p scope-monitor plugin::tests::test_plugin_new -- --nocapture
```

On Linux, building requires `libudev-dev` (for `serialport`). On Windows, `ctrlc` is used to swallow Ctrl+C so it stays an in-app shortcut rather than terminating the process.

CLI flags (top-level, before the subcommand): `-c/--capacity` (history buffer size, default 2000), `-t/--tag-file` (default `tags.yml`), `-l/--latency` (ms, clamped 0–100000, default 100).

## Architecture

A session is built from **four long-running tasks** that communicate exclusively over channels. They are wired up in `app_serial` / `app_rtt` in [src/main.rs](src/main.rs) — that wiring is the canonical reference for how the system fits together:

1. **InterfaceTask** ([src/interfaces/](src/interfaces/)) — owns the serial port or RTT session. Receives `InterfaceCommand` (Serial/RTT setup, send, disconnect, …); pushes inbound bytes onto `rx_channel`; consumes `tx_channel` to send.
2. **InputsTask** ([src/inputs/](src/inputs/)) — keyboard/command-bar handling, command history, tag expansion. Translates user actions into `InterfaceCommand`, `GraphicsCommand`, or `PluginEngineCommand`.
3. **GraphicsTask** ([src/graphics/](src/graphics/)) — ratatui screen, ANSI parsing ([ansi.rs](src/graphics/ansi.rs)), buffered scrollback ([buffer.rs](src/graphics/buffer.rs)), selection ([selection.rs](src/graphics/selection.rs)), recording, save-to-file.
4. **PluginEngine** ([src/plugin/](src/plugin/)) — hosts Lua plugins, dispatches events (`on_serial_recv`, `on_rtt_recv`, user `!cmd` calls), bridges Lua calls back to interface/graphics commands.

Two pieces of shared infrastructure tie them together:

- **`infra::task::Task<S, M>`** ([src/infra/task.rs](src/infra/task.rs)) — every long-running component is a `Task` with shared state (`Arc<RwLock<S>>`, exposed read-only via `Shared<S>`) plus an `mpsc::Sender<M>` for commands. Tasks see each other only via these handles, never via direct ownership.
- **`infra::mpmc::Channel<T>`** ([src/infra/mpmc.rs](src/infra/mpmc.rs)) — a custom multi-producer/multi-consumer channel. `tx_channel` (outbound bytes) and `rx_channel` (inbound bytes) are both MPMC: each consumer is created up front (`new_consumer`) and Producers are cloned from `Arc<Channel>`. Producers fan out to **all** consumers except the originating id — be careful when adding a new consumer/producer that you don't accidentally re-deliver a message to its sender.

Counts to keep in mind: `app_serial`/`app_rtt` create **3 tx consumers** (interface, plugin engine, graphics) and **2 rx consumers** (plugin engine, graphics). If you add a task that needs to observe these streams, bump the loop and `pop()` an extra consumer in `main.rs`.

### Plugins

Plugins are Lua files (LuaJIT-compatible Lua 5.4 via `mlua`). The `Plugin` struct ([src/plugin/mod.rs](src/plugin/mod.rs)) loads the file, evaluates it to a table, and stores it as global `M`. The plugin engine then calls table functions by name. The Scope-side API surface that plugins call into (`require("scope")`) lives in [src/plugin/bridge.rs](src/plugin/bridge.rs); the user-facing contract is documented in [plugins/README.md](plugins/README.md). Standard libs `scope.lua` and `shell.lua` ship in [plugins/](plugins/) and must sit alongside plugin scripts.

Method calls run on a dedicated thread per call ([src/plugin/method_call.rs](src/plugin/method_call.rs), [src/plugin/thread.lua](src/plugin/thread.lua)) so a slow plugin doesn't block the engine.

### Adding a new top-level interface (e.g. BLE)

The pattern set by Serial/RTT:
1. Add a module under `src/interfaces/` with `<Name>Command`, `<Name>Setup`, `<Name>Shared`, `<Name>Connections`, and an `<Name>Interface::task` function.
2. Add variants to `InterfaceCommand` / `InterfaceShared` / `InterfaceType` ([src/interfaces/mod.rs](src/interfaces/mod.rs)) and a `spawn_<name>_interface` constructor on `InterfaceTask`.
3. Add a `Commands::<Name>` subcommand and an `app_<name>` function in [src/main.rs](src/main.rs) that mirrors `app_serial`/`app_rtt` (channels, consumer counts, task spawn order, `.join()` order).
4. Surface it in the plugin bridge if plugins should drive it.

The `Ble` subcommand is a placeholder — `app_serial`/`app_rtt` are the working templates.
