# Floret Landing Page — Design Spec

Design a marketing landing page for **Floret**, a browser-based node canvas for generative art. Desktop-first, 1440px wide. The audience is **artists and makers, not developers** — people who want to build interactive installations, live visuals, and generative prints without writing code. The tone is welcoming, playful, and community-focused. Every headline sells an outcome (a room, a stage, a print), never a feature spec.

---

## 1. Brand & Design Tokens

### Color — light theme (the page's base)
| Token | Value | Use |
|---|---|---|
| bg-0 | `#e7e8ee` | page background (soft cool gray-lavender) |
| bg-1 | `#f3f4f7` | tinted panels, mini-grid cards |
| bg-2 | `#ffffff` | cards, nav CTA surface, node bodies |
| bg-3 | `#e0e2e9` | node header base, pressed states |
| border | `#d6d9e1` | all hairline borders |
| fg-0 | `#1b1d24` | headlines, primary text |
| fg-1 | `#565b66` | body copy |
| fg-2 | `#9096a2` | captions, footer meta |
| **accent** | `#f5a623` | **sunflower yellow-orange** — buttons, dots, CTA band |
| accent-deep | `#b3720d` | eyebrow text, wire color (readable on light bg) |
| ink-art | `#14131a` | near-black tiles that generative art renders on |

Dark "art tiles" (`#14131a`) are used inside an otherwise light page — art glows on dark, chrome stays light. This contrast is a signature of the page.

### Type
- **Display: Space Grotesk** (Bold/Medium) — wordmark, all headlines, node titles, card titles. Tight letter-spacing on big sizes (−1px at 60px).
- **Body: Hanken Grotesk** (Regular/Medium/SemiBold) — all paragraphs, links, buttons.
- **Mono: JetBrains Mono** (Medium) — eyebrows (uppercase, +1.5px tracking), tiny value pills, filename chips. Mono is seasoning, never body text.

### Shape & elevation
- Radii: buttons 12px, cards 16–18px, hero visual 20px, node cards 10px, pills 999px.
- Shadows: soft and cool — `0 6px 18px rgba(20,22,30,0.08)` on cards, `0 16px 44px rgba(20,22,30,0.12)` on the hero visual.
- Category hues (used for node headers, dots, illustration accents): a harmonised sweep at `hsl(H, 58%, 63%)` — sound=272, particles=202, color/palette=34, image=104, motion=188, random=230.

---

## 2. Page Structure (top to bottom)

### Nav
Transparent on bg-0. Left: **FLORET** wordmark (Space Grotesk Bold, ~21px, +3px letter-spacing). Right: text links `Explore · Learn · Community` + sunflower button **"Start creating"** (dark ink text, subtle shadow).

### Hero — centered
1. Pill chip (white, hairline border, sunflower dot): `Free · runs in your browser · no code`
2. H1, ~62px, centered, two lines: **"Make art that moves, reacts, and fills a room."**
3. Sub (~18.5px, max-width 680px): *"Floret is a playful canvas where you snap blocks together — particles, shaders, sound, motion — and watch it come alive. For gallery walls, live stages, and everything in between."*
4. Buttons: **"Start creating — it's free"** (sunflower) + **"See what people made"** (white, bordered).
5. **Hero visual** — a wide (full-width, ~520px tall, 20px radius, big soft shadow) mock of the Floret editor:
   - Light chrome: white titlebar with 3 traffic lights + mono filename `my-first-patch`.
   - Left: three **authentic Floret nodes** (see §3) stacked vertically: `Mic In` (sound, hue 272), `Particles` (hue 202), `Palette` (hue 34).
   - Right ~56% of the width: a near-black art pane (16px radius) rendering a **spiral galaxy of colored particle dots** (300+ dots, hue drifting 20→250 along the spiral, brighter/larger toward center).
   - Thin orthogonal **sunflower-amber wires** (~1.6px, `#c77f10` at 75%) route from the node output pins into the art pane.

### Section 2 — Use cases: "Not just a screen. A space."
Eyebrow (mono, uppercase, accent-deep): `WHERE YOUR ART LIVES`. H2 ~40px centered.
Three equal white cards (18px radius, hairline border, soft shadow), each = illustration (~200px tall, flush to card top) + copy block:
1. **Interactive installations** — dark illustration: a glowing gradient wall (cyan→purple→pink) speckled with white dots, two black visitor silhouettes in front. Copy: *"Camera and motion input turn a gallery wall into something visitors play with — the art answers back."*
2. **Live visuals & VJ sets** — dark illustration: a wall of colorful FFT bars (violet→pink sweep) above a row of crowd-head silhouettes. Copy: *"Feed it your music and the visuals dance in time — ready for gigs, streams, and late-night jams."*
3. **Prints & screens** — light illustration: a framed generative print (concentric warm-hued rings) hanging on a pale wall with a drop shadow. Copy: *"Freeze the frame you love and export it crisp as a poster, album cover, or a living wallpaper."*

### Section 3 — Capability spotlights (white band)
Full-width white background band. Three alternating rows (visual ↔ copy, ~620×340px art tiles on `#14131a`, 18px radius) + a mini-grid. Each copy column: mono eyebrow → 36px headline → 16.5px body → 2 bullet rows (7px sunflower dot + 15px Medium text).

**Row 1 — PARTICLES** (visual left): art tile with a dense particle spiral (230+ dots, hue 160→340, fading outward).
- H: **"Ten thousand fireflies, zero code."**
- Body: *"Spawn a swarm, give it wind, gravity, or your voice — and steer it with a slider. Particles in Floret feel less like physics homework and more like finger-painting."*
- Bullets: `Flocking, trails, and collisions built in` · `Steer with mouse, camera, or microphone`

**Row 2 — SHADERS** (copy left): art tile with a liquid plasma gradient (layered translucent color bands, blue→violet→magenta, concentric white ripple rings at center).
- H: **"Paint with light itself."**
- Body: *"Shaders are how screens glow, ripple, and melt. Drop one in like any other block — tweak its dials live, no GPU degree required. Borrow one from the community and make it yours."*
- Bullets: `Works with ShaderToy-style snippets` · `Every dial becomes a knob you can wire`

**Row 3 — CAMERA & BODY** (visual left): art tile with a white pose-skeleton figure (dots + limb lines, one arm raised) with warm particles (hue 20→110) streaming from the raised hand.
- H: **"You are the controller."**
- Body: *"Point a camera at yourself and wave — hands, face, and full-body pose become brushes. Built on MediaPipe tracking that runs entirely in the browser, so your installation reacts the moment someone steps in front of it."*
- Bullets: `Hand, face & pose tracking — no extra hardware` · `Everything stays on device, nothing is recorded`

**Mini-grid** (4 equal tiles, bg-1 fill, 16px radius, colored 12px dot + 16.5px title + 13.5px body):
1. **Sound-reactive** — *"Beats, FFT bands, and mic level — wire music straight into anything."*
2. **Print-quality export** — *"Freeze any frame as a high-res PNG, or record a seamless loop as video."*
3. **Feedback & trails** — *"Let frames melt into each other for smoke, ghosts, and long-exposure looks."*
4. **Remix anything** — *"Every gallery piece opens as a patch. See how it works, make it yours."*

### Section 4 — Community: "Every patch is a conversation."
Eyebrow: `MADE TOGETHER`. Centered H2 + sub: *"Open anything from the community gallery, see exactly how it was made, and remix it into something of your own. That's how everyone here learned."*
Three white quote cards (18px radius): quote (16.5px) + avatar row (38px gradient-fill circle avatar, name SemiBold, role in fg-2):
- **Amara, set designer** — *"I sketched an idea on Sunday. By Friday it was breathing on a nine-metre wall."*
- **Theo, VJ & producer** — *"I remix other people's patches live on stage. Nobody minds — that's the culture."*
- **June, illustrator** — *"I don't code. I never felt like I had to. I just kept plugging things in until it sang."*

### Section 5 — CTA band
Full-width **sunflower** (`#f5a623`) band, ~96px vertical padding, centered:
- H2 ~46px in ink: **"Your first patch takes five minutes."**
- Sub in deep warm brown (`#5c4408`): *"No account, no install, no code. Just a canvas and a place to start."*
- Buttons: **"Open Floret"** (near-black `#1b1d24`, white text) + **"Join the community"** (lighter sunflower `#f7b94e`, deep-amber border).

### Section 6 — Footer
bg-0. Left: FLORET wordmark + tagline **"Art you can touch."** Right: three link columns with mono uppercase headers — `CREATE` (Open the editor, Gallery, Starter patches), `LEARN` (Five-minute start, Tutorials, Docs), `COMMUNITY` (Discord, Showcase, GitHub). Hairline divider, then bottom row: `© 2026 Floret` ↔ `Made with Floret, naturally`.

---

## 3. Anatomy of an authentic Floret node (for the hero mock)

Reproduce these faithfully — they're the product:
- White card (`#ffffff`), 1px `#d6d9e1` border, **10px radius**, small soft shadow, ~176px wide.
- **Header, 19px tall**: horizontal gradient from the category color (mixed ~35% into `#e0e2e9`) on the left fading to plain `#e0e2e9` by ~60%; 1px hairline divider below; title in **Space Grotesk Medium 10.5px**, dark ink, e.g. `Particles`.
- **Rows, 18px each**: port label in Hanken Grotesk 10.5px (`count`, `speed`, `wind`) left-aligned for inputs, right-aligned for outputs (`out`, `level`, `beat`).
- **Value pills**: tiny rounded rects (5px radius, bg `#f3f4f7`, hairline border) with JetBrains Mono 9.5px values (`2000`, `0.4`) sitting opposite the label.
- **Pins**: 7px circles in the category color, centered ON the card edge (half outside), thin white outline. Inputs on the left edge, outputs on the right.
- A **wired** input's label turns amber (`#b3720d`).

## 4. Spacing rhythm
- Global horizontal padding: 64px. Section vertical padding: 88–96px top and bottom, consistently.
- Card gaps: 18–22px. Copy column internal spacing: 14–16px. Hero element spacing: ~26px.

## 5. Voice checklist
- Say "blocks," "snap together," "dials," "wire" — never "nodes API," "parameters," "GLSL," "compile."
- Name the technology only when it earns trust (MediaPipe, ShaderToy compatibility) and immediately translate it into an outcome.
- Emphasize: free, in the browser, no install, no code, remix culture, art in physical space.
