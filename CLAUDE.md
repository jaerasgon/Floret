# Floret — rules for Claude (read before doing anything)

These are the owner's standing rules. They have been broken before; they are
not optional and they override any plan, prompt or convenience. Every
agent / subagent working in this repo must follow them, and every agent
prompt you write must tell the agent to read this file first.

## Golden rules

1. **Patches flow left to right.** In every patch, subpatch and region, a
   node's output connects only to a node to its **right**. Never wire into a
   node behind it (to its left) in the flow. Feedback / memory across frames
   goes **only** through a **Write → Read pair** (the Write at the end of the
   flow, its paired Read at the start). Never build single-node loops
   (`State` / `State Spread` with `next` wired back). The editor refuses
   backward wires; samples are checked by a guard test.

2. **No code in patches.** No `math/expression` (formula) node in any
   Library sample, subpatch or region. Patches are built from visual nodes.
   If a visual node is missing, build a general-purpose node or mode (as few
   nodes as possible, mode dropdowns) and log it in
   `docs/archive/new-nodes-log.md`. The only exception: the GLSL Shader showcase
   samples. Never move logic into GLSL to dodge this rule. (The Expression
   node stays available for users' own patches.)

3. **Read / Write pairs** are matched by their slot index (kept underneath).
   Users can name a pair; naming one names its partner. Creating a Write
   creates its paired Read.

4. **No UI or behaviour changes without the owner's explicit approval.**
   Implement exactly what was approved; list other ideas as suggestions.

5. **Every change must genuinely improve Floret.** Don't copy other apps
   (e.g. Caddis) for their own sake.

6. **Billing: never trigger GitHub Actions** (CI is manual-only; macOS and
   Windows runners cost extra). Test locally.

7. **Git:** the owner tests from `main` with `./floret.sh install`. Only
   fully tested work goes to `main`; work in progress lives on the dev
   branch. Commit and push regularly.

8. **Licensing:** no royalty-bearing codecs we ship ourselves. On macOS,
   video goes through Apple's AVFoundation (no OpenH264 in the Mac build).

9. **Before pushing,** from `native/` with `CARGO_INCREMENTAL=0`:
   `cargo fmt --all --check`,
   `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace --no-fail-fast`.

10. **Releases and attribution.** This file and the `.claude/` folder stay
    in the repository but are never part of a release: `.gitattributes`
    marks them `export-ignore` (source archives and releases leave them
    out). Commits and pull-request descriptions carry no AI attribution
    lines: no `Co-Authored-By`, no session links, no mention of an AI
    assistant. Nothing else that ships may mention one either. New commits
    use the owner's GitHub identity (`jaerasgon
    <jaerasgon@users.noreply.github.com>`): set `git config user.name` and
    `user.email` to it in each fresh clone before committing.

## Terminal commands for the owner

The owner switches between Macs and user accounts, so the clone's folder
differs from machine to machine. Never give a command that assumes a path
or that Terminal is already in the folder. Always give this one line,
which finds the clone (or clones it into `~/github/Floret`), updates
`main` and installs:

```
F=$(find ~ -maxdepth 4 -name floret.sh -path "*Floret*" 2>/dev/null | head -1); if [ -z "$F" ]; then mkdir -p ~/github && git clone https://github.com/jaerasgon/Floret.git ~/github/Floret && F=~/github/Floret/floret.sh; fi; cd "$(dirname "$F")" && git fetch origin && git checkout main && git pull && ./floret.sh install
```

To try another branch, swap both `main`s for that branch's name.

## How the rules are checked

- `native/crates/floret-engine/tests/no_expression_nodes.rs` — no Expression
  node in any Library sample.
- The flow-rule guard test (no backward wire in any scope of any sample; no
  `State` / `State Spread` loops) — next to it in the same folder.
- `native/crates/floret-engine/tests/frozen_ports.rs` — saved-patch node / pin
  / param names never disappear without a migration.
- `native/crates/floret-engine/tests/release_hygiene.rs` — nothing that
  ships (tracked files not marked `export-ignore`) mentions an AI assistant,
  and a source archive of `HEAD` leaves out this file and `.claude/`.
- `native/crates/floret-app/src/licenses.rs` (tests) — every crate in
  `Cargo.lock` is in `native/licenses/crates.md` and is under an allowed
  licence. After changing dependencies run
  `python3 -I tools/gen-licenses.py` and commit the result.
- `docs/archive/plan-feature-complete.md` "Ground rules" — how to add nodes
  and samples (samples, tests, groups, memo flags).
