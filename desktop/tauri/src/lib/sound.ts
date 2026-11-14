// KOTOR-style UI click sound.
//
// The KOTOR menu "select" blip is a short, resonant, slightly metallic
// electronic twing - a warm two-tone chirp with a hollow body, not a bright
// typewriter tick. We synthesize it with the Web Audio API instead of shipping
// an audio file, so there is no binary asset to bundle and nothing to download.
// The click is played globally whenever the user activates an interactive
// element, but only while the preference is enabled (it is OFF by default).

const STORAGE_KEY = "kmi.uiSounds";

let enabled = readInitial();
let ctx: AudioContext | null = null;
let installed = false;

function readInitial(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function audioCtx(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const Ctor = window.AudioContext ?? (window as any).webkitAudioContext;
  if (!Ctor) return null;
  if (!ctx) ctx = new Ctor();
  // A context created before a user gesture starts suspended; resume it.
  if (ctx.state === "suspended") ctx.resume().catch(() => {});
  return ctx;
}

export function isUiSoundEnabled(): boolean {
  return enabled;
}

export function setUiSoundEnabled(value: boolean): void {
  enabled = value;
  try {
    localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
  } catch {
    /* persistence is best-effort */
  }
}

/** Play one KOTOR-style menu click. No-op if sounds are disabled. */
export function playClick(): void {
  if (!enabled) return;
  const ac = audioCtx();
  if (!ac) return;

  const now = ac.currentTime;
  const out = ac.createGain();
  out.gain.value = 0.16; // master volume for the blip
  out.connect(ac.destination);

  // A resonant band-pass gives the click its hollow, "computer console" body -
  // the character that makes it read as KOTOR rather than a generic beep.
  const band = ac.createBiquadFilter();
  band.type = "bandpass";
  band.frequency.value = 1150;
  band.Q.value = 5;
  band.connect(out);

  // Two partials make the short downward "twing": a warm base tone plus a
  // quieter octave above, both easing down in pitch for the retro-futuristic feel.
  const partials: { type: OscillatorType; f0: number; f1: number; gain: number }[] = [
    { type: "triangle", f0: 940, f1: 660, gain: 1.0 },
    { type: "sine", f0: 1880, f1: 1480, gain: 0.35 },
  ];
  for (const p of partials) {
    const osc = ac.createOscillator();
    osc.type = p.type;
    osc.frequency.setValueAtTime(p.f0, now);
    osc.frequency.exponentialRampToValueAtTime(p.f1, now + 0.05);
    const g = ac.createGain();
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(p.gain, now + 0.005);
    g.gain.exponentialRampToValueAtTime(0.0001, now + 0.11);
    osc.connect(g).connect(band);
    osc.start(now);
    osc.stop(now + 0.12);
  }

  // A soft, low-passed transient adds a gentle attack edge - just enough click
  // without the bright, typewriter-like snap the old version had.
  const noise = ac.createBufferSource();
  const buf = ac.createBuffer(1, Math.ceil(ac.sampleRate * 0.015), ac.sampleRate);
  const data = buf.getChannelData(0);
  for (let i = 0; i < data.length; i++) {
    data[i] = (Math.random() * 2 - 1) * (1 - i / data.length);
  }
  noise.buffer = buf;
  const lp = ac.createBiquadFilter();
  lp.type = "lowpass";
  lp.frequency.value = 3200;
  const noiseGain = ac.createGain();
  noiseGain.gain.setValueAtTime(0.16, now);
  noiseGain.gain.exponentialRampToValueAtTime(0.0001, now + 0.012);
  noise.connect(lp).connect(noiseGain).connect(out);
  noise.start(now);
  noise.stop(now + 0.015);
}

// Elements whose activation should click. Covers buttons, links, the custom
// switch (role="switch"), native selects, checkboxes/radios, and ARIA widgets.
const INTERACTIVE =
  'button, a[href], summary, select, [role="button"], [role="switch"], ' +
  '[role="tab"], [role="menuitem"], [role="option"], ' +
  'input[type="checkbox"], input[type="radio"]';

function handleClick(e: MouseEvent): void {
  if (!enabled) return;
  if (e.button !== 0) return; // primary clicks only
  const target = e.target as Element | null;
  const el = target?.closest?.(INTERACTIVE) as HTMLElement | null;
  if (!el) return;
  if (el.hasAttribute("disabled") || el.getAttribute("aria-disabled") === "true") return;
  playClick();
}

/**
 * Initialize global UI sounds: refresh the enabled flag from storage and attach
 * the document-wide click listener. Safe to call more than once.
 */
export function initUiSounds(): void {
  enabled = readInitial();
  if (installed || typeof document === "undefined") return;
  installed = true;
  // Capture phase so it fires even when a handler stops propagation.
  document.addEventListener("click", handleClick, true);
}
