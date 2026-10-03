//! Tick every sample for N frames and print ms per tick, slowest first.
//!
//! ```sh
//! cargo run --release -p floret-engine --example bench -- [--ticks N] [--top N] [sample …]
//! ```
//!
//! Frames follow the parity fixtures' script (60 fps, mouse circling the
//! centre, key `a` held on frames 10–19). `first` is tick 0, which builds
//! everything memoised; `median` / `mean` / `max` cover the remaining ticks,
//! `memo` is the share of computes skipped on the last tick. With
//! `--top N`, each sample also lists its N slowest nodes (self time).

use floret_engine::{Engine, FrameCtx};
use std::path::Path;
use std::time::Instant;

fn frame(i: usize, width: f64, height: f64) -> FrameCtx {
    let t = i as f64 / 60.0;
    let a = t * std::f64::consts::TAU / 4.0;
    let r = width.min(height) * 0.3;
    FrameCtx {
        t,
        dt: 1.0 / 60.0,
        frame_count: i as f64,
        width,
        height,
        mouse_x: r * a.cos(),
        mouse_y: r * a.sin(),
        mouse_is_pressed: (20..40).contains(&(i % 40)),
        key: if i >= 10 { "a".into() } else { String::new() },
        key_is_pressed: (10..20).contains(&i),
        ..FrameCtx::default()
    }
}

fn canvas(engine: &Engine) -> (f64, f64) {
    let Some(r) = engine.graph.renderer() else {
        return (600.0, 400.0);
    };
    let id = &engine.graph.nodes[r].id;
    let dim = |k: &str, d: f64| {
        engine
            .params
            .get(id, k)
            .and_then(|v| v.as_f64())
            .unwrap_or(d)
            .clamp(32.0, 4096.0)
    };
    (dim("width", 600.0), dim("height", 400.0))
}

struct Row {
    id: String,
    nodes: usize,
    first: f64,
    median: f64,
    mean: f64,
    max: f64,
    /// Share of computes the memo skipped on the last tick.
    hit_rate: f64,
    top: Vec<String>,
}

/// Run `ticks` frames; per-tick wall times in ms.
fn run(engine: &mut Engine, ticks: usize) -> Vec<f64> {
    let (w, h) = canvas(engine);
    (0..ticks.max(2))
        .map(|i| {
            let f = frame(i, w, h);
            let start = Instant::now();
            let r = engine.tick(&f);
            let ms = start.elapsed().as_secs_f64() * 1e3;
            drop(r);
            ms
        })
        .collect()
}

fn main() {
    let mut ticks = 120usize;
    let mut top = 0usize;
    let mut only = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--ticks" => ticks = args.next().and_then(|v| v.parse().ok()).unwrap_or(ticks),
            "--top" => top = args.next().and_then(|v| v.parse().ok()).unwrap_or(5),
            "--bench" => {}
            _ => only.push(a),
        }
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .expect("samples dir")
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".flora")
                .map(str::to_owned)
        })
        .filter(|id| only.is_empty() || only.iter().any(|o| id.contains(o.as_str())))
        .collect();
    ids.sort();

    let mut rows = Vec::new();
    for id in ids {
        let text = std::fs::read_to_string(dir.join(format!("{id}.flora"))).expect("read");
        let Ok(mut engine) = Engine::load(&text) else {
            eprintln!("{id}: compile error");
            continue;
        };
        let times = run(&mut engine, ticks);
        let (evals, hits) = engine.profile().totals();
        let mut rest = times[1..].to_vec();
        rest.sort_by(f64::total_cmp);
        // Timing every node slows the tick, so the breakdown is a second run.
        let top = if top > 0 {
            let mut engine = Engine::load(&text).expect("compiled once");
            engine.set_profiling(true);
            run(&mut engine, ticks);
            let p = engine.profile();
            p.top(top)
                .into_iter()
                .map(|(i, n)| {
                    let node = &engine.graph.nodes[i];
                    format!(
                        "{:>8.3} ms  {} ({})  evals {} hits {}",
                        n.avg_ms, node.id, node.ty, n.evals, n.hits
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        rows.push(Row {
            id,
            nodes: engine.graph.nodes.len(),
            first: times[0],
            median: rest[rest.len() / 2],
            mean: rest.iter().sum::<f64>() / rest.len() as f64,
            max: rest.last().copied().unwrap_or(0.0),
            hit_rate: hits as f64 / ((evals + hits) as f64).max(1.0),
            top,
        });
    }
    rows.sort_by(|a, b| b.median.total_cmp(&a.median));
    println!(
        "{:32} {:>6} {:>10} {:>10} {:>10} {:>10} {:>6}",
        "sample", "nodes", "first ms", "median ms", "mean ms", "max ms", "memo"
    );
    for r in &rows {
        println!(
            "{:32} {:>6} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>5.0}%",
            r.id,
            r.nodes,
            r.first,
            r.median,
            r.mean,
            r.max,
            r.hit_rate * 100.0
        );
        for t in &r.top {
            println!("      {t}");
        }
    }
    let total: f64 = rows.iter().map(|r| r.median).sum();
    println!(
        "sum of median ms/tick over {} samples: {total:.3}",
        rows.len()
    );
}
