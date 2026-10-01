//! The interpreter against LiteRT (TensorFlow Lite's own runtime).
//!
//! Each vendored model is fed the same deterministic input
//! (`x[i] = (i·7919 mod 1000) / 1000`) the reference values were recorded
//! with, via `ai-edge-litert` 2.2 in Python; we compare every output's sum,
//! absolute sum and a few individual values.

use floret_input::nn::{Interpreter, Model};
use floret_input::tracking::bundle;
use std::sync::Arc;

struct Want {
    shape: &'static [usize],
    sum: f64,
    abs: f64,
    at: &'static [(usize, f64)],
}

fn check(task: &[u8], name: &str, wants: &[Want]) {
    let model = Arc::new(Model::from_bytes(bundle::entry(task, name).unwrap()).unwrap());
    let mut it = Interpreter::new(model.clone());
    for (i, x) in it.input_mut(0).iter_mut().enumerate() {
        *x = ((i as u64 * 7919) % 1000) as f32 / 1000.0;
    }
    it.run();
    for (o, w) in wants.iter().enumerate() {
        assert_eq!(model.output_shape(o), w.shape, "{name} output {o} shape");
        let v = it.output(o);
        let sum: f64 = v.iter().map(|&x| f64::from(x)).sum();
        let abs: f64 = v.iter().map(|&x| f64::from(x).abs()).sum();
        let close = |a: f64, b: f64, tol: f64| (a - b).abs() <= tol * (1.0 + b.abs());
        assert!(
            close(sum, w.sum, 1e-3),
            "{name}[{o}] sum {sum} vs {}",
            w.sum
        );
        assert!(
            close(abs, w.abs, 1e-3),
            "{name}[{o}] abs {abs} vs {}",
            w.abs
        );
        for &(i, want) in w.at {
            let got = f64::from(v[i]);
            assert!(close(got, want, 2e-3), "{name}[{o}][{i}] {got} vs {want}");
        }
    }
}

#[test]
fn hand_models_match_litert() {
    check(
        bundle::HAND_TASK,
        "hand_detector.tflite",
        &[
            Want {
                shape: &[1, 2016, 18],
                sum: 213951.0752,
                abs: 371634.0,
                at: &[
                    (0, -1.28184),
                    (1, 0.267566),
                    (2, 11.8837),
                    (12096, -6.06921),
                    (18144, -0.0998561),
                    (36287, 14.448),
                ],
            },
            Want {
                shape: &[1, 2016, 1],
                sum: -20707.356,
                abs: 20707.356,
                at: &[
                    (0, -6.75108),
                    (1, -10.9938),
                    (2, -6.32879),
                    (672, -5.22798),
                    (1008, -6.51434),
                    (2015, -13.465),
                ],
            },
        ],
    );
    check(
        bundle::HAND_TASK,
        "hand_landmarks_detector.tflite",
        &[
            Want {
                shape: &[1, 63],
                sum: 4638.4826,
                abs: 4638.48,
                at: &[
                    (0, 105.439),
                    (1, 182.047),
                    (21, 111.864),
                    (31, 79.0197),
                    (62, 10.5842),
                ],
            },
            Want {
                shape: &[1, 1],
                sum: 0.0024549,
                abs: 0.0024549,
                at: &[],
            },
            Want {
                shape: &[1, 1],
                sum: 0.799817,
                abs: 0.799817,
                at: &[],
            },
            Want {
                shape: &[1, 63],
                sum: -0.226052,
                abs: 1.12255,
                at: &[
                    (1, 0.0785307),
                    (2, 0.0458161),
                    (31, -0.0458448),
                    (62, 0.010776),
                ],
            },
        ],
    );
}

#[test]
fn face_models_match_litert() {
    check(
        bundle::FACE_TASK,
        "face_detector.tflite",
        &[
            Want {
                shape: &[1, 896, 16],
                sum: 54932.5723,
                abs: 186139.0,
                at: &[
                    (0, 1.19596),
                    (1, -0.0470841),
                    (2, 28.4674),
                    (4778, -4.23443),
                    (14335, -15.3566),
                ],
            },
            Want {
                shape: &[1, 896, 1],
                sum: -2947.7292,
                abs: 2947.7292,
                at: &[
                    (0, -4.82399),
                    (1, -3.88019),
                    (298, -3.35461),
                    (448, -5.06554),
                    (895, -28.9507),
                ],
            },
        ],
    );
    check(
        bundle::FACE_TASK,
        "face_landmarks_detector.tflite",
        &[
            Want {
                shape: &[1, 1, 1, 1434],
                sum: 109703.5702,
                abs: 116300.0,
                at: &[
                    (0, 118.157),
                    (1, 132.032),
                    (2, -17.8369),
                    (478, 63.6005),
                    (717, 112.715),
                    (1433, -5.47778),
                ],
            },
            Want {
                shape: &[1, 1, 1, 1],
                sum: -14.1012,
                abs: 14.1012,
                at: &[],
            },
        ],
    );
}

#[test]
fn pose_models_match_litert() {
    check(
        bundle::POSE_TASK,
        "pose_detector.tflite",
        &[
            Want {
                shape: &[1, 2254, 12],
                sum: -65772.7775,
                abs: 704572.0,
                at: &[
                    (0, -2.62246),
                    (1, -7.80924),
                    (2, 0.394805),
                    (9016, -152.374),
                    (13524, -2.41358),
                    (27047, 0.123061),
                ],
            },
            Want {
                shape: &[1, 2254, 1],
                sum: -130821.7402,
                abs: 130821.7402,
                at: &[
                    (0, -73.4553),
                    (1, -5.8628),
                    (2, -90.0802),
                    (751, -9.2493),
                    (1127, -6.86289),
                    (2253, -28.3589),
                ],
            },
        ],
    );
    check(
        bundle::POSE_TASK,
        "pose_landmarks_detector.tflite",
        &[
            Want {
                shape: &[1, 195],
                sum: 8497.8607,
                abs: 10219.5,
                at: &[
                    (0, 127.349),
                    (1, 70.0855),
                    (2, -19.0737),
                    (65, 136.51),
                    (97, -44.5325),
                    (194, -5.37172),
                ],
            },
            Want {
                shape: &[1, 1],
                sum: 0.000161144,
                abs: 0.000161144,
                at: &[],
            },
            Want {
                shape: &[1, 256, 256, 1],
                sum: -3597226.4626,
                abs: 3607860.0,
                at: &[
                    (0, -13.2177),
                    (1, -49.9068),
                    (2, -86.2858),
                    (21845, 0.217647),
                    (65535, -19.524),
                ],
            },
            Want {
                shape: &[1, 64, 64, 39],
                sum: -1302870.7154,
                abs: 1302870.0,
                at: &[
                    (0, -6.95888),
                    (1, -9.93038),
                    (53248, -5.14936),
                    (159743, -9.92098),
                ],
            },
            Want {
                shape: &[1, 117],
                sum: -8.7329,
                abs: 25.4109,
                at: &[
                    (0, 0.0193875),
                    (1, -0.54283),
                    (58, -0.671885),
                    (116, 0.334764),
                ],
            },
        ],
    );
}
