# Building Floret from source

Most people should download Floret from the
[Releases page](https://github.com/jaerasgon/Floret/releases). This page is for
building it yourself or working on the code.

## On a Mac

Paste this one line into Terminal. It finds your copy of Floret (or clones it
into `~/github/Floret`), updates it and installs **Floret.app** into
Applications. The first run installs Rust and builds Floret, which takes
several minutes. Run the same line again whenever you want to update.

```sh
F=$(find ~ -maxdepth 4 -name floret.sh -path "*Floret*" 2>/dev/null | head -1); if [ -z "$F" ]; then mkdir -p ~/github && git clone https://github.com/jaerasgon/Floret.git ~/github/Floret && F=~/github/Floret/floret.sh; fi; cd "$(dirname "$F")" && git fetch origin && git checkout main && git pull && ./floret.sh install
```

If Apple's command line tools aren't installed yet, a dialog offers them:
install them, then run the line again.

## On any system

With the stable [Rust](https://rustup.rs) toolchain (and `libasound2-dev` on
Linux):

```sh
git clone https://github.com/jaerasgon/Floret.git
cd Floret/native
cargo build --release -p floret-app
```

The app is `native/target/release/floret`. `./floret.sh` in the repository
root builds and runs it in one step, on a Mac or on Linux.

## Project layout

| Path | What it is |
|---|---|
| `native/` | **The app.** A Rust workspace (`native/Cargo.toml`). |
| `native/crates/floret-app` | The desktop app: window, node editor, panels, menus, files, export (egui + wgpu). |
| `native/crates/floret-engine` | The dataflow engine: loads and compiles patches, runs the nodes each frame. No GPU, UI or OS code. |
| `native/crates/floret-render` | The GPU renderer: turns the engine's scenes into pixels (wgpu). |
| `native/crates/floret-media` | The real-time audio engine behind the sound nodes. |
| `native/crates/floret-input` | MIDI, cameras, and hand / face / pose tracking. |
| `native/crates/floret-video` | Video playback and MP4 / GIF export. |
| `native/crates/floret-open-files` | Opening `.flora` files from Finder (macOS). |
| `native/samples/` | The built-in samples and their catalogue. |
| `floret.sh` | Build, run and install script. |
| `docs/` | The beta tester guide, the Mac QA checklist, how releases are made ([releasing.md](releasing.md)), the release notes and `docs/archive/` (old planning notes and logs, kept for history). |
| `web-legacy/` | The earlier web version of Floret, kept for reference only. It isn't the app anymore. |

## Checking your changes

From `native/`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

See [CONTRIBUTING.md](../CONTRIBUTING.md) for how to send a change, and
[docs/qa-checklist-mac.md](qa-checklist-mac.md) for the manual test list used
on the Mac build.
