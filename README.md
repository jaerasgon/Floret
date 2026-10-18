# Floret

**Make generative art by connecting boxes.** Floret is a free, open-source
tool where you wire shapes, motion, noise, particles, text, images and sound
together, and watch the picture change live as you edit. No code needed.

![Floret with the Kaleido Slime sample open: the patch on the node canvas, its live preview on the right, the Inspector below](docs/images/floret-window.png)

Every box (a *node*) does one small thing: draw a shape, add noise, pick a
colour, move something in a loop. Connect them and you have a picture, a
pattern, a particle storm or a moving poster. Change any number while it
plays and see the result straight away. When you love it, save a still, a
seamless loop or a whole sheet of variations.

Floret is **in beta**: it works, it is growing, and your feedback shapes it.

## What you can make

- **Patterns and textures:** flow fields, Voronoi cells, tiles, wallpaper
  symmetry, kaleidoscopes, stippling.
- **Living systems:** flocks, slime mould, coral growth, cloth, reaction
  diffusion.
- **Particles, trails and glow:** emitters, forces, bloom, grain, halftone.
- **Type and motion:** variable fonts, kinetic type, loops and easing.
- **Things that react:** to sound, MIDI, your camera, your hands and face.

You don't start from nothing: Floret comes with **175 sample patches**,
including **48 short tutorials** that each teach one idea.

## A look inside

| | |
|---|---|
| ![The Weather Map sample: a node graph of subpatches, a ForEach region and Vector Math nodes on the left, and on the right the live preview of a pressure map with coloured bands, isobars, wind flow lines and arrows](docs/images/floret-system.png) | ![The Flock Herder sample: the Simulation, Palette, Connect Points and subpatch nodes on the left, and on the right the live preview of colourful flocks of boids with trails](docs/images/floret-particles.png) |
| **A complex system: Weather Map.** Many small parts wired into a living pressure map. | **A particles sample: Flock Herder.** 320 swimming creatures that follow, gather and leave trails. |
| ![The Library: every sample by category, with search](docs/images/floret-library.png) | ![Floret with several panels docked: the Generative Poster sample with the canvas, Library, live preview and Presets](docs/images/floret-panels.png) |
| **The Library.** Open any sample and take it apart. | **Your layout.** Panels dock, split and float wherever you like. |

## Why you might like it

- **Play, don't wait.** Every value is a box you can drag or type into while
  the patch runs. Everything can be undone.
- **Easy to read.** Patches flow left to right, like a sentence, so you can
  follow what feeds what.
- **Stay tidy.** Wrap nodes into groups (subpatches), save them to your
  Library, and reuse them in any patch.
- **Find your look.** Roll new variations with one key, save the ones you
  like as presets, and morph between them.
- **Share it.** Export PNG stills, seamless loops as MP4, GIF or image
  sequences, or a contact sheet of variations.
- **Light and dark themes**, and your patches are plain files you own.

## Download

Get Floret from the **[Releases page](https://github.com/jaerasgon/Floret/releases)**.

| | |
|---|---|
| **Mac** (macOS 11 or later, Apple Silicon and Intel) | The most tested. |
| **Windows** | Experimental. |
| **Linux** | Experimental. Needs ALSA (`libasound2`). |

Floret isn't signed yet, so your system asks you to confirm the first launch.
The Releases page has the two or three clicks for each system. Camera and
microphone data stay on your computer.

Want to build it yourself? See [docs/building.md](docs/building.md).

## Try it in two minutes

1. Open Floret and choose **File, then Samples Library**.
2. Open **01 · Hello Rect**, then the other tutorials in order. Each has a
   short note on the canvas.
3. Click any number on a node and drag it. Then press **Tab** to add a node of
   your own.

## Feedback and help

Floret is young, and the most useful thing you can do is tell me what
happens when you use it: what worked, what confused you, what you wished
it could do.

- [Report a bug](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml)
  or [share an idea](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml)
- Or email **[Floret.make@gmail.com](mailto:Floret.make@gmail.com)**
- A `.flora` file that misbehaves, plus a sentence about what you expected,
  is perfect.

Please send ideas here rather than forking, so Floret gets better for
everyone. [CONTRIBUTING.md](CONTRIBUTING.md) has the details.

## Support Floret

Floret is free and will stay free. If it's useful to you and you'd like to
help me keep building it, you can leave a tip on
[Ko-fi](https://ko-fi.com/jaerasgon). Feedback helps just as much.

## Licence

Free and open source under the **[Apache License 2.0](LICENSE)**: use it,
change it, build on it. If you build Floret into another tool, keep the
[NOTICE](NOTICE) file and credit Floret and its author, Jaafer Elsadig.
Third-party licences are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
and in the app under Help, then Licenses.

## More

[All 252 nodes](NODES.md) · [Beta tester guide](docs/beta-testing.md) ·
[Changelog](CHANGELOG.md) · [Security](SECURITY.md) ·
[Code of conduct](CODE_OF_CONDUCT.md)
