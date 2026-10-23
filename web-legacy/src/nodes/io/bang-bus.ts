import { markDirty } from '../../runtime/dirty';

/**
 * One-shot trigger bus shared by the Bang node and its editor component.
 *
 * A "bang" is momentary: clicking the button (or any caller of `fireBang`)
 * makes the node emit `true` for exactly one tick, then `false`. We can't
 * hold the value in a param (it would latch like a Toggle), so the click
 * sets a pending flag and wakes a tick; the node consumes it once.
 *
 * `consumeBang` returns true once, clears the flag, and wakes ONE more tick
 * so the follow-up frame emits `false` — a clean false→true→false pulse even
 * when the patch is otherwise idle (no continuous animation needed).
 */
const pending = new Map<string, boolean>();

export function fireBang(id: string): void {
  pending.set(id, true);
  markDirty();
}

export function consumeBang(id: string): boolean {
  if (pending.get(id)) {
    pending.set(id, false);
    markDirty(); // force a follow-up tick that emits the trailing `false`
    return true;
  }
  return false;
}
