// Adaptive gain for the HUD's waveform. The bars are scaled to how loud you
// have been speaking, not to an absolute level, so a whisper moves them about
// as much as a normal voice does.
//
// Works in dB on the meter's ~30 Hz RMS frames, with two followers:
//   floor — the room: the quietest frame of the last ~1.5 s. Gaps between
//           words keep pinning it down, so speech never drags it up.
//   voice — how loud your syllables peak, on average: a running average of
//           the upper envelope while you're voiced. Peaks rather than every
//           voiced frame, because a whisper only clears the floor at its
//           peaks and would otherwise read as louder than it is. It carries
//           over between dictations (and restarts), so a whisperer starts out
//           already sensitive.
// A frame's height is set by where it sits below your usual peak, and a soft
// gate above the floor keeps the room itself at rest.

const MIN_DB = -90;
const FLOOR_FRAMES = 45; // ~1.5 s
const VOICED_DB = 6; // this far above the floor counts as voice
const PEAK_LEVEL = 0.9; // your usual peak's height
const RANGE_DB = 20; // the bars fall to rest this far below it
const GATE_DB = 4.5; // the bars start to wake this far above the floor…
const GATE_SOFT_DB = 6; // …and are fully awake this much further up
const RELEASE_DB = 0.3; // the upper envelope falls ~9 dB/s between syllables
const VOICE_UP_K = 0.04; // louder: settle over ~1 s of speech (brief clipping is harmless)
const VOICE_DOWN_K = 0.08; // quieter: settle faster, so a whisper isn't left flat
const VOICE_DEFAULT_DB = -26; // a normal speaking voice, until we hear yours
const VOICE_KEY = "una.hud.voiceDb";

function loadVoice(): number {
  try {
    const v = Number(localStorage.getItem(VOICE_KEY));
    if (v && Number.isFinite(v)) return v;
  } catch {}
  return VOICE_DEFAULT_DB;
}

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));

export class LevelNormalizer {
  private floorRing = new Float32Array(FLOOR_FRAMES);
  private floorLen = 0;
  private floorAt = 0;
  private envelope = MIN_DB;
  private voice = loadVoice();

  /** Start a new dictation: the room may have changed, the voice hasn't. */
  reset() {
    this.floorLen = 0;
    this.floorAt = 0;
    this.envelope = MIN_DB;
  }

  /** Remember the voice level for the next launch. */
  save() {
    try {
      localStorage.setItem(VOICE_KEY, this.voice.toFixed(1));
    } catch {}
  }

  /** Feed one RMS frame (linear, 0..1); returns the display level, 0..1. */
  push(rms: number): number {
    // A stream that's still opening delivers exact zeros; they say nothing
    // about the room.
    if (!(rms > 0)) return 0;
    const db = Math.max(MIN_DB, 20 * Math.log10(rms));

    this.floorRing[this.floorAt] = db;
    this.floorAt = (this.floorAt + 1) % FLOOR_FRAMES;
    if (this.floorLen < FLOOR_FRAMES) this.floorLen++;
    let floor = Infinity;
    for (let i = 0; i < this.floorLen; i++) floor = Math.min(floor, this.floorRing[i]);

    this.envelope = Math.max(db, this.envelope - RELEASE_DB);
    if (db > floor + VOICED_DB) {
      const k = this.envelope > this.voice ? VOICE_UP_K : VOICE_DOWN_K;
      this.voice += (this.envelope - this.voice) * k;
      this.voice = Math.min(-6, Math.max(-70, this.voice));
    }

    const level = PEAK_LEVEL - (PEAK_LEVEL * (this.voice - db)) / RANGE_DB;
    const gate = (db - floor - GATE_DB) / GATE_SOFT_DB;
    return clamp01(level) * clamp01(gate);
  }
}
