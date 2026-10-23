# Releasing Floret

Downloadable builds for macOS, Windows and Linux are made by one manual
GitHub Actions workflow, `.github/workflows/release.yml`. It builds each
platform, packages it with the scripts in `tools/package/`, and attaches the
files to a GitHub Release. Nothing runs by itself: it only starts when you
press Run workflow.

## Cut a release

1. Get the commit you want to release onto GitHub (usually `main`). The
   release points at the exact commit the workflow runs on. The workflow
   file itself has to be on the repository's default branch for the Run
   workflow button to appear.
2. Check the notes in `docs/release-notes.md`. They become the body of
   the release, below an automatic header with the download table and the
   first-launch steps, so the notes don't repeat those. (Its first `#`
   heading is dropped; the release title says it.)
3. On GitHub: **Actions → release → Run workflow**. Choose the branch, then:
   - **tag**: for example `v0.2.0-beta.1`. It is a `v`, then three numbers
     with dots (`0.2.0`), then optionally a dash and a label such as
     `-beta.1`. The tag is created at the commit being built when the
     release is published.
   - **prerelease**: on for betas.
   - **draft**: on, so you can look at it before anyone else can.
   - **run_tests**: runs `cargo test --workspace` on Linux first (off by
     default; run the tests locally before releasing).
4. Wait. A full run takes roughly 30 to 60 minutes; the Mac and Windows jobs
   are the slow ones.
5. Open **Releases**. The draft is there with every file, `SHA256SUMS.txt`
   and the notes. Download one or two files and try them. Edit the notes if
   you like.
6. Press **Publish release**.
7. Update the download links in the **Download** section of `README.md` to the
   new tag and file names (the release page and each file link contain the
   tag and version). Merge that change to `main`.

Running it again with the same tag updates the release instead of failing:
the notes are rewritten and files with the same name are replaced. If the
tag already exists on a different commit, the run stops with an error: use a
new tag (for example `v0.2.0-beta.2`), or delete the old tag and release.

If the Apple-silicon Mac build fails, no release is made. A failure in the
Intel Mac, Windows or Linux build does not stop the release: that row of the
download table says "not built in this run", and the run shows a warning.
Open the failed job to see why, fix it, and run the workflow again with the
same tag.

## What the files are

| File | Made by |
| --- | --- |
| `Floret-<version>-macos-arm64.dmg` / `.zip` | `tools/package/macos.sh` (Apple silicon) |
| `Floret-<version>-macos-x86_64.dmg` / `.zip` | `tools/package/macos.sh` (Intel) |
| `Floret-<version>-windows-x86_64.zip` | `tools/package/windows.sh` |
| `Floret-<version>-linux-x86_64.tar.gz` | `tools/package/linux.sh` |
| `SHA256SUMS.txt` | the release job, for all of the above |

`<version>` is the tag without the `v`. Every package carries `LICENSE`,
`NOTICE`, `THIRD_PARTY_NOTICES.md` and the licence lists
(`native/licenses/`). The binary embeds the samples, fonts,
models and node reference, so nothing else is needed.

### Names that don't change

Each download is also attached with the version left out of its name. These
are identical copies (same bytes) of the versioned files, so a link to one
never needs editing when a new release comes out:

| Stable name | Same file as |
| --- | --- |
| `Floret-macos-arm64.dmg` / `.zip` | `Floret-<version>-macos-arm64.dmg` / `.zip` |
| `Floret-macos-x86_64.dmg` / `.zip` | `Floret-<version>-macos-x86_64.dmg` / `.zip` |
| `Floret-windows-x86_64.zip` | `Floret-<version>-windows-x86_64.zip` |
| `Floret-linux-x86_64.tar.gz` | `Floret-<version>-linux-x86_64.tar.gz` |

The packaging scripts write both copies, and `SHA256SUMS.txt` lists both.
The download table in the release notes names the versioned files. A link
to a stable name on one specific release looks like
`https://github.com/jaerasgon/Floret/releases/download/v0.2.0-beta.1/Floret-macos-arm64.dmg`.
GitHub's `releases/latest/download/<name>` form follows the newest
**published release that is not marked pre-release**, so it only works once
a full release exists.

The Mac bundle is the same one `./floret.sh install` makes: both call
`tools/package/build-macos-app.sh`, so there is one place that knows what
goes in `Floret.app`.

## Making a package on your own machine

Each script builds the app and writes into `dist/` (git-ignored), including a
`SHA256SUMS` line per file:

```
tools/package/macos.sh   [--profile NAME] [--skip-build] [VERSION]   # on a Mac
tools/package/linux.sh   [--profile NAME] [--skip-build] [VERSION]   # on Linux
tools/package/windows.sh [--profile NAME] [--skip-build] [VERSION]   # in Git Bash
```

`VERSION` defaults to the version in `native/Cargo.toml`. This is how to test
a script without waiting for the workflow. (Building on GitHub's runners is
the way to get the Windows and Intel Mac files.)

## Checking a download

Each release has `SHA256SUMS.txt`. Put it in the same folder as the files you
downloaded, then:

- macOS: `shasum -a 256 -c SHA256SUMS.txt --ignore-missing`
- Linux: `sha256sum -c SHA256SUMS.txt --ignore-missing`
- Windows (PowerShell): `Get-FileHash .\Floret-<version>-windows-x86_64.zip`
  and compare with the line in the file.

The checksum shows the file arrived intact. It does not prove who made it:
the apps are not signed.

## First launch of an unsigned app

None of the builds is signed or notarised (no Apple Developer ID, no Windows
code-signing certificate), so each system asks you to confirm. The packages
carry a short note saying the same (`READ ME FIRST.txt` on the Mac,
`README.txt` on Windows and Linux).

- **macOS.** Drag Floret.app into Applications and open it. macOS refuses.
  Open **System Settings → Privacy & Security**, scroll to the message about
  Floret and click **Open Anyway**. (On macOS 14 and earlier, right-click the
  app, Open, Open.) Or remove the download mark in Terminal:
  `xattr -dr com.apple.quarantine /Applications/Floret.app`. The app has an
  ad-hoc signature, which is what lets it run on Apple silicon at all, but
  macOS still treats it as from an unidentified developer. Camera and
  microphone access are asked for as usual.
- **Windows.** Unzip the folder and run `floret.exe`. On the blue SmartScreen
  window ("Windows protected your PC") click **More info**, then **Run
  anyway**.
- **Linux.** Unpack and run `./floret`. Needs `libasound2` and a graphics
  driver with Vulkan or OpenGL. `README.txt` shows how to add it to the
  applications menu.

## Known limits

- **Unsigned.** The prompts above stay until the Mac app is signed with a
  Developer ID and notarised (and Windows gets a code-signing certificate).
  Both cost money and are not set up.
- **Windows and Linux are experimental.** They build and package, but have
  had far less testing than the Mac app. The Windows exe has no icon of its
  own, and `.flora` files are not associated with Floret on Windows or
  Linux (open them from File → Open or by dragging onto the app).
- **Linux:** only x86_64, built on Ubuntu 22.04 so it runs on distributions
  with glibc 2.35 or newer. Wayland and X11 are both built in; sound needs
  the ALSA library at run time. No AppImage, deb or rpm yet.
- **Windows:** the exe needs the Microsoft Visual C++ runtime, which almost
  every Windows 10 / 11 PC already has.
- **Video export** uses Apple's AVFoundation on the Mac and the bundled
  OpenH264 encoder on Windows and Linux.
- The first run of the workflow on a runner starts with an empty build cache
  and is the slowest; later runs of the same branch are faster.
- A Mac build number comes from the commit count, so the workflow checks out
  the full history.
