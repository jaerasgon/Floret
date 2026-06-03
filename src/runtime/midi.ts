/**
 * MIDI input registry.
 *
 * Wraps the Web MIDI API (`navigator.requestMIDIAccess`) so node defs
 * can read controller state each tick without each node managing its
 * own MIDIAccess subscription.
 *
 * Model: the registry keeps a "latest value" snapshot per (channel,
 * cc) and (channel, note). Nodes read those snapshots from compute.
 * That's enough for parameter-control workflows — a knob's value is
 * intrinsically the LAST value it sent.
 *
 * For note-pressed state we track a per-(channel, note) boolean —
 * true between note-on (velocity > 0) and note-off (or note-on
 * velocity 0). For monophonic last-note semantics, a stack of held
 * notes per channel is also kept.
 *
 * Permission: `navigator.requestMIDIAccess()` resolves without a UI
 * prompt in Chromium for non-sysex use. Safari may reject — the
 * registry catches that and stays in a passive state (every read
 * returns the default).
 *
 * Lifecycle: a single MIDIAccess for the lifetime of the page. New
 * devices connected mid-session are auto-attached via the
 * `statechange` event.
 */

/**
 * Per-device state. Keying by deviceId means multiple boards can
 * route to different nodes without their values colliding.
 *
 * Keys inside each map use `${channel}:${cc}` or `${channel}:${note}`.
 * The outer key is the MIDIInput.id (stable per hardware per browser).
 */
interface DeviceState {
  cc: Map<string, number>;
  noteVelocity: Map<string, number>;
  heldNotes: Map<number, number[]>;
}

interface DeviceInfo {
  id: string;
  name: string;
}

class MidiRegistry {
  private byDevice = new Map<string, DeviceState>();
  private access: MIDIAccess | null = null;
  private inflight: Promise<void> | null = null;
  private inputDevices: DeviceInfo[] = [];
  /** Cached list returned from `deviceOptions()`. Rebuilt on attach/detach,
   *  identity-stable in between so React's useSyncExternalStore doesn't
   *  thrash. */
  private deviceOptionsCache: ReadonlyArray<{ value: string; label: string }> = [];
  private listeners = new Set<() => void>();

  /**
   * Lazily request MIDI access. Idempotent — multiple calls share a
   * single in-flight promise.
   */
  ensureAccess(): Promise<void> {
    if (this.access) return Promise.resolve();
    if (this.inflight) return this.inflight;
    if (typeof navigator === 'undefined' || !navigator.requestMIDIAccess) {
      // No Web MIDI in this browser. Stay passive; reads return defaults.
      return Promise.resolve();
    }
    this.inflight = navigator
      .requestMIDIAccess({ sysex: false })
      .then((access) => {
        this.access = access;
        this.attachAllInputs();
        access.onstatechange = () => this.attachAllInputs();
      })
      .catch((err) => {
        // eslint-disable-next-line no-console
        console.warn('MIDI access denied or unavailable:', err);
      })
      .finally(() => {
        this.inflight = null;
      });
    return this.inflight;
  }

  /** Names of currently-attached input devices. Useful for the editor's
   *  param hint UX so the user knows what's connected. */
  deviceNames(): ReadonlyArray<string> {
    return this.inputDevices.map((d) => d.name);
  }

  /**
   * Identity-stable list of `{value: deviceId, label: name}` pairs for
   * the per-node device dropdown. Rebuilt only when the device list
   * changes — useSyncExternalStore relies on reference equality.
   */
  deviceOptions(): ReadonlyArray<{ value: string; label: string }> {
    return this.deviceOptionsCache;
  }

  /** Subscribe to device-list changes (attach / detach). Returns an
   *  unsubscribe fn. */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  }

  private ensureDeviceState(deviceId: string): DeviceState {
    let s = this.byDevice.get(deviceId);
    if (!s) {
      s = { cc: new Map(), noteVelocity: new Map(), heldNotes: new Map() };
      this.byDevice.set(deviceId, s);
    }
    return s;
  }

  /**
   * Latest CC value (0..127) on the given device + channel.
   * `deviceId === ''` means any device; `channel === 0` means any channel.
   * Returns -1 when nothing has been received yet.
   */
  getCC(deviceId: string, channel: number, cc: number): number {
    const devs = deviceId ? [this.byDevice.get(deviceId)].filter(Boolean) as DeviceState[]
                          : [...this.byDevice.values()];
    let v = -1;
    for (const ds of devs) {
      if (channel === 0) {
        for (let ch = 1; ch <= 16; ch++) {
          const got = ds.cc.get(`${ch}:${cc}`);
          if (got !== undefined) v = got;
        }
      } else {
        const got = ds.cc.get(`${channel}:${cc}`);
        if (got !== undefined) v = got;
      }
    }
    return v;
  }

  /**
   * Most recently pressed note still held on (device, channel).
   * `deviceId === ''` means any device; `channel === 0` means any channel.
   * -1 if nothing held.
   */
  getLastHeldNote(deviceId: string, channel: number): number {
    const devs = deviceId ? [this.byDevice.get(deviceId)].filter(Boolean) as DeviceState[]
                          : [...this.byDevice.values()];
    let best = -1;
    for (const ds of devs) {
      if (channel === 0) {
        for (const stack of ds.heldNotes.values()) {
          const top = stack[stack.length - 1];
          if (top !== undefined) best = top;
        }
      } else {
        const stack = ds.heldNotes.get(channel);
        const top = stack?.[stack.length - 1];
        if (top !== undefined) best = top;
      }
    }
    return best;
  }

  /** Velocity (0..127) of `note` on (device, channel), or 0 if not held. */
  getNoteVelocity(deviceId: string, channel: number, note: number): number {
    const devs = deviceId ? [this.byDevice.get(deviceId)].filter(Boolean) as DeviceState[]
                          : [...this.byDevice.values()];
    for (const ds of devs) {
      if (channel === 0) {
        for (let ch = 1; ch <= 16; ch++) {
          const v = ds.noteVelocity.get(`${ch}:${note}`);
          if (v !== undefined) return v;
        }
      } else {
        const v = ds.noteVelocity.get(`${channel}:${note}`);
        if (v !== undefined) return v;
      }
    }
    return 0;
  }

  /** Is any note held on (device, channel)? */
  isAnyNoteHeld(deviceId: string, channel: number): boolean {
    const devs = deviceId ? [this.byDevice.get(deviceId)].filter(Boolean) as DeviceState[]
                          : [...this.byDevice.values()];
    for (const ds of devs) {
      if (channel === 0) {
        for (const stack of ds.heldNotes.values()) {
          if (stack.length > 0) return true;
        }
      } else if ((ds.heldNotes.get(channel)?.length ?? 0) > 0) {
        return true;
      }
    }
    return false;
  }

  private attachAllInputs(): void {
    if (!this.access) return;
    const devices: DeviceInfo[] = [];
    const seenIds = new Set<string>();
    for (const input of this.access.inputs.values()) {
      const id = input.id;
      const name = input.name ?? input.id;
      devices.push({ id, name });
      seenIds.add(id);
      // Replacing the handler each time is harmless — same closure shape.
      input.onmidimessage = (e: MIDIMessageEvent) => this.handle(id, e);
    }
    // Drop state for devices that detached. Keeping it around would
    // make `getCC(deviceId=anything)` continue to report a stale value.
    for (const id of [...this.byDevice.keys()]) {
      if (!seenIds.has(id)) this.byDevice.delete(id);
    }
    this.inputDevices = devices;
    this.deviceOptionsCache = devices.map((d) => ({ value: d.id, label: d.name }));
    // Notify subscribers (the device dropdown's useSyncExternalStore
    // re-renders on each call).
    for (const l of this.listeners) l();
  }

  private handle(deviceId: string, event: MIDIMessageEvent): void {
    const data = event.data;
    if (!data || data.length < 2) return;
    const status = data[0]!;
    const type = status & 0xf0;
    const channel = (status & 0x0f) + 1; // 1..16
    const ds = this.ensureDeviceState(deviceId);

    if (type === 0xb0) {
      const cc = data[1]!;
      const value = data[2] ?? 0;
      ds.cc.set(`${channel}:${cc}`, value);
      return;
    }

    if (type === 0x90) {
      // Note On — but velocity 0 is the conventional way to send Note Off,
      // so route it through the off path.
      const note = data[1]!;
      const velocity = data[2] ?? 0;
      if (velocity === 0) {
        this.noteOff(ds, channel, note);
      } else {
        this.noteOn(ds, channel, note, velocity);
      }
      return;
    }

    if (type === 0x80) {
      const note = data[1]!;
      this.noteOff(ds, channel, note);
      return;
    }
    // Other message types (pitch bend, aftertouch, sysex, clock) — not
    // surfaced yet. Add nodes for them later if needed.
  }

  private noteOn(ds: DeviceState, channel: number, note: number, velocity: number): void {
    ds.noteVelocity.set(`${channel}:${note}`, velocity);
    let stack = ds.heldNotes.get(channel);
    if (!stack) {
      stack = [];
      ds.heldNotes.set(channel, stack);
    }
    // If the note is already in the stack (retrigger without note-off),
    // move it to the top rather than duplicating.
    const idx = stack.indexOf(note);
    if (idx >= 0) stack.splice(idx, 1);
    stack.push(note);
  }

  private noteOff(ds: DeviceState, channel: number, note: number): void {
    ds.noteVelocity.delete(`${channel}:${note}`);
    const stack = ds.heldNotes.get(channel);
    if (!stack) return;
    const idx = stack.indexOf(note);
    if (idx >= 0) stack.splice(idx, 1);
  }
}

export const midiRegistry = new MidiRegistry();

/**
 * App-boot helper. Called from main.tsx — no-op if there's no Web MIDI
 * support. We don't wait for the promise; nodes that read MIDI state
 * just return -1 / 0 until the access resolves, then start emitting
 * live values automatically.
 */
export function installMidiHook(): void {
  void midiRegistry.ensureAccess();
}

// Dev-only: expose the registry on window for browser debugging.
if (typeof window !== 'undefined') {
  (window as unknown as { __midiRegistry?: unknown }).__midiRegistry = midiRegistry;
}
