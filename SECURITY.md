# Security

If you find a security problem in Floret, please report it **privately by
email** to [Floret.make@gmail.com](mailto:Floret.make@gmail.com), not as a
public issue. That gives me time to fix it before it is widely known.

Include what you found, how to reproduce it (a patch file helps), the
Floret version, and your operating system. You will get a reply as soon as
I can manage; Floret is a one-person project, so please be patient.

## What is in scope

- **Opening files.** A `.flora` patch, an image, a video, a font, a CSV
  file or a library file that crashes Floret, reads or writes files it
  should not, or runs something it should not.
- **Network use.** Floret fetches images from URLs when a patch asks for
  them; anything that lets a patch reach further than that.
- **Shaders and the GPU.** A shader or patch that can hang or crash the
  machine rather than just the app.
- **Exports and saved files.** Writing outside the folder you chose.
- **Camera, microphone and tracking.** Anything that sends video or sound
  off your computer. Floret processes these on-device and should never
  upload them.
- **The build.** Problems in `floret.sh` or the dependencies Floret ships.

## What is not

- Ordinary bugs, crashes on bad input that do no harm, and slow patches:
  please use the [bug report form](https://github.com/jaerasgon/Floret/issues/new?template=bug_report.yml).
- Problems in the earlier web version, which is kept in `web-legacy/` for
  reference only.
- Vulnerabilities in a dependency that do not affect Floret.

## Supported versions

Floret is in beta. Fixes go into the newest version, so please check that
you can reproduce the problem on the latest release first.
