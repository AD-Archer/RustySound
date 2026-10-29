// Generated media: WAV tones for `stream`, PNG covers for `getCoverArt`.
import { deflateSync } from "node:zlib";
import { hashString } from "./library";

// ---------------------------------------------------------------- audio

export const SAMPLE_RATE = 11025; // 16-bit mono PCM: ~22 KB/s, decodes in WebKit, Chromium and Firefox
const BYTES_PER_SAMPLE = 2;
const WAV_HEADER = 44;

/** Optional cap on generated stream length (seconds). 0 = full song duration. */
export const STREAM_CAP_SECONDS = Number(process.env.MOCK_STREAM_CAP_SECONDS ?? "0") || 0;

export function wavLength(durationSeconds: number): number {
  const secs = STREAM_CAP_SECONDS > 0 ? Math.min(durationSeconds, STREAM_CAP_SECONDS) : durationSeconds;
  return WAV_HEADER + Math.round(secs * SAMPLE_RATE) * BYTES_PER_SAMPLE;
}

function wavHeader(dataBytes: number): Uint8Array {
  const buf = new ArrayBuffer(WAV_HEADER);
  const v = new DataView(buf);
  const w = (o: number, s: string) => { for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i)); };
  w(0, "RIFF");
  v.setUint32(4, 36 + dataBytes, true);
  w(8, "WAVE");
  w(12, "fmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, 1, true); // mono
  v.setUint32(24, SAMPLE_RATE, true);
  v.setUint32(28, SAMPLE_RATE * BYTES_PER_SAMPLE, true);
  v.setUint16(32, BYTES_PER_SAMPLE, true);
  v.setUint16(34, 16, true);
  w(36, "data");
  v.setUint32(40, dataBytes, true);
  return new Uint8Array(buf);
}

/**
 * Produce bytes [start, end] (inclusive) of a deterministic WAV tone for `seedKey`.
 * Samples are computed on demand, so Range requests never materialise the whole file.
 */
export function wavSlice(seedKey: string, durationSeconds: number, start: number, end: number): Uint8Array {
  const total = wavLength(durationSeconds);
  end = Math.min(end, total - 1);
  const out = new Uint8Array(Math.max(0, end - start + 1));
  if (out.length === 0) return out;
  const header = wavHeader(total - WAV_HEADER);
  const h = hashString(seedKey);
  // Two quiet partials so different songs sound different; a slow tremolo marks the passage of time.
  const f1 = 196 + (h % 24) * 11; // ~196..449 Hz
  const f2 = f1 * 1.5;
  const amp = 2600;
  let o = 0;
  for (let pos = start; pos <= end; ) {
    if (pos < WAV_HEADER) {
      out[o++] = header[pos++];
      continue;
    }
    const sampleIndex = Math.floor((pos - WAV_HEADER) / 2);
    const t = sampleIndex / SAMPLE_RATE;
    const trem = 0.65 + 0.35 * Math.sin(2 * Math.PI * 0.5 * t);
    const s = Math.round(amp * trem * (Math.sin(2 * Math.PI * f1 * t) * 0.7 + Math.sin(2 * Math.PI * f2 * t) * 0.3));
    const u = s & 0xffff;
    const lowByte = ((pos - WAV_HEADER) & 1) === 0;
    out[o++] = lowByte ? u & 0xff : (u >> 8) & 0xff;
    pos++;
  }
  return out;
}

// ---------------------------------------------------------------- images

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const v = new DataView(out.buffer);
  v.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  v.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

function hsl(h: number, s: number, l: number): [number, number, number] {
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  };
  return [f(0), f(8), f(4)];
}

const pngCache = new Map<string, Uint8Array>();

/** Deterministic cover: a two-colour diagonal gradient with a ring, unique per id. */
export function coverPng(id: string, size: number): Uint8Array {
  size = Math.max(16, Math.min(1024, Math.round(size) || 300));
  const key = `${id}@${size}`;
  const hit = pngCache.get(key);
  if (hit) return hit;
  const h = hashString(id);
  const c1 = hsl(h % 360, 0.55, 0.42);
  const c2 = hsl((h >>> 9) % 360, 0.6, 0.18);
  const ringR = 0.18 + ((h >>> 3) % 20) / 100;
  const raw = new Uint8Array(size * (size * 3 + 1));
  let p = 0;
  for (let y = 0; y < size; y++) {
    raw[p++] = 0; // filter: none
    for (let x = 0; x < size; x++) {
      const t = (x + y) / (2 * size);
      const dx = x / size - 0.5;
      const dy = y / size - 0.5;
      const d = Math.sqrt(dx * dx + dy * dy);
      const ring = Math.abs(d - ringR) < 0.02 ? 0.35 : 0;
      for (let ch = 0; ch < 3; ch++) {
        const base = c1[ch] * (1 - t) + c2[ch] * t;
        raw[p++] = Math.min(255, Math.round(base + ring * 255 * 0.5));
      }
    }
  }
  const ihdr = new Uint8Array(13);
  const v = new DataView(ihdr.buffer);
  v.setUint32(0, size);
  v.setUint32(4, size);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  const sig = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  const parts = [sig, chunk("IHDR", ihdr), chunk("IDAT", new Uint8Array(deflateSync(raw))), chunk("IEND", new Uint8Array())];
  const len = parts.reduce((n, a) => n + a.length, 0);
  const png = new Uint8Array(len);
  let off = 0;
  for (const part of parts) { png.set(part, off); off += part.length; }
  if (pngCache.size > 4000) pngCache.clear();
  pngCache.set(key, png);
  return png;
}
