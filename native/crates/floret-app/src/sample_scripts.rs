//! Scripted performances for the Interactive and Sound samples: mouse
//! moves, presses and drags, keys held, text typed, frame by frame.
//! `sample_play` checks each sample responds to its performance; the
//! Library's covers ([`crate::thumbs`]) play it for samples that are empty
//! until used (`coverPlay` in `samples/catalogue.json`).

use floret_engine::FrameCtx;

/// What the player does on one frame.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub mouse: (f64, f64),
    pub pressed: bool,
    pub keys: Vec<&'static str>,
    pub typed: &'static str,
}

fn at(x: f64, y: f64) -> Input {
    Input {
        mouse: (x, y),
        ..Input::default()
    }
}

fn drag(x: f64, y: f64) -> Input {
    Input {
        mouse: (x, y),
        pressed: true,
        ..Input::default()
    }
}

fn keys(k: &[&'static str]) -> Input {
    Input {
        keys: k.to_vec(),
        ..Input::default()
    }
}

/// A point on a circle, for drags that draw loops.
fn round(f: u32, r: f64, speed: f64) -> (f64, f64) {
    let a = f64::from(f) * speed;
    (r * a.cos(), r * a.sin())
}

/// The scripted performance for a sample: what happens on frame `f`.
pub type Script = fn(u32) -> Input;

/// Paint loops with each brush, mirrored for the last one.
fn brush_studio(f: u32) -> Input {
    let (x, y) = round(f, 150.0, 0.09);
    match f {
        0..=9 => at(x, y),
        10..=39 => drag(x, y),
        40 => keys(&["2"]),
        41..=69 => drag(x * 0.6, y * 0.6),
        70 => keys(&["m"]),
        71..=79 => at(x, y),
        80..=129 => drag(x * 0.4 + 60.0, y * 0.4),
        _ => at(0.0, 0.0),
    }
}

/// Paint a run along the top row, then scatter a random tune with R.
fn step_sequencer(f: u32) -> Input {
    let x = -270.0 + f64::from(f.saturating_sub(10)) * 9.0;
    match f {
        10..=50 => drag(x.min(270.0), -156.0),
        60 => keys(&["r"]),
        _ => at(0.0, 300.0),
    }
}

/// A melody, then a held chord, with the filter opened.
fn keyboard_synth(f: u32) -> Input {
    let held: &[&'static str] = match f {
        10..=24 => &["a"],
        25..=39 => &["d"],
        40..=54 => &["g"],
        60..=140 => &["a", "d", "g"],
        _ => &[],
    };
    Input {
        mouse: (0.0, -180.0),
        keys: held.to_vec(),
        ..Input::default()
    }
}

/// Five slingshot shots from round the arena, then gravity off.
fn physics_sandbox(f: u32) -> Input {
    let shot = f / 22;
    let (x, y) = round(shot, 180.0, 1.3);
    match (f % 22, shot) {
        (0..=11, 0..=4) => {
            let pull = f64::from(f % 22) * 6.0;
            drag(x + pull * 0.5, y + pull)
        }
        (_, 6) if f.is_multiple_of(22) => keys(&["g"]),
        _ => at(-260.0, 260.0),
    }
}

/// Start a game, steer, launch.
fn breakout(f: u32) -> Input {
    let x = 200.0 * (f64::from(f) * 0.05).sin();
    match f {
        5 | 30 => drag(x, 200.0),
        _ => at(x, 200.0),
    }
}

/// Add points, show the triangles, relax.
fn voronoi_editor(f: u32) -> Input {
    match f {
        10 => drag(140.0, 120.0),
        20 => drag(-160.0, 40.0),
        30 => drag(10.0, -230.0),
        40 => keys(&["d"]),
        50..=130 => keys(&["l"]),
        _ => at(0.0, 0.0),
    }
}

/// Lure the flock into a corner.
fn flock_herder(f: u32) -> Input {
    if f >= 20 {
        drag(180.0, -150.0)
    } else {
        at(180.0, -150.0)
    }
}

/// Type a word.
fn typing_toy(f: u32) -> Input {
    let typed = match f {
        10 => "H",
        16 => "e",
        22 => "l",
        28 => "l",
        34 => "o",
        40 => "!",
        _ => "",
    };
    Input {
        typed,
        ..Input::default()
    }
}

/// Turn the tube, dropping beads, then eight mirrors.
fn kaleidoscope(f: u32) -> Input {
    match f {
        20..=100 => drag(-150.0 + f64::from(f - 20) * 4.0, 60.0),
        110 => keys(&["8"]),
        _ => at(0.0, 0.0),
    }
}

/// Paint a loop of chemical, then switch to fingerprints.
fn reaction_painter(f: u32) -> Input {
    let (x, y) = round(f, 130.0, 0.08);
    match f {
        10..=90 => drag(x, y),
        100 => keys(&["3"]),
        _ => at(x, y),
    }
}

/// Sweep the "hand" across the pitch range, held down.
fn hand_theremin(f: u32) -> Input {
    let x = -300.0 + f64::from(f) * 4.0;
    if f >= 10 {
        drag(x, -120.0 + 60.0 * (f64::from(f) * 0.1).sin())
    } else {
        at(x, -120.0)
    }
}

/// Plant flowers by clicking, then play keys.
fn midi_garden(f: u32) -> Input {
    match f {
        10 | 11 => drag(-180.0, -150.0),
        25 | 26 => drag(170.0, -40.0),
        40 | 41 => drag(-60.0, 190.0),
        60..=70 => keys(&["a"]),
        75..=85 => keys(&["g"]),
        90..=100 => keys(&["k"]),
        _ => at(0.0, 0.0),
    }
}

/// Drag to fan the layers apart and swell them.
fn preset_morph(f: u32) -> Input {
    if (20..=140).contains(&f) {
        drag(f64::from(f - 20) * 2.5, -150.0)
    } else {
        at(0.0, 0.0)
    }
}

/// Hold R (the red square), then B (the blue circle).
fn keyboard_shapes(f: u32) -> Input {
    match f {
        20..=59 => keys(&["r"]),
        60.. => keys(&["b"]),
        _ => Input::default(),
    }
}

/// Clear the disc, then add pins by clicking round it.
fn music_box(f: u32) -> Input {
    match f {
        5 => keys(&["c"]),
        20..=21 => drag(0.0, -120.0),
        35..=36 => drag(110.0, 40.0),
        50..=51 => drag(-90.0, 90.0),
        65..=66 => drag(-140.0, -60.0),
        _ => at(300.0, 300.0),
    }
}

/// The mouse circling the centre once every 1½ seconds (as the sample
/// review moves it), for covers of samples that draw where it goes.
fn circling_mouse(f: u32) -> Input {
    let (x, y) = round(f, 150.0, 0.07);
    at(x, y)
}

/// The samples and their performances.
pub const SCRIPTS: &[(&str, Script)] = &[
    ("play-brush-studio", brush_studio),
    ("sound-step-sequencer", step_sequencer),
    ("sound-keyboard-synth", keyboard_synth),
    ("play-physics-sandbox", physics_sandbox),
    ("play-breakout", breakout),
    ("play-voronoi-editor", voronoi_editor),
    ("play-flock-herder", flock_herder),
    ("play-typing-toy", typing_toy),
    ("play-kaleidoscope", kaleidoscope),
    ("play-reaction-painter", reaction_painter),
    ("sound-hand-theremin", hand_theremin),
    ("play-midi-garden", midi_garden),
    ("play-preset-morph", preset_morph),
    ("sound-music-box", music_box),
    ("tut-30-keyboard", keyboard_shapes),
];

pub fn frame_ctx(f: u32, size: (u32, u32), input: &Input) -> FrameCtx {
    let dt = 1.0 / 60.0;
    FrameCtx {
        t: f64::from(f) * dt,
        dt,
        frame_count: f64::from(f),
        width: f64::from(size.0),
        height: f64::from(size.1),
        mouse_x: input.mouse.0,
        mouse_y: input.mouse.1,
        mouse_is_pressed: input.pressed,
        key: input
            .keys
            .last()
            .map(|k| (*k).to_owned())
            .unwrap_or_default(),
        key_is_pressed: !input.keys.is_empty(),
        keys_down: input.keys.iter().map(|k| (*k).to_owned()).collect(),
        typed: input.typed.to_owned(),
        loop_length: 0.0,
    }
}

/// Input played only for the Library covers of samples that follow the
/// mouse but aren't performances themselves (so `sample_play` doesn't
/// hold them to its "playing changes the picture" bar).
pub const COVER_SCRIPTS: &[(&str, Script)] = &[
    ("tut-21-trails", circling_mouse),
    ("tut-32-normalize", circling_mouse),
    ("paint-trail", circling_mouse),
];

/// The input scripted for sample `id`, if any.
pub fn script_for(id: &str) -> Option<Script> {
    SCRIPTS
        .iter()
        .chain(COVER_SCRIPTS)
        .find(|(s, _)| *s == id)
        .map(|(_, f)| *f)
}
