# Floret 0.2.0 beta 1

Floret is a free, open-source app for making generative art. You connect
boxes (called nodes) on a canvas (shapes, motion, noise, particles, text,
images, sound) and the picture changes live as you edit. No code needed.
This is the first release of Floret as a native app, built first for the
Mac.

## What's in it

- **A quick node editor.** Press Tab to add a node, drag between pins to
  connect them. Every value is a box you can drag to scrub or click to
  type into, and everything can be undone.
- **Patches read left to right**, so you can follow what feeds what.
- **Stay tidy.** Wrap nodes into groups (subpatches), save them to your
  Library and reuse them in any patch.
- **A live preview.** Every Renderer in your patch gets its own preview,
  with a full-screen output for a second display. Panels dock, split and
  float wherever you like.
- **252 nodes** for shapes, type, noise, colour, particles and
  simulations (flocks, slime mould, coral growth, cloth), effects, images,
  video, sound, MIDI, your camera and body tracking. The
  [full list](https://github.com/jaerasgon/Floret/blob/main/NODES.md) says
  what each one does.
- **175 samples to learn from**, including 48 short tutorials that each
  teach one idea, in the Library (File → Library).
- **Find your look.** Roll a new variation with one key, save the ones you
  like as presets, and morph between them.
- **Export** a still as PNG, or a seamless loop as MP4, GIF or a sequence of
  images. A contact sheet of variations helps you pick favourites. On the
  Mac, MP4s include the patch's sound.
- **Your files are yours.** Patches are plain `.flora` files, unsaved work
  is offered back after a crash, and older Floret patches open as they
  were. Light and dark themes.

## Free and open source

Floret is released under the **Apache License 2.0**: free to use and build
on. If you build Floret, or parts of it, into another tool, keep the
`NOTICE` file and credit Floret and its author, Jaafer Elsadig.

If Floret is useful to you, you can help me keep building it with a tip on
[Ko-fi](https://ko-fi.com/jaerasgon). Feedback helps just as much: open an
issue on GitHub or email **Floret.make@gmail.com**. Ideas and bug reports
help most when they come to the project rather than to a fork, so Floret
improves for everyone.

## Known limitations

- The apps aren't signed or notarised, so macOS and Windows ask you to
  confirm the first launch (the steps are above). macOS may ask again for
  camera or microphone access after an update.
- The Mac app is the main target. The Windows and Linux builds are
  experimental and have had far less testing. MP4 sound is Mac-only for
  now.
- Older Floret patches open as they were, except that a Save SVG node is
  left out (a notice says so); save pictures with Export PNG.

## More

- [Beta tester guide](https://github.com/jaerasgon/Floret/blob/main/docs/beta-testing.md):
  what to try, where your files live and how to report a problem.
- [Report a bug](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml)
  or [share an idea](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml).
- [Changelog](https://github.com/jaerasgon/Floret/blob/main/CHANGELOG.md)
