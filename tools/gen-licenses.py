#!/usr/bin/env python3
"""Generate the third-party Rust crate licence files shown in Help -> Licenses.

Run from anywhere (the script finds the repository root itself):

    python3 -I tools/gen-licenses.py            # rewrite native/licenses/*
    python3 -I tools/gen-licenses.py --offline  # use only the local cargo cache
    python3 -I tools/gen-licenses.py --check    # fail if the committed files are stale

It reads `cargo metadata --format-version 1 --locked` (so it follows
native/Cargo.lock exactly) for the macOS (aarch64 and x86_64), Linux and
Windows targets, and writes:

  native/licenses/crates.md          one row per third-party crate: name,
                                     version, SPDX licence, where it is used,
                                     repository
  native/licenses/licence-texts.txt  every licence text once (from
                                     tools/license-texts/), then the copyright
                                     notices the crates ship, de-duplicated

Floret's own workspace crates are left out. The app embeds both files with
`include_str!`; a test in floret-app (`licenses.rs`) checks that every
package in Cargo.lock is listed and that every licence expression can be
satisfied from its allow-list. Re-run this script whenever Cargo.lock
changes. It needs only the Python standard library and cargo.
"""
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NATIVE = os.path.join(ROOT, "native")
OUT_DIR = os.path.join(NATIVE, "licenses")
TEXTS = os.path.join(ROOT, "tools", "license-texts")

# (label, rust target)
TARGETS = [
    ("macOS", "aarch64-apple-darwin"),
    ("macOS", "x86_64-apple-darwin"),
    ("Linux", "x86_64-unknown-linux-gnu"),
    ("Windows", "x86_64-pc-windows-msvc"),
]

# Licence-file names inside a crate (case-insensitive prefixes).
FILE_RE = re.compile(r"^(LICEN[CS]E|COPYING|UNLICENSE|COPYRIGHT|NOTICE)", re.I)

# SPDX exceptions that may follow WITH.
EXCEPTIONS = {"LLVM-exception"}


def cargo_metadata(offline, target=None):
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if offline:
        cmd.append("--offline")
    if target:
        cmd += ["--filter-platform", target]
    out = subprocess.run(
        cmd, cwd=NATIVE, check=True, stdout=subprocess.PIPE, text=True
    ).stdout
    return json.loads(out)


def allowed_atoms():
    """Licence ids we hold a text for (tools/license-texts/<id>.txt)."""
    ids = set()
    for f in os.listdir(TEXTS):
        if f.endswith(".txt"):
            ids.add(f[:-4])
    return ids


def parse_spdx(expr):
    """SPDX expression -> nested ('or'|'and', [..]) / ('id', name, exception)."""
    toks = re.findall(r"\(|\)|/|[A-Za-z0-9.+-]+", expr)
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def take():
        nonlocal pos
        t = toks[pos]
        pos += 1
        return t

    def parse_or():
        parts = [parse_and()]
        while peek() in ("OR", "or", "/"):
            take()
            parts.append(parse_and())
        return parts[0] if len(parts) == 1 else ("or", parts)

    def parse_and():
        parts = [parse_atom()]
        while peek() in ("AND", "and"):
            take()
            parts.append(parse_atom())
        return parts[0] if len(parts) == 1 else ("and", parts)

    def parse_atom():
        t = take()
        if t == "(":
            e = parse_or()
            assert take() == ")", expr
            return e
        exc = None
        if peek() == "WITH":
            take()
            exc = take()
        return ("id", t, exc)

    tree = parse_or()
    assert pos == len(toks), "trailing tokens in " + expr
    return tree


def satisfy(tree, allowed):
    """Allowed atoms used if the expression can be met from `allowed`,
    else None. OR takes every allowed alternative that is itself
    satisfiable (so each one's text is shipped)."""
    kind = tree[0]
    if kind == "id":
        _, name, exc = tree
        if name not in allowed:
            return None
        if exc is not None and exc not in EXCEPTIONS:
            return None
        return {name} | ({exc} if exc else set())
    parts = [satisfy(t, allowed) for t in tree[1]]
    if kind == "and":
        if any(p is None for p in parts):
            return None
        return set().union(*parts)
    ok = [p for p in parts if p is not None]
    return set().union(*ok) if ok else None


def reachable(meta):
    """Package ids reachable from the workspace members through normal and
    build dependencies of the (platform-filtered) resolve graph."""
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen = set()
    todo = list(meta["workspace_members"])
    while todo:
        i = todo.pop()
        if i in seen or i not in nodes:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            kinds = [k.get("kind") for k in d["dep_kinds"]]
            if any(k in (None, "build") for k in kinds):
                todo.append(d["pkg"])
    return seen


def clean(text):
    text = text.replace("\r\n", "\n").replace("\r", "\n").lstrip("﻿")
    text = text.replace("\t", "    ").replace("\f", "")
    lines = [ln.rstrip() for ln in text.split("\n")]
    while lines and not lines[-1]:
        lines.pop()
    while lines and not lines[0]:
        lines.pop(0)
    return "\n".join(lines)


def norm_key(text):
    return hashlib.sha1(" ".join(text.split()).encode()).hexdigest()


def wrap_names(names, width=96, indent="  "):
    lines, cur = [], indent
    for i, n in enumerate(names):
        piece = n + ("," if i + 1 < len(names) else "")
        if len(cur) + len(piece) + 1 > width and cur.strip():
            lines.append(cur.rstrip())
            cur = indent
        cur += piece + " "
    lines.append(cur.rstrip())
    return "\n".join(lines)


def build(offline):
    meta = cargo_metadata(offline)
    ws = set(meta["workspace_members"])
    pkgs = [p for p in meta["packages"] if p["id"] not in ws]
    allowed = allowed_atoms()

    # Where each package is compiled in.
    used_on = {p["id"]: set() for p in pkgs}
    for label, target in TARGETS:
        reach = reachable(cargo_metadata(offline, target))
        for i in reach:
            if i in used_on:
                used_on[i].add(label)

    rows, atoms_used, problems = [], set(), []
    for p in sorted(pkgs, key=lambda p: (p["name"], p["version"])):
        lic = p.get("license")
        if not lic:
            problems.append(f"{p['name']} {p['version']}: no licence field")
            continue
        used = satisfy(parse_spdx(lic), allowed)
        if used is None:
            problems.append(f"{p['name']} {p['version']}: {lic}")
            continue
        atoms_used |= {a for a in used if a in allowed}
        where = used_on[p["id"]]
        if len(where) == 3:
            where_s = "all"
        elif not where:
            where_s = "other targets"
        else:
            where_s = ", ".join(x for x in ("macOS", "Linux", "Windows") if x in where)
        repo = p.get("repository") or p.get("homepage") or "-"
        rows.append((p["name"], p["version"], lic, where_s, repo))
    if problems:
        sys.exit(
            "Licence outside the allow-list (tools/license-texts/):\n  "
            + "\n  ".join(problems)
        )

    def cell(s):
        return s.replace("|", "\\|")

    crates = [
        "# Rust crates",
        "",
        "Floret is built from the open-source Rust crates listed here, each used under",
        "its own licence. Where a crate offers a choice (`OR`), Floret uses it under",
        "the permissive option. Floret's own crates are not listed. The full licence",
        "texts and the copyright notices the crates ship are in `licence-texts.txt`.",
        "",
        "Generated by `tools/gen-licenses.py` from `cargo metadata` (native/Cargo.lock);",
        "do not edit by hand. %d crates." % len(rows),
        "",
        "| Crate | Version | Licence | Used on | Repository |",
        "| --- | --- | --- | --- | --- |",
    ]
    for r in rows:
        crates.append("| " + " | ".join(cell(c) for c in r) + " |")
    crates_md = "\n".join(crates) + "\n"

    # Licence texts, each once.
    out = [
        "LICENCE TEXTS",
        "=============",
        "",
        "Part 1 gives each licence that the Rust crates are used under, once.",
        "Part 2 gives the copyright notices and licence files the crates ship with,",
        "grouped where several crates share the same text.",
        "",
        "",
        "PART 1: THE LICENCES",
        "====================",
    ]
    for a in sorted(atoms_used):
        users = sum(1 for r in rows if re.search(r"(?<![A-Za-z0-9.+-])%s(?![A-Za-z0-9+-])" % re.escape(a), r[2]))
        with open(os.path.join(TEXTS, a + ".txt"), encoding="utf-8") as f:
            text = clean(f.read())
        out += ["", "", "-" * 78, f"{a}  (named by {users} crate version(s))", "-" * 78, "", text]
    canonical = {norm_key(clean(open(os.path.join(TEXTS, a + ".txt"), encoding="utf-8").read())) for a in atoms_used}

    groups = {}
    for p in sorted(pkgs, key=lambda p: (p["name"], p["version"])):
        d = os.path.dirname(p["manifest_path"])
        names = sorted(f for f in os.listdir(d) if FILE_RE.match(f) and os.path.isfile(os.path.join(d, f)))
        lf = p.get("license_file")
        if lf and os.path.basename(lf) not in names and os.path.isfile(os.path.join(d, lf)):
            names.append(lf)
        for fn in names:
            with open(os.path.join(d, fn), encoding="utf-8", errors="replace") as f:
                text = clean(f.read())
            if not text:
                continue
            k = norm_key(text)
            if k in canonical:
                continue  # identical to a Part 1 text
            g = groups.setdefault(k, {"text": text, "users": []})
            tag = f"{p['name']} {p['version']}"
            if tag not in g["users"]:
                g["users"].append(tag)

    out += [
        "",
        "",
        "PART 2: NOTICES SHIPPED WITH THE CRATES",
        "=======================================",
        "",
        "Crates whose licence file is the plain text of a Part 1 licence are not",
        "repeated here; their copyright holders are named in the repository links",
        "of crates.md.",
    ]
    for k in sorted(groups, key=lambda k: (groups[k]["users"][0], k)):
        g = groups[k]
        out += [
            "",
            "",
            "-" * 78,
            "Used by:",
            wrap_names(g["users"]),
            "-" * 78,
            "",
            g["text"],
        ]
    texts_txt = "\n".join(out) + "\n"
    return {"crates.md": crates_md, "licence-texts.txt": texts_txt}, len(rows)


def main():
    args = sys.argv[1:]
    files, n = build("--offline" in args)
    if "--check" in args:
        stale = []
        for name, content in files.items():
            path = os.path.join(OUT_DIR, name)
            try:
                with open(path, encoding="utf-8") as f:
                    if f.read() != content:
                        stale.append(name)
            except FileNotFoundError:
                stale.append(name)
        if stale:
            sys.exit("stale: %s (run tools/gen-licenses.py)" % ", ".join(stale))
        print("licence files are up to date (%d crates)" % n)
        return
    os.makedirs(OUT_DIR, exist_ok=True)
    for name, content in files.items():
        with open(os.path.join(OUT_DIR, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(content)
        print("wrote native/licenses/%s (%d bytes)" % (name, len(content.encode())))
    print("%d crates" % n)


if __name__ == "__main__":
    main()
