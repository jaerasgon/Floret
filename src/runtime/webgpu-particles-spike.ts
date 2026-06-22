/**
 * SPIKE — WebGPU instanced particle renderer (render path only).
 *
 * Proof-of-concept to measure the ceiling of true GPU instancing vs the
 * Canvas2D per-particle loop. NOT wired into the app — it renders to its
 * own canvas, which in a real integration would be composited over the
 * Canvas2D scene (transparent background, `position: absolute` overlay, or
 * drawImage of its frame into the 2D context).
 *
 * One draw call → `count` instances. Per-particle (centre, size, rgba) live
 * in a single interleaved instance vertex buffer with `stepMode: 'instance'`;
 * the GPU expands a 2-triangle quad per particle and the fragment shader
 * rounds it into a soft disc. No `fillStyle`, no string parse, no per-particle
 * CPU draw call — per-particle colour is just a shader attribute.
 *
 * Instance layout (7 floats / 28 bytes, tightly packed):
 *   [0..1] centre x,y   (centred canvas coords, +y down)
 *   [2]    size (diameter, px)
 *   [3..6] colour r,g,b,a (0..1)
 */

export const PARTICLE_INSTANCE_FLOATS = 7;
const STRIDE = PARTICLE_INSTANCE_FLOATS * 4; // 28 bytes

const WGSL = /* wgsl */ `
struct Uniforms { halfW: f32, halfH: f32 };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VSOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) color: vec4f,
};

@vertex
fn vs(
  @builtin(vertex_index) vid: u32,
  @location(0) center: vec2f,
  @location(1) size: f32,
  @location(2) color: vec4f,
) -> VSOut {
  var corners = array<vec2f, 6>(
    vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0),
    vec2f(-1.0, 1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0)
  );
  let c = corners[vid];
  let world = center + c * (size * 0.5);
  var out: VSOut;
  // map centred canvas coords → clip space (flip y for screen down-positive)
  out.pos = vec4f(world.x / u.halfW, -world.y / u.halfH, 0.0, 1.0);
  out.uv = c;
  out.color = color;
  return out;
}

@fragment
fn fs(in: VSOut) -> @location(0) vec4f {
  let d = length(in.uv);
  if (d > 1.0) { discard; }
  let a = in.color.a * smoothstep(1.0, 0.82, d);
  // premultiplied output (matches alphaMode 'premultiplied' + over blend)
  return vec4f(in.color.rgb * a, a);
}
`;

export interface WebGPUParticleRenderer {
  readonly canvas: HTMLCanvasElement;
  /** Capacity in particles (instance buffer size). */
  readonly capacity: number;
  /** Upload `count` particles' interleaved instance data and draw them. */
  render(instanceData: Float32Array, count: number): void;
  /** Resolves when all submitted GPU work has completed (for benchmarking). */
  flush(): Promise<void>;
  resize(w: number, h: number): void;
  destroy(): void;
}

export interface WebGPUParticleOptions {
  canvas?: HTMLCanvasElement;
  width?: number;
  height?: number;
  /** Max particles to allocate the instance buffer for. */
  capacity?: number;
  /** 'over' = normal alpha; 'add' = additive (glowy). */
  blend?: 'over' | 'add';
}

/**
 * Initialise the renderer, or return null if WebGPU is unavailable. Async
 * because adapter/device acquisition is async.
 */
export async function createWebGPUParticleRenderer(
  opts: WebGPUParticleOptions = {},
): Promise<WebGPUParticleRenderer | null> {
  // Spike: the project's TS lib doesn't include WebGPU types and we don't
  // want a dependency for a proof-of-concept, so the GPU surface is `any`.
  /* eslint-disable @typescript-eslint/no-explicit-any */
  const g = globalThis as any;
  const gpu = (navigator as any).gpu;
  if (!gpu) return null;
  const adapter = await gpu.requestAdapter();
  if (!adapter) return null;
  const device = await adapter.requestDevice();

  const width = opts.width ?? 540;
  const height = opts.height ?? 540;
  const capacity = Math.max(1, Math.floor(opts.capacity ?? 1_000_000));

  const canvas = opts.canvas ?? document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('webgpu') as any;
  if (!ctx) return null;
  const format = gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format, alphaMode: 'premultiplied' });

  const module = device.createShaderModule({ code: WGSL });

  const blend: any = opts.blend === 'add'
    ? { color: { srcFactor: 'one', dstFactor: 'one' }, alpha: { srcFactor: 'one', dstFactor: 'one' } }
    : { color: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' } };

  const pipeline = device.createRenderPipeline({
    layout: 'auto',
    vertex: {
      module,
      entryPoint: 'vs',
      buffers: [{
        arrayStride: STRIDE,
        stepMode: 'instance',
        attributes: [
          { shaderLocation: 0, offset: 0,  format: 'float32x2' }, // center
          { shaderLocation: 1, offset: 8,  format: 'float32'   }, // size
          { shaderLocation: 2, offset: 12, format: 'float32x4' }, // color
        ],
      }],
    },
    fragment: { module, entryPoint: 'fs', targets: [{ format, blend }] },
    primitive: { topology: 'triangle-list' },
  });

  const BU = g.GPUBufferUsage;
  const instanceBuffer = device.createBuffer({
    size: capacity * STRIDE,
    usage: BU.VERTEX | BU.COPY_DST,
  });

  const uniformBuffer = device.createBuffer({
    size: 16, // two f32 + pad to 16
    usage: BU.UNIFORM | BU.COPY_DST,
  });
  const writeUniforms = (w: number, h: number) => {
    device.queue.writeBuffer(uniformBuffer, 0, new Float32Array([w / 2, h / 2, 0, 0]));
  };
  writeUniforms(width, height);

  const bindGroup = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer: uniformBuffer } }],
  });

  return {
    canvas,
    capacity,
    render(instanceData: Float32Array, count: number) {
      const n = Math.min(count, capacity);
      if (n <= 0) return;
      // upload only the live range
      device.queue.writeBuffer(instanceBuffer, 0, instanceData, 0, n * PARTICLE_INSTANCE_FLOATS);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [{
          view: ctx.getCurrentTexture().createView(),
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
          loadOp: 'clear',
          storeOp: 'store',
        }],
      });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, bindGroup);
      pass.setVertexBuffer(0, instanceBuffer);
      pass.draw(6, n); // 6 verts (quad) × n instances — ONE draw call
      pass.end();
      device.queue.submit([encoder.finish()]);
    },
    flush() {
      return device.queue.onSubmittedWorkDone();
    },
    resize(w: number, h: number) {
      canvas.width = w;
      canvas.height = h;
      writeUniforms(w, h);
    },
    destroy() {
      instanceBuffer.destroy();
      uniformBuffer.destroy();
      device.destroy();
    },
  };
}

/**
 * Pack a SoA ParticleBuffer's live particles into the interleaved instance
 * array this renderer expects. In a real integration you'd either keep the
 * data in this layout, or (better) upload the SoA arrays as separate
 * instance vertex buffers and skip packing entirely.
 */
export function packParticleInstances(
  out: Float32Array,
  buf: {
    capacity: number; alive: Uint8Array;
    posX: Float32Array; posY: Float32Array; size: Float32Array;
    colorR: Float32Array; colorG: Float32Array; colorB: Float32Array; colorA: Float32Array;
  },
): number {
  let o = 0;
  const { capacity, alive, posX, posY, size, colorR, colorG, colorB, colorA } = buf;
  for (let i = 0; i < capacity; i++) {
    if (!alive[i]) continue;
    out[o] = posX[i]!; out[o + 1] = posY[i]!; out[o + 2] = size[i]!;
    out[o + 3] = colorR[i]!; out[o + 4] = colorG[i]!; out[o + 5] = colorB[i]!; out[o + 6] = colorA[i]!;
    o += PARTICLE_INSTANCE_FLOATS;
  }
  return o / PARTICLE_INSTANCE_FLOATS;
}
