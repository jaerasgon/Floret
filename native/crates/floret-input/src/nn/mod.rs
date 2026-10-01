//! A small TFLite interpreter for MediaPipe's hand / face / pose models.
//!
//! The web app runs the `.task` bundles vendored in `public/mediapipe/`
//! through MediaPipe's WASM runtime. Native runs the same `.tflite` files
//! here, so landmark sets and indices are identical. Only what those
//! models use is supported: float32 graphs whose weights are float16
//! (`DEQUANTIZE`) or sparse (`DENSIFY`), both folded at load time, and the
//! ops in [`ops`]. Anything else fails to load with a clear error.

mod flatbuf;
pub mod ops;

use flatbuf::{Table, R};
use ops::{Act, Window};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct TensorInfo {
    shape: Vec<usize>,
    name: String,
}

#[derive(Clone, Debug)]
enum Const {
    F32(Arc<[f32]>),
    I32(Vec<i32>),
}

#[derive(Clone, Debug)]
enum Op {
    Conv {
        input: usize,
        filter: usize,
        bias: Option<usize>,
        win: Window,
        act: Act,
    },
    Depthwise {
        input: usize,
        filter: usize,
        bias: Option<usize>,
        win: Window,
        act: Act,
    },
    MaxPool {
        input: usize,
        win: Window,
        act: Act,
    },
    Add {
        a: usize,
        b: usize,
        act: Act,
    },
    Prelu {
        input: usize,
        alpha: usize,
    },
    Relu(usize),
    Logistic(usize),
    Copy(usize),
    Pad {
        input: usize,
        pads: Vec<(usize, usize)>,
    },
    Concat {
        inputs: Vec<usize>,
        axis: usize,
        act: Act,
    },
    Resize {
        input: usize,
        align_corners: bool,
        half_pixel: bool,
    },
    Mean {
        input: usize,
        axes: Vec<usize>,
    },
    FullyConnected {
        input: usize,
        weights: usize,
        bias: Option<usize>,
        act: Act,
    },
    DepthToSpace {
        input: usize,
        block: usize,
    },
}

/// A loaded model: immutable, shareable between interpreters.
#[derive(Debug)]
pub struct Model {
    tensors: Vec<TensorInfo>,
    consts: Vec<Option<Const>>,
    /// `(op, output tensor)` in execution order.
    ops: Vec<(Op, usize)>,
    inputs: Vec<usize>,
    outputs: Vec<usize>,
}

// TFLite builtin operator codes.
const ADD: i32 = 0;
const CONCATENATION: i32 = 2;
const CONV_2D: i32 = 3;
const DEPTHWISE_CONV_2D: i32 = 4;
const DEPTH_TO_SPACE: i32 = 5;
const DEQUANTIZE: i32 = 6;
const FULLY_CONNECTED: i32 = 9;
const LOGISTIC: i32 = 14;
const MAX_POOL_2D: i32 = 17;
const RELU: i32 = 19;
const RESHAPE: i32 = 22;
const RESIZE_BILINEAR: i32 = 23;
const PAD: i32 = 34;
const MEAN: i32 = 40;
const PRELU: i32 = 54;
const DENSIFY: i32 = 124;

// TFLite tensor types.
const FLOAT32: u8 = 0;
const FLOAT16: u8 = 1;
const INT32: u8 = 2;

impl Model {
    pub fn from_bytes(buf: &[u8]) -> R<Model> {
        Loader::new(buf)?.load()
    }

    /// Shape of input `i`.
    pub fn input_shape(&self, i: usize) -> &[usize] {
        &self.tensors[self.inputs[i]].shape
    }

    pub fn output_shape(&self, i: usize) -> &[usize] {
        &self.tensors[self.outputs[i]].shape
    }

    pub fn output_names(&self) -> Vec<&str> {
        self.outputs
            .iter()
            .map(|&t| self.tensors[t].name.as_str())
            .collect()
    }
}

struct Loader<'a> {
    buf: &'a [u8],
    model: Table<'a>,
    graph: Table<'a>,
}

fn len_of(shape: &[usize]) -> usize {
    shape.iter().product()
}

impl<'a> Loader<'a> {
    fn new(buf: &'a [u8]) -> R<Loader<'a>> {
        if buf.get(4..8) != Some(b"TFL3") {
            return Err("not a TFLite model".into());
        }
        let model = Table::root(buf)?;
        let graphs = model.vector(2)?.ok_or("model has no subgraphs")?;
        if graphs.len == 0 {
            return Err("model has no subgraphs".into());
        }
        Ok(Loader {
            buf,
            model,
            graph: graphs.table(0)?,
        })
    }

    /// Raw bytes of buffer `index` (`data`, or `offset`/`size` past 2 GB).
    fn buffer(&self, index: u32) -> R<Option<&'a [u8]>> {
        if index == 0 {
            return Ok(None);
        }
        let buffers = self.model.vector(4)?.ok_or("model has no buffers")?;
        let b = buffers.table(index as usize)?;
        if let Some(v) = b.vector(0)? {
            return (v.len > 0).then(|| v.bytes(1)).transpose();
        }
        let (off, size) = (b.u64(1, 0)? as usize, b.u64(2, 0)? as usize);
        if size == 0 {
            return Ok(None);
        }
        self.buf
            .get(off..off + size)
            .map(Some)
            .ok_or_else(|| "buffer out of range".into())
    }

    fn load(self) -> R<Model> {
        let tensors_fb = self.graph.vector(0)?.ok_or("subgraph has no tensors")?;
        let mut tensors = Vec::with_capacity(tensors_fb.len);
        let mut consts: Vec<Option<Const>> = Vec::with_capacity(tensors_fb.len);
        let mut sparse: Vec<Option<Table>> = Vec::with_capacity(tensors_fb.len);
        let mut raw: Vec<Option<(u8, &[u8])>> = Vec::with_capacity(tensors_fb.len);
        for i in 0..tensors_fb.len {
            let t = tensors_fb.table(i)?;
            let shape = t
                .i32s(0)?
                .into_iter()
                .map(|d| usize::try_from(d).map_err(|_| format!("dynamic shape on tensor {i}")))
                .collect::<R<Vec<usize>>>()?;
            let ty = t.u8(1, 0)?;
            let data = self.buffer(t.u32(2, 0)?)?;
            let sp = t.table(6)?;
            consts.push(match (ty, data) {
                (_, Some(_)) if sp.is_some() => None,
                (FLOAT32, Some(d)) => Some(Const::F32(
                    d.chunks_exact(4)
                        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                        .collect(),
                )),
                (INT32, Some(d)) => Some(Const::I32(
                    d.chunks_exact(4)
                        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                        .collect(),
                )),
                _ => None,
            });
            raw.push(data.map(|d| (ty, d)));
            sparse.push(sp);
            tensors.push(TensorInfo {
                shape,
                name: t.str(3)?.to_owned(),
            });
        }

        let codes = self.model.vector(1)?.ok_or("model has no operator codes")?;
        let ops_fb = self.graph.vector(3)?.ok_or("subgraph has no operators")?;
        let mut ops = Vec::new();
        for i in 0..ops_fb.len {
            let op = ops_fb.table(i)?;
            let code_t = codes.table(op.u32(0, 0)? as usize)?;
            let code = code_t.i32(3, 0)?.max(i32::from(code_t.u8(0, 0)? as i8));
            let ins: Vec<i32> = op.i32s(1)?;
            let outs: Vec<i32> = op.i32s(2)?;
            let t_in = |k: usize| -> R<usize> {
                let v = *ins
                    .get(k)
                    .ok_or_else(|| format!("op {i}: missing input {k}"))?;
                usize::try_from(v).map_err(|_| format!("op {i}: optional input {k} unsupported"))
            };
            let opt_in = |k: usize| ins.get(k).and_then(|&v| usize::try_from(v).ok());
            let out = usize::try_from(*outs.first().ok_or_else(|| format!("op {i}: no output"))?)
                .map_err(|e| e.to_string())?;
            let opts = op.table(4)?;
            let opt = || opts.ok_or_else(|| format!("op {i}: missing options"));
            let shape = |t: usize| tensors[t].shape.clone();
            let ints = |t: usize| -> R<Vec<i32>> {
                match &consts[t] {
                    Some(Const::I32(v)) => Ok(v.clone()),
                    _ => Err(format!("op {i}: expected a constant int tensor")),
                }
            };

            let node = match code {
                DEQUANTIZE => {
                    let src = t_in(0)?;
                    let v = match (&consts[src], raw[src]) {
                        // Densified (already f32) or float32 constants.
                        (Some(Const::F32(v)), _) => v.clone(),
                        (_, Some((FLOAT16, data))) if sparse[src].is_none() => half_floats(data),
                        _ => {
                            return Err(format!(
                                "op {i}: only constant float16 DEQUANTIZE is supported"
                            ))
                        }
                    };
                    consts[out] = Some(Const::F32(v));
                    continue;
                }
                DENSIFY => {
                    let src = t_in(0)?;
                    let (ty, data) = raw[src].ok_or(format!("op {i}: non-constant DENSIFY"))?;
                    let values: Vec<f32> = match ty {
                        FLOAT16 => half_floats(data).to_vec(),
                        FLOAT32 => data
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                            .collect(),
                        _ => return Err(format!("op {i}: unsupported DENSIFY type {ty}")),
                    };
                    let sp = sparse[src].ok_or(format!("op {i}: DENSIFY input not sparse"))?;
                    // A float16 result feeds a DEQUANTIZE next, which then
                    // just passes these f32 values through.
                    let dense = densify(&sp, &values, &tensors[out].shape)?;
                    consts[out] = Some(Const::F32(dense.into()));
                    continue;
                }
                CONV_2D | DEPTHWISE_CONV_2D => {
                    let o = opt()?;
                    let (input, filter) = (t_in(0)?, t_in(1)?);
                    let fs = shape(filter);
                    let is = shape(input);
                    let (padding, sw, sh) = (o.u8(0, 0)?, o.i32(1, 1)?, o.i32(2, 1)?);
                    let (act, dw, dh) = if code == CONV_2D {
                        (o.u8(3, 0)?, o.i32(4, 1)?, o.i32(5, 1)?)
                    } else {
                        if o.i32(3, 1)? != 1 || fs[3] != is[3] {
                            return Err(format!("op {i}: depth multiplier ≠ 1 unsupported"));
                        }
                        (o.u8(4, 0)?, o.i32(5, 1)?, o.i32(6, 1)?)
                    };
                    let win = Window::new(
                        (is[1], is[2]),
                        (fs[1], fs[2]),
                        (sh as usize, sw as usize),
                        (dh as usize, dw as usize),
                        padding == 0,
                    );
                    let (input, filter, bias, act) =
                        (input, filter, opt_in(2), Act::from_tflite(act)?);
                    if code == CONV_2D {
                        Op::Conv {
                            input,
                            filter,
                            bias,
                            win,
                            act,
                        }
                    } else {
                        Op::Depthwise {
                            input,
                            filter,
                            bias,
                            win,
                            act,
                        }
                    }
                }
                MAX_POOL_2D => {
                    let o = opt()?;
                    let input = t_in(0)?;
                    let is = shape(input);
                    let win = Window::new(
                        (is[1], is[2]),
                        (o.i32(4, 1)? as usize, o.i32(3, 1)? as usize),
                        (o.i32(2, 1)? as usize, o.i32(1, 1)? as usize),
                        (1, 1),
                        o.u8(0, 0)? == 0,
                    );
                    Op::MaxPool {
                        input,
                        win,
                        act: Act::from_tflite(o.u8(5, 0)?)?,
                    }
                }
                ADD => Op::Add {
                    a: t_in(0)?,
                    b: t_in(1)?,
                    act: Act::from_tflite(opts.map_or(Ok(0), |o| o.u8(0, 0))?)?,
                },
                PRELU => Op::Prelu {
                    input: t_in(0)?,
                    alpha: t_in(1)?,
                },
                RELU => Op::Relu(t_in(0)?),
                LOGISTIC => Op::Logistic(t_in(0)?),
                RESHAPE => Op::Copy(t_in(0)?),
                PAD => {
                    let input = t_in(0)?;
                    let p = ints(t_in(1)?)?;
                    let pads = p
                        .chunks_exact(2)
                        .map(|c| (c[0].max(0) as usize, c[1].max(0) as usize))
                        .collect::<Vec<_>>();
                    if pads.len() != shape(input).len() {
                        return Err(format!("op {i}: bad PAD paddings"));
                    }
                    Op::Pad { input, pads }
                }
                CONCATENATION => {
                    let o = opt()?;
                    let rank = shape(out).len() as i32;
                    let axis = o.i32(0, 0)?;
                    let axis = if axis < 0 { axis + rank } else { axis } as usize;
                    let inputs = (0..ins.len()).map(t_in).collect::<R<Vec<_>>>()?;
                    Op::Concat {
                        inputs,
                        axis,
                        act: Act::from_tflite(o.u8(1, 0)?)?,
                    }
                }
                RESIZE_BILINEAR => {
                    let o = opt()?;
                    Op::Resize {
                        input: t_in(0)?,
                        align_corners: o.bool(2)?,
                        half_pixel: o.bool(3)?,
                    }
                }
                MEAN => {
                    let input = t_in(0)?;
                    let rank = shape(input).len() as i32;
                    let mut axes: Vec<usize> = ints(t_in(1)?)?
                        .into_iter()
                        .map(|a| if a < 0 { a + rank } else { a } as usize)
                        .collect();
                    axes.sort_unstable();
                    axes.dedup();
                    Op::Mean { input, axes }
                }
                FULLY_CONNECTED => {
                    let o = opt()?;
                    if o.u8(1, 0)? != 0 {
                        return Err(format!("op {i}: shuffled FC weights unsupported"));
                    }
                    Op::FullyConnected {
                        input: t_in(0)?,
                        weights: t_in(1)?,
                        bias: opt_in(2),
                        act: Act::from_tflite(o.u8(0, 0)?)?,
                    }
                }
                DEPTH_TO_SPACE => Op::DepthToSpace {
                    input: t_in(0)?,
                    block: opt()?.i32(0, 2)? as usize,
                },
                other => return Err(format!("unsupported TFLite op {other}")),
            };
            ops.push((node, out));
        }

        let model = Model {
            consts,
            ops,
            inputs: to_indices(self.graph.i32s(1)?)?,
            outputs: to_indices(self.graph.i32s(2)?)?,
            tensors,
        };
        model.check()?;
        Ok(model)
    }
}

fn to_indices(v: Vec<i32>) -> R<Vec<usize>> {
    v.into_iter()
        .map(|i| usize::try_from(i).map_err(|e| e.to_string()))
        .collect()
}

fn half_floats(data: &[u8]) -> Arc<[f32]> {
    data.chunks_exact(2)
        .map(|c| ops::f16_to_f32(u16::from_le_bytes([c[0], c[1]])))
        .collect()
}

/// Expand a TFLite sparse tensor (TFLite's `FormatConverter`, without block
/// sparsity) to dense row-major.
fn densify(sp: &Table, values: &[f32], shape: &[usize]) -> R<Vec<f32>> {
    struct Dim {
        dense: Option<usize>,
        segments: Vec<usize>,
        indices: Vec<usize>,
    }
    let order: Vec<usize> = sp.i32s(0)?.into_iter().map(|v| v as usize).collect();
    if !sp.i32s(1)?.is_empty() {
        return Err("block-sparse tensors are unsupported".into());
    }
    let meta = sp.vector(2)?.ok_or("sparse tensor without dim metadata")?;
    let read_index_vec = |t: &Table, type_field: usize| -> R<Vec<usize>> {
        let ty = t.u8(type_field, 0)?;
        let Some(v) = t.table(type_field + 1)? else {
            return Ok(Vec::new());
        };
        let Some(vals) = v.vector(0)? else {
            return Ok(Vec::new());
        };
        (0..vals.len)
            .map(|i| match ty {
                1 => vals.i32(i).map(|x| x.max(0) as usize),
                2 => vals.u16(i).map(usize::from),
                3 => vals.u8(i).map(usize::from),
                _ => Err(format!("bad sparse index vector type {ty}")),
            })
            .collect()
    };
    let mut dims = Vec::with_capacity(meta.len);
    for d in 0..meta.len {
        let m = meta.table(d)?;
        dims.push(if m.u8(0, 0)? == 0 {
            Dim {
                dense: Some(m.i32(1, 0)? as usize),
                segments: Vec::new(),
                indices: Vec::new(),
            }
        } else {
            Dim {
                dense: None,
                segments: read_index_vec(&m, 2)?,
                indices: read_index_vec(&m, 4)?,
            }
        });
    }
    if dims.len() != shape.len() || order.len() != shape.len() {
        return Err("sparse tensor rank mismatch".into());
    }
    let mut strides = vec![1usize; shape.len()];
    for d in (0..shape.len().saturating_sub(1)).rev() {
        strides[d] = strides[d + 1] * shape[d + 1];
    }
    // Depth-first walk in traversal order; `prev` is the position within
    // the previous level (TFLite's `Populate`).
    struct Walk<'a> {
        dims: &'a [Dim],
        order: &'a [usize],
        strides: &'a [usize],
        values: &'a [f32],
        next: usize,
        out: Vec<f32>,
    }
    impl Walk<'_> {
        fn walk(&mut self, level: usize, prev: usize, pos: usize) -> R<()> {
            if level == self.dims.len() {
                let v = *self
                    .values
                    .get(self.next)
                    .ok_or("sparse values too short")?;
                *self.out.get_mut(pos).ok_or("sparse index out of range")? = v;
                self.next += 1;
                return Ok(());
            }
            let stride = self.strides[self.order[level]];
            let d = &self.dims[level];
            match d.dense {
                Some(n) => {
                    for i in 0..n {
                        self.walk(level + 1, prev * n + i, pos + i * stride)?;
                    }
                }
                None => {
                    let a = *d.segments.get(prev).ok_or("bad sparse segments")?;
                    let b = *d.segments.get(prev + 1).ok_or("bad sparse segments")?;
                    for k in a..b {
                        let i = *d.indices.get(k).ok_or("bad sparse indices")?;
                        self.walk(level + 1, k, pos + i * stride)?;
                    }
                }
            }
            Ok(())
        }
    }
    let mut w = Walk {
        dims: &dims,
        order: &order,
        strides: &strides,
        values,
        next: 0,
        out: vec![0.0; len_of(shape)],
    };
    w.walk(0, 0, 0)?;
    Ok(w.out)
}

impl Model {
    /// Every op's inputs are produced before use and sizes agree.
    fn check(&self) -> R<()> {
        let mut ready: Vec<bool> = self.consts.iter().map(Option::is_some).collect();
        for &i in &self.inputs {
            ready[i] = true;
        }
        for (op, out) in &self.ops {
            for t in op_inputs(op) {
                if !ready.get(t).copied().unwrap_or(false) {
                    return Err(format!("tensor {t} used before it is computed"));
                }
            }
            if let Op::Copy(src) = op {
                if len_of(&self.tensors[*src].shape) != len_of(&self.tensors[*out].shape) {
                    return Err("RESHAPE changes the element count".into());
                }
            }
            ready[*out] = true;
        }
        if let Some(&t) = self.outputs.iter().find(|&&t| !ready[t]) {
            return Err(format!("output tensor {t} is never computed"));
        }
        Ok(())
    }
}

fn op_inputs(op: &Op) -> Vec<usize> {
    match op {
        Op::Conv {
            input,
            filter,
            bias,
            ..
        }
        | Op::Depthwise {
            input,
            filter,
            bias,
            ..
        }
        | Op::FullyConnected {
            input,
            weights: filter,
            bias,
            ..
        } => {
            let mut v = vec![*input, *filter];
            v.extend(bias);
            v
        }
        Op::Add { a, b, .. } => vec![*a, *b],
        Op::Prelu { input, alpha } => vec![*input, *alpha],
        Op::Concat { inputs, .. } => inputs.clone(),
        Op::MaxPool { input, .. }
        | Op::Pad { input, .. }
        | Op::Resize { input, .. }
        | Op::Mean { input, .. }
        | Op::DepthToSpace { input, .. }
        | Op::Relu(input)
        | Op::Logistic(input)
        | Op::Copy(input) => vec![*input],
    }
}

/// Runs a [`Model`]; owns the activation buffers so repeated runs don't
/// allocate.
pub struct Interpreter {
    model: Arc<Model>,
    values: Vec<Vec<f32>>,
    scratch: Vec<f32>,
    /// Per op: whether a wanted output depends on it.
    active: Vec<bool>,
}

impl Interpreter {
    pub fn new(model: Arc<Model>) -> Interpreter {
        let values = model
            .tensors
            .iter()
            .zip(&model.consts)
            .map(|(t, c)| {
                if c.is_some() {
                    Vec::new()
                } else {
                    vec![0.0; len_of(&t.shape)]
                }
            })
            .collect();
        Interpreter {
            active: vec![true; model.ops.len()],
            model,
            values,
            scratch: Vec::new(),
        }
    }

    /// Only compute what outputs `wanted` need (e.g. skip a segmentation
    /// head nobody reads); other outputs keep stale values.
    pub fn with_outputs(mut self, wanted: &[usize]) -> Interpreter {
        let mut needed = vec![false; self.model.tensors.len()];
        for &o in wanted {
            needed[self.model.outputs[o]] = true;
        }
        for (i, (op, out)) in self.model.ops.iter().enumerate().rev() {
            self.active[i] = needed[*out];
            if needed[*out] {
                for t in op_inputs(op) {
                    needed[t] = true;
                }
            }
        }
        self
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Input tensor `i`'s buffer, to fill before [`run`](Self::run).
    pub fn input_mut(&mut self, i: usize) -> &mut [f32] {
        &mut self.values[self.model.inputs[i]]
    }

    pub fn output(&self, i: usize) -> &[f32] {
        self.get(self.model.outputs[i])
    }

    fn get(&self, t: usize) -> &[f32] {
        match &self.model.consts[t] {
            Some(Const::F32(v)) => v,
            _ => &self.values[t],
        }
    }

    fn shape(&self, t: usize) -> &[usize] {
        &self.model.tensors[t].shape
    }

    pub fn run(&mut self) {
        let model = self.model.clone();
        for (i, (op, out)) in model.ops.iter().enumerate() {
            if !self.active[i] {
                continue;
            }
            let mut dst = std::mem::take(&mut self.values[*out]);
            self.exec(op, *out, &mut dst);
            self.values[*out] = dst;
        }
    }

    fn exec(&mut self, op: &Op, out: usize, dst: &mut [f32]) {
        let last = |s: &[usize]| s.last().copied().unwrap_or(1);
        match op {
            Op::Conv {
                input,
                filter,
                bias,
                win,
                act,
            } => {
                let mut scratch = std::mem::take(&mut self.scratch);
                ops::conv2d(
                    self.get(*input),
                    last(self.shape(*input)),
                    self.get(*filter),
                    bias.map(|b| self.get(b)),
                    last(self.shape(out)),
                    win,
                    *act,
                    dst,
                    &mut scratch,
                );
                self.scratch = scratch;
            }
            Op::Depthwise {
                input,
                filter,
                bias,
                win,
                act,
            } => ops::depthwise(
                self.get(*input),
                last(self.shape(*input)),
                self.get(*filter),
                bias.map(|b| self.get(b)),
                win,
                *act,
                dst,
            ),
            Op::MaxPool { input, win, act } => {
                ops::max_pool(self.get(*input), last(self.shape(*input)), win, *act, dst)
            }
            Op::Add { a, b, act } => {
                ops::binary(
                    self.get(*a),
                    self.shape(*a),
                    self.get(*b),
                    self.shape(*b),
                    self.shape(out),
                    dst,
                    |x, y| x + y,
                );
                act.apply(dst);
            }
            Op::Prelu { input, alpha } => ops::binary(
                self.get(*input),
                self.shape(*input),
                self.get(*alpha),
                self.shape(*alpha),
                self.shape(out),
                dst,
                |x, a| if x >= 0.0 { x } else { x * a },
            ),
            Op::Relu(input) => {
                dst.copy_from_slice(self.get(*input));
                Act::Relu.apply(dst);
            }
            Op::Logistic(input) => ops::logistic(self.get(*input), dst),
            Op::Copy(input) => dst.copy_from_slice(self.get(*input)),
            Op::Pad { input, pads } => ops::pad(self.get(*input), self.shape(*input), pads, dst),
            Op::Concat { inputs, axis, act } => {
                let parts: Vec<(&[f32], &[usize])> = inputs
                    .iter()
                    .map(|&t| (self.get(t), self.shape(t)))
                    .collect();
                ops::concat(&parts, *axis, dst);
                act.apply(dst);
            }
            Op::Resize {
                input,
                align_corners,
                half_pixel,
            } => {
                let (is, os) = (self.shape(*input), self.shape(out));
                ops::resize_bilinear(
                    self.get(*input),
                    (is[1], is[2]),
                    is[3],
                    (os[1], os[2]),
                    *align_corners,
                    *half_pixel,
                    dst,
                );
            }
            Op::Mean { input, axes } => ops::mean(self.get(*input), self.shape(*input), axes, dst),
            Op::FullyConnected {
                input,
                weights,
                bias,
                act,
            } => {
                let ws = self.shape(*weights);
                ops::fully_connected(
                    self.get(*input),
                    ws[1],
                    self.get(*weights),
                    bias.map(|b| self.get(b)),
                    ws[0],
                    *act,
                    dst,
                );
            }
            Op::DepthToSpace { input, block } => {
                let is = self.shape(*input);
                ops::depth_to_space(self.get(*input), (is[1], is[2]), is[3], *block, dst);
            }
        }
    }
}
