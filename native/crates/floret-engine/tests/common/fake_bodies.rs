//! Stand-in bodies for the tracking samples' tests and Library covers: a
//! dancing figure (33 pose landmarks) and a face (478 face landmarks), as
//! a tracker would report them for a 640 × 480 camera picture.
//!
//! Coordinates are normalised to the picture (x right, y down), as the
//! `input/landmarks` node receives them from the app's tracker, before it
//! mirrors and scales them. The face is MediaPipe's canonical face model
//! (`canonical_face.txt`, centimetres, y up) turned, tilted and with the
//! mouth opened; the irises are rings round each eye.
//!
//! Included from `floret-engine`'s tests and `floret-app`'s sample sweep
//! with `#[path]`, so it only uses `std`.

/// The picture the stand-in camera shows.
pub const PICTURE: (f32, f32) = (640.0, 480.0);

fn norm(x: f32, y: f32, z: f32) -> [f32; 3] {
    [x / PICTURE.0, y / PICTURE.1, z / PICTURE.0]
}

/// The elbow (or knee) between `a` and `b`, `l1` from `a` and `l2` from
/// `b`, bent towards `side` (±1 on x).
fn joint(a: (f32, f32), b: (f32, f32), l1: f32, l2: f32, side: f32) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let d = (dx * dx + dy * dy).sqrt().clamp(1.0, l1 + l2 - 0.5);
    let along = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
    let h = (l1 * l1 - along * along).max(0.0).sqrt();
    let (ux, uy) = (dx / d.max(1e-3), dy / d.max(1e-3));
    (
        a.0 + ux * along - uy * h * side,
        a.1 + uy * along + ux * h * side,
    )
}

/// A figure dancing in the middle of the picture at time `t` seconds: the
/// arms draw wide loops, the feet step, the head sways. 33 landmarks in
/// MediaPipe's pose order (nose, eyes, ears, mouth, shoulders, elbows,
/// wrists, hands, hips, knees, ankles, heels, toes).
pub fn pose(t: f64) -> Vec<[f32; 3]> {
    let t = t as f32;
    let cx = 320.0 + 14.0 * (t * 1.3).sin();
    let bob = 3.0 * (t * 4.2).sin().abs();
    let (nose, shoulder_y, hip_y) = (
        (
            cx + 20.0 * (t * 2.3).sin(),
            96.0 - bob + 10.0 * (t * 3.1).cos(),
        ),
        158.0 - bob,
        268.0,
    );
    // The figure's left side is on the right of the picture.
    let sh = [(cx + 58.0, shoulder_y), (cx - 58.0, shoulder_y)];
    let hip = [(cx + 34.0, hip_y), (cx - 34.0, hip_y)];
    // A wrist draws a looping figure round `c`: sines at two different
    // speeds across and up, kept within an arm's reach of the shoulder.
    let arm = |s: (f32, f32), c: (f32, f32), r: (f32, f32), w: (f32, f32), ph: f32| {
        let (mut x, mut y) = (
            c.0 + r.0 * (t * w.0 + ph).sin(),
            c.1 + r.1 * (t * w.1).sin(),
        );
        let (dx, dy) = (x - s.0, y - s.1);
        let reach = (dx * dx + dy * dy).sqrt();
        if reach > 160.0 {
            x = s.0 + dx * 160.0 / reach;
            y = s.1 + dy * 160.0 / reach;
        }
        (x, y)
    };
    let wrist = [
        arm(sh[0], (cx + 100.0, 110.0), (70.0, 90.0), (2.9, 4.4), 0.4),
        arm(sh[1], (cx - 100.0, 125.0), (62.0, 80.0), (3.7, 2.6), 1.9),
    ];
    let elbow = [
        joint(sh[0], wrist[0], 84.0, 80.0, 1.0),
        joint(sh[1], wrist[1], 84.0, 80.0, -1.0),
    ];
    let step = |ph: f32| {
        let s = (t * 4.4 + ph).sin();
        (s, 428.0 - 44.0 * s.max(0.0))
    };
    let (s0, y0) = step(0.0);
    let (s1, y1) = step(std::f32::consts::PI);
    let ankle = [(cx + 44.0 + 34.0 * s0, y0), (cx - 44.0 + 34.0 * s1, y1)];
    let knee = [
        joint(hip[0], ankle[0], 84.0, 84.0, 1.0),
        joint(hip[1], ankle[1], 84.0, 84.0, -1.0),
    ];
    let mut p = vec![[0.0f32; 3]; 33];
    let mut set = |i: usize, x: f32, y: f32, z: f32| p[i] = norm(x, y, z);
    // Head: nose, eyes (inner, centre, outer), ears, mouth.
    set(0, nose.0, nose.1, -30.0);
    for (i, dx) in [
        (1, -8.0),
        (2, -16.0),
        (3, -24.0),
        (4, 8.0),
        (5, 16.0),
        (6, 24.0),
    ] {
        set(i, nose.0 + dx, nose.1 - 14.0, -16.0);
    }
    set(7, nose.0 - 40.0, nose.1 - 4.0, 0.0);
    set(8, nose.0 + 40.0, nose.1 - 4.0, 0.0);
    set(9, nose.0 - 10.0, nose.1 + 22.0, -14.0);
    set(10, nose.0 + 10.0, nose.1 + 22.0, -14.0);
    for k in 0..2 {
        // Left side first (11, 13, 15, …): the right of the picture.
        let sgn = k;
        let (s, e, w) = (11 + k, 13 + k, 15 + k);
        set(s, sh[sgn].0, sh[sgn].1, 0.0);
        set(e, elbow[sgn].0, elbow[sgn].1, 0.0);
        set(w, wrist[sgn].0, wrist[sgn].1, -10.0);
        // Pinky, index, thumb a little past the wrist.
        let (ex, ey) = (wrist[sgn].0 - elbow[sgn].0, wrist[sgn].1 - elbow[sgn].1);
        let len = (ex * ex + ey * ey).sqrt().max(1.0);
        let (ux, uy) = (ex / len, ey / len);
        for (j, off) in [(17, -9.0), (19, 9.0), (21, 0.0)] {
            let (px, py) = (
                wrist[sgn].0 + ux * 22.0 - uy * off,
                wrist[sgn].1 + uy * 22.0 + ux * off,
            );
            set(j + k, px, py, -10.0);
        }
        set(23 + k, hip[sgn].0, hip[sgn].1, 0.0);
        set(25 + k, knee[sgn].0, knee[sgn].1, 0.0);
        set(27 + k, ankle[sgn].0, ankle[sgn].1, 0.0);
        set(29 + k, ankle[sgn].0 - 6.0, ankle[sgn].1 + 12.0, 0.0);
        set(31 + k, ankle[sgn].0 + 10.0, ankle[sgn].1 + 14.0, 0.0);
    }
    p
}

/// The canonical face: 468 points in centimetres, y up, z towards the
/// camera.
fn canonical() -> Vec<[f32; 3]> {
    include_str!("canonical_face.txt")
        .lines()
        .map(|l| {
            let mut v = l
                .split_whitespace()
                .map(|s| s.parse::<f32>().expect("a number"));
            [v.next().unwrap(), v.next().unwrap(), v.next().unwrap()]
        })
        .collect()
}

/// A face looking into the camera at time `t`: turned by `yaw` radians
/// (left–right) and `pitch` (nodding), its mouth open by `open` (0 shut,
/// 1 wide), centred at (`cx`, `cy`) of the picture with `k` pixels to the
/// centimetre. 478 landmarks: the 468 of the face mesh and ten for the
/// irises.
pub fn face(cx: f32, cy: f32, k: f32, yaw: f32, pitch: f32, open: f32) -> Vec<[f32; 3]> {
    let canon = canonical();
    let mouth_y = (canon[13][1] + canon[14][1]) / 2.0;
    let (sy, cyw) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let turn = |[x, y, z]: [f32; 3]| {
        let (x, z) = (x * cyw + z * sy, -x * sy + z * cyw);
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        [x, y, z]
    };
    let mut moved: Vec<[f32; 3]> = canon
        .iter()
        .map(|&[x, y, z]| {
            // The jaw and lower lip drop with the mouth.
            let below = ((mouth_y - y) / 0.3).clamp(0.0, 1.0);
            let side = 1.0 - (x.abs() / 8.0).powi(2).min(1.0) * 0.5;
            [x, y - open * 3.4 * below * side, z]
        })
        .collect();
    // Irises: a ring of four round the middle of each eye, and its centre.
    let centre = |mesh: &[[f32; 3]], ids: &[usize]| {
        let n = ids.len() as f32;
        let sum = ids.iter().fold([0.0f32; 3], |a, &i| {
            [a[0] + mesh[i][0], a[1] + mesh[i][1], a[2] + mesh[i][2]]
        });
        [sum[0] / n, sum[1] / n, sum[2] / n]
    };
    for eye in [
        &[33usize, 133, 159, 145][..], // the figure's right eye: 468–472
        &[263, 362, 386, 374][..],     // the left: 473–477
    ] {
        let c = centre(&moved, eye);
        moved.push([c[0], c[1], c[2] + 0.4]);
        for (dx, dy) in [(0.0, 0.5), (0.5, 0.0), (0.0, -0.5), (-0.5, 0.0)] {
            moved.push([c[0] + dx, c[1] + dy, c[2] + 0.4]);
        }
    }
    moved
        .into_iter()
        .map(turn)
        .map(|[x, y, z]| norm(cx + x * k, cy - y * k, z * k))
        .collect()
}

/// A face that nods and turns on its own, its mouth opening now and then:
/// the figure the face sample's tests and cover watch at time `t` seconds.
pub fn talking_face(t: f64) -> Vec<[f32; 3]> {
    let t = t as f32;
    // The mouth opens and closes every two seconds, wide at the peak.
    let open = ((t * std::f32::consts::PI).sin() * 2.0)
        .clamp(0.0, 1.0)
        .powi(2)
        * 0.75;
    face(
        320.0 + 12.0 * (t * 0.9).sin(),
        232.0,
        18.0,
        0.35 * (t * 0.8).sin(),
        0.1 * (t * 1.1).cos(),
        open,
    )
}
