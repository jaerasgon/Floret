import type { FrameCtx } from './types';

/**
 * Mutable backing record + frozen accessor. The runtime updates the backing
 * record once per frame; nodes read the frozen view. Mutation is contained
 * here so we never accidentally write to a FrameCtx from a node.
 */
class FrameCtxStore {
  private state = {
    t: 0,
    dt: 0,
    frameCount: 0,
    width: 0,
    height: 0,
    mouseX: 0,
    mouseY: 0,
    mouseIsPressed: false,
    key: '',
    keyIsPressed: false,
  };

  update(partial: Partial<FrameCtx>): void {
    Object.assign(this.state, partial);
  }

  snapshot(): FrameCtx {
    return { ...this.state };
  }
}

export const frameCtxStore = new FrameCtxStore();
