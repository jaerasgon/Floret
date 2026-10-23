#!/usr/bin/env python3
"""Keep the node tables in NODES.md in step with the node catalogue.

    python3 -I tools/nodes-doc-check.py          # list what is out of step
    python3 -I tools/nodes-doc-check.py --fix    # rewrite titles and pin cells

The truth is native/crates/floret-engine/src/nodes.json. For every table row
of NODES.md this checks

  * the title: `**Label** · `type``, with the node's display name;
  * the pins cell, `inputs → outputs`, one side at a time:
      - a side with fixed pins gets a generated cell (--fix rewrites it);
      - a side whose pins change with a setting (dynamic inputs / outputs)
        stays hand-written, and must still name each of its default pins;
  * a few leftovers from the web version ("Native-only.", TypeScript, ...).

Pin format (what a user sees on the node; the name saved in patch files
follows in brackets only where it is different):

    `pos: vec2, w, h: number, anchor: string` → `layer (shape): layer`

Names that share a type are grouped, `*` after a type marks a spread pin,
`—` means no pins. Descriptions are never touched.

The same rules are enforced by the test `nodes_md_documents_every_visible_node`
in native/crates/floret-app/src/node_help.rs.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NODES_JSON = ROOT / "native/crates/floret-engine/src/nodes.json"
NODES_MD = ROOT / "NODES.md"

# Phrases from the web version that must not come back.
STALE = ["Native-only", "TypeScript", "React", "p5.", "src/nodes", "Math.min", "Math.max"]


def display(p, is_input):
    """The pin's name as the app shows it (editor/values.rs `display_label`)."""
    if p.get("label"):
        return p["label"]
    if not is_input and p["name"] in ("result", "number") and p["type"] != "boolean":
        return "value"
    if is_input and p["type"] == "audio":
        return "audio"
    if p["type"] == "layer":
        main_out = (
            not is_input
            and p["kind"] != "spread"
            and p["name"] not in ("viz", "guides", "inner", "outer")
        )
        if p["name"] == "layers" or main_out:
            return "layer"
    return p["name"]


def side(pins, is_input):
    """`a, b: number, c: layer*` for one side of the arrow."""
    if not pins:
        return "—"
    groups = []  # [(type-with-star, [names])]
    for p in pins:
        shown = display(p, is_input)
        name = shown if shown == p["name"] else f"{shown} ({p['name']})"
        ty = p["type"] + ("*" if p["kind"] == "spread" else "")
        if groups and groups[-1][0] == ty:
            groups[-1][1].append(name)
        else:
            groups.append((ty, [name]))
    return "`" + ", ".join(", ".join(n) + ": " + t for t, n in groups) + "`"


def split_arrow(cell):
    """Split `inputs → outputs` at the first arrow that is outside backticks."""
    ticks = False
    for i, ch in enumerate(cell):
        if ch == "`":
            ticks = not ticks
        elif ch == "→" and not ticks:
            return cell[:i].strip(), cell[i + 1 :].strip()
    return None


def mentions(text, name):
    return re.search(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])", text)


def split_row(line):
    parts = re.split(r"(?<!\\)\|", line.rstrip("\n"))
    return [c.strip() for c in parts[1:-1]]


def main():
    fix = "--fix" in sys.argv[1:]
    catalogue = {n["type"]: n for n in json.loads(NODES_JSON.read_text())["nodes"]}
    lines = NODES_MD.read_text().split("\n")
    problems = []
    changed = 0
    seen = set()
    for i, line in enumerate(lines):
        where = f"line {i + 1}"
        for word in STALE:
            if word in line:
                problems.append(f"{where}: web-era wording ({word!r})")
        if not line.startswith("| **"):
            continue
        cells = split_row(line)
        m = re.match(r"\*\*(.*?)\*\* · `(.*?)`$", cells[0])
        if not m or len(cells) < 3:
            problems.append(f"{where}: row isn't | **Label** · `type` | pins | description |")
            continue
        ty = m.group(2)
        node = catalogue.get(ty)
        if node is None:
            problems.append(f"{where}: {ty} is not in nodes.json")
            continue
        seen.add(ty)
        new = list(cells)
        title = f"**{node['label']}** · `{ty}`"
        if cells[0] != title:
            problems.append(f"{where}: {ty} title {cells[0]} should be {title}")
            new[0] = title
        sides = split_arrow(cells[1])
        if sides is None:
            problems.append(f"{where}: {ty} pins cell has no arrow")
            sides = ("", "")
        fixed = []
        for is_input, pins, dynamic, got in (
            (True, node["inputs"], node.get("dynamicInputs"), sides[0]),
            (False, node["outputs"], node.get("dynamicOutputs"), sides[1]),
        ):
            word = "inputs" if is_input else "outputs"
            if dynamic:
                # Hand-written: it only has to name each default pin.
                fixed.append(got)
                for p in pins:
                    if not (mentions(got, p["name"]) or mentions(got, display(p, is_input))):
                        problems.append(
                            f"{where}: {ty} {word} change with a setting; "
                            f"the cell doesn't mention {display(p, is_input)!r}"
                        )
            else:
                want = side(pins, is_input)
                fixed.append(want)
                if got != want:
                    problems.append(f"{where}: {ty} {word} {got} should be {want}")
        new[1] = fixed[0] + " → " + fixed[1]
        if new != cells:
            lines[i] = "| " + " | ".join(new) + " |"
            changed += 1
    for node in catalogue.values():
        if not node.get("hidden") and node["type"] not in seen:
            problems.append(f"no row for visible node {node['type']}")
    if fix:
        NODES_MD.write_text("\n".join(lines))
        print(f"rewrote {changed} rows")
        # What --fix can't mend (hand-written cells, stale words) still counts.
        problems = [p for p in problems if " should be " not in p]
    for p in problems:
        print(p)
    print(f"{len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
