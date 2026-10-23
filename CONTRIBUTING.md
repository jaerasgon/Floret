# Helping Floret grow

Thank you for caring about Floret. It is free and open source, it is early,
and what you tell me shapes where it goes next.

The best way to help is to **send feedback**: report what breaks, what
confuses you, and what you wish it could do. The licence lets you fork
(see [Licence](#licence)), but I'd be grateful if ideas and fixes came back
to this project, so Floret gets better for everyone.

## How to send feedback

| You want to | Do this |
|---|---|
| Report a bug | [Open a bug report](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml) |
| Share an idea, a wish or how it felt to use | [Open a feedback issue](https://github.com/jaerasgon/Floret/issues/new?template=feedback.yml) |
| Say something privately, or you would rather not use GitHub | Email [Floret.make@gmail.com](mailto:Floret.make@gmail.com) |
| Report a security problem | Please email me rather than opening a public issue: see [SECURITY.md](SECURITY.md) |

You do not need to know how to code. "I was trying to make X and got stuck
at Y" is one of the most useful things you can write.

### What makes a useful bug report

The more of this you include, the faster it gets fixed:

1. **The sample or patch.** The sample's name as the Library shows it (for
   example "Slime Mould"), or your `.flora` file attached to the issue.
   GitHub won't take a `.flora` file as it is, so zip it first.
2. **Steps.** What you did, in order, as exactly as you can.
3. **What happened, and what you expected instead.**
4. **The version.** Shown in Floret → About Floret on a Mac (Help → About Floret on
   Windows and Linux) and in the bottom bar of the window, for example
   `v0.2.0`.
5. **Your system.** Operating system and version, and your computer or
   graphics card if you know it.
6. **A screenshot or screen recording.** Cmd+Shift+4 / Cmd+Shift+5 on a Mac.
7. If Floret showed a "Something went wrong" window, press **Copy details**
   and paste it in.

A patch that loads wrongly, a sample that looks different from its
description, a node whose help text is wrong or unclear: all of these are
bugs worth reporting.

## Pull requests

Pull requests are welcome, with one request: **open an issue first** and
describe what you would like to change. Floret has firm design rules (below),
and a quick conversation saves you from writing something I can't merge. Small fixes such as a typo or an obvious bug can go straight to a
pull request.

A pull request is easiest to review when it is small, does one thing, says
how you tested it, and links its issue. The pull request form asks for these.

### Design rules

These keep patches readable and keep Floret consistent. Changes have to
follow them.

- **Patches flow left to right.** In every patch, subpatch and region, a
  node's output connects only to a node on its right. The editor refuses
  backward wires. Memory across frames goes through a **Write / Read pair**
  (the Write at the end of the flow, its paired Read at the start), never
  through a node wired back into itself.
- **No code in patches.** The built-in samples, subpatches and regions are
  built from visual nodes, not formulas. If a visual node is missing, the
  fix is a general-purpose node or a mode on an existing node, not a
  formula. (The Expression node stays available for your own patches. The
  GLSL Shader samples are the one exception.)
- **Saved patches keep working.** A node, pin or parameter name that appears
  in saved `.flora` files must not disappear without a migration.
- **No surprise changes to the interface or behaviour.** Changes to how the
  app looks or acts are agreed in an issue before they are built.
- **Every change should genuinely improve Floret**, not just copy another
  tool's feature.
- **No royalty-bearing codecs shipped by Floret.** On macOS, video goes
  through Apple's AVFoundation; Windows and Linux use OpenH264.

### Build and test from source

You need the stable [Rust](https://rustup.rs) toolchain. On Linux, also
`libasound2-dev` (ALSA). From the repository root:

```sh
cd native
cargo build --release -p floret-app      # the app: native/target/release/floret
```

Before you open a pull request, from `native/`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
```

All three must pass. On a Mac, `./floret.sh` builds and runs Floret from the
repository, and `./floret.sh install` puts Floret.app in Applications.

New nodes need a row in [NODES.md](NODES.md) and a sample or test that uses
them; new samples need an entry in `native/samples/catalogue.json`. The
tests tell you what is missing.

## Licence

Floret is released under the [Apache License 2.0](LICENSE). By sending a
pull request you agree that your contribution is licensed under the same
terms (section 5 of the licence).

If you do fork Floret or build parts of it into another tool, the licence
asks you to keep the [NOTICE](NOTICE) file with it and credit Floret and its
author, Jaafer Elsadig.

## Be kind

Everyone here is learning, and most people are artists first. Please follow
the [Code of Conduct](CODE_OF_CONDUCT.md).
