// Töne bei Benachrichtigungen: synthetisch (Web Audio, keine Aufnahmen, keine Dateien).
import type { Theme } from "./api";

/** Spielt einen Ton ab `audio.currentTime` in `output`. */
type Sound = (audio: AudioContext, output: AudioNode) => void;

const VOLUME = 0.6;
/** Viele Nachrichten auf einmal (Nachholen nach Verbindungsabbruch) ergeben höchstens einen Ton. */
const MIN_INTERVAL_MS = 2000;

// ---------- Ping: Standardton, zwei Sinustöne im Quintabstand ----------

interface Tone {
  offsetSec: number;
  hz: number;
  durationSec: number;
}

const PING: Tone[] = [
  { offsetSec: 0, hz: 880, durationSec: 0.25 },
  { offsetSec: 0.1, hz: 1320, durationSec: 0.3 },
];

const ping: Sound = (audio, output) => {
  for (const tone of PING) {
    const start = audio.currentTime + tone.offsetSec;
    const end = start + tone.durationSec;
    const oscillator = audio.createOscillator();
    oscillator.frequency.value = tone.hz;
    const envelope = audio.createGain();
    envelope.gain.setValueAtTime(0, start);
    envelope.gain.linearRampToValueAtTime(0.5, start + 0.005);
    // Exponentiell klingt wie ein angeschlagenes Glas; die Rampe darf nicht auf 0 enden.
    envelope.gain.exponentialRampToValueAtTime(0.001, end);
    oscillator.connect(envelope).connect(output);
    oscillator.start(start);
    oscillator.stop(end);
  }
};

// ---------- Uh-oh (seequ): Cartoon-Stimme aus Obertönen und Formantfiltern ----------

/** Formanten der Vokale (Erwachsenenstimme), mit FORMANT_SCALE zur kleinen, hellen Cartoon-Stimme verschoben. */
const VOWELS = {
  uh: [640, 1190, 2390],
  oh: [500, 900, 2400],
  oo: [340, 720, 2250],
} as const;
type Vowel = keyof typeof VOWELS;

const FORMANT_SCALE = 1.3;
const FORMANT_GAINS = [1, 0.6, 0.25];
const FORMANT_Q = 7;

interface Syllable {
  offsetSec: number;
  durationSec: number;
  pitchHz: [number, number];
  vowels: [Vowel, Vowel];
  attackSec: number;
  releaseSec: number;
}

// „uh“ endet hart (Stimmritzenverschluss), „oh“ setzt hart ein, fällt und gleitet Richtung „u“.
const UH_OH: Syllable[] = [
  { offsetSec: 0, durationSec: 0.13, pitchHz: [500, 520], vowels: ["uh", "uh"], attackSec: 0.015, releaseSec: 0.008 },
  { offsetSec: 0.18, durationSec: 0.24, pitchHz: [450, 340], vowels: ["oh", "oo"], attackSec: 0.004, releaseSec: 0.09 },
];

/** Obertöne fallen steiler ab als beim Sägezahn: klingt weniger nach Summer. */
function createGlottalWave(audio: AudioContext): PeriodicWave {
  const harmonicCount = 40;
  const real = new Float32Array(harmonicCount + 1);
  const imag = new Float32Array(harmonicCount + 1);
  for (let harmonic = 1; harmonic <= harmonicCount; harmonic++) imag[harmonic] = 1 / harmonic ** 1.4;
  return audio.createPeriodicWave(real, imag);
}

function playSyllable(audio: AudioContext, wave: PeriodicWave, output: AudioNode, syllable: Syllable) {
  const start = audio.currentTime + syllable.offsetSec;
  const end = start + syllable.durationSec;
  const [fromVowel, toVowel] = syllable.vowels;

  const voice = audio.createOscillator();
  voice.setPeriodicWave(wave);
  voice.frequency.setValueAtTime(syllable.pitchHz[0], start);
  voice.frequency.linearRampToValueAtTime(syllable.pitchHz[1], end);

  const envelope = audio.createGain();
  envelope.gain.setValueAtTime(0, start);
  envelope.gain.linearRampToValueAtTime(1, start + syllable.attackSec);
  envelope.gain.setValueAtTime(1, end - syllable.releaseSec);
  envelope.gain.linearRampToValueAtTime(0, end);
  envelope.connect(output);

  VOWELS[fromVowel].forEach((fromHz, index) => {
    const formant = audio.createBiquadFilter();
    formant.type = "bandpass";
    formant.Q.value = FORMANT_Q;
    formant.frequency.setValueAtTime(fromHz * FORMANT_SCALE, start);
    formant.frequency.linearRampToValueAtTime(VOWELS[toVowel][index] * FORMANT_SCALE, end);
    const level = audio.createGain();
    level.gain.value = FORMANT_GAINS[index];
    voice.connect(formant).connect(level).connect(envelope);
  });

  voice.start(start);
  voice.stop(end);
}

const uhOh: Sound = (audio, output) => {
  const wave = createGlottalWave(audio);
  for (const syllable of UH_OH) playSyllable(audio, wave, output, syllable);
};

// ---------- Auswahl und Wiedergabe ----------

/** Ton je Optik; ohne Eintrag spielt `ping`. Neuer Ton: Funktion vom Typ `Sound` schreiben und hier eintragen. */
const THEME_SOUNDS: Partial<Record<Theme, Sound>> = {
  seequ: uhOh,
};

let context: AudioContext | null = null;
let lastPlayedMs = 0;

export function playNotificationSound(theme: Theme) {
  const now = Date.now();
  if (now - lastPlayedMs < MIN_INTERVAL_MS) return;
  lastPlayedMs = now;
  try {
    context ??= new AudioContext();
    // Ein neuer AudioContext kann bis zur ersten Nutzeraktion angehalten sein.
    context.resume().catch((e) => console.warn("Ton fortsetzen:", e));
    const master = context.createGain();
    master.gain.value = VOLUME;
    master.connect(context.destination);
    (THEME_SOUNDS[theme] ?? ping)(context, master);
  } catch (e) {
    console.warn("Ton abspielen:", e);
  }
}
