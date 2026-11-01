// Reads a cover as 32x32 RGB for the palette (art_palette/mod.rs).
// The image decodes off the main thread (img.decode()); drawing 32x32 and
// reading it back is a few microseconds. Returns [r, g, b, …] for the opaque
// pixels, or null when the cover can't be read (no CORS, failed load).
window.__rsArtSample ??= async (url) => {
  const img = new Image();
  // Server covers on the web are cross-origin: ask for CORS so the canvas
  // stays readable. Native covers come from the app's own route.
  try {
    if (new URL(url, location.href).origin !== location.origin && !url.startsWith("data:")) {
      img.crossOrigin = "anonymous";
    }
  } catch {}
  img.decoding = "async";
  img.src = url;
  try {
    await img.decode();
  } catch {
    return null;
  }
  const size = 32;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.imageSmoothingQuality = "medium";
  ctx.drawImage(img, 0, 0, size, size);
  let data;
  try {
    data = ctx.getImageData(0, 0, size, size).data;
  } catch {
    return null;
  }
  const out = [];
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) continue;
    out.push(data[i], data[i + 1], data[i + 2]);
  }
  return out;
};
