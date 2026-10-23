# Changelog

What changed in each release of Floret, newest first, in the
[Keep a Changelog](https://keepachangelog.com) style. Floret uses 0.x
version numbers while it is in beta, so things may still change.

- The longer version of 0.2.0, with every group of nodes and every part of
  the editor: [docs/release-0.2.0-details.md](docs/release-0.2.0-details.md)
- The history of the earlier web version, from the first plan on:
  [docs/archive/CHANGELOG-web-era.md](docs/archive/CHANGELOG-web-era.md)

## [0.2.0-beta.1] - 2026-10-18

The first release of Floret as a native desktop app, written in Rust and
drawn on the GPU. It replaces the earlier web version, which is kept in the
repository for reference (`web-legacy/`). Published as a beta:
[v0.2.0-beta.1](https://github.com/jaerasgon/Floret/releases/tag/v0.2.0-beta.1).

### Added

- A node editor built for live work: press Tab to add a node, drag between
  pins to connect them, drag or type into every value, undo everything.
  Patches read left to right.
- Groups of nodes (subpatches) that you can save to the Library and reuse,
  and regions to repeat, switch or cache part of a patch.
- A live GPU preview for every Renderer, with focus mode and a full-screen
  output window. Panels dock, split and float; light and dark themes.
- 252 nodes in the Nodes browser: shapes and paths, type, noise, colour,
  particles and simulations (flocking, slime mould, coral growth, cloth,
  reaction diffusion), effects and shaders, images and video, sound, MIDI,
  camera and body tracking. See [NODES.md](NODES.md).
- 177 samples in the Library, including 48 short tutorials.
- Seeds, variations and presets: roll a new variation with one key, save
  presets and morph between them.
- Export: PNG stills up to 4×, seamless loops as MP4, GIF or image
  sequences, and Export Variations with a contact sheet. MP4s include the
  patch's sound on macOS.
- `.flora` files with autosave and crash recovery. Patches from the web
  version open as they were.
- Downloads for macOS (Apple silicon and Intel), plus experimental
  downloads for Windows and Linux, with checksums.
- Free and open source under the Apache License 2.0.

### Changed

- The app is no longer the web version: it is a native app. A Save SVG node
  in an older patch is left out when it opens (a notice says so); save
  pictures with Export PNG.

### Known limitations

- The apps are not signed or notarised, so macOS and Windows ask you to
  confirm the first launch.
- The Windows and Linux builds are experimental and have had far less
  testing than the Mac app. MP4 sound is Mac-only for now.
- One patch per window, one window.

[0.2.0-beta.1]: https://github.com/jaerasgon/Floret/releases/tag/v0.2.0-beta.1
