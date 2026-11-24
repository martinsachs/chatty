// Schriftgröße als Zoom der WebView (skaliert Text, Emojis und Abstände gemeinsam), je Rechner in localStorage.
import { getCurrentWebview } from "@tauri-apps/api/webview";

export const ZOOMS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
const KEY = "zoom";

function load(): number {
  try {
    const z = Number(localStorage.getItem(KEY));
    return ZOOMS.includes(z) ? z : 1;
  } catch {
    return 1;
  }
}

export const zoom = $state({ value: load() });

export function applyZoom() {
  getCurrentWebview()
    .setZoom(zoom.value)
    .catch(() => {});
}

export function setZoom(z: number) {
  zoom.value = ZOOMS.includes(z) ? z : 1;
  try {
    localStorage.setItem(KEY, String(zoom.value));
  } catch {
    // ohne Speicher gilt die Größe bis zum Neustart
  }
  applyZoom();
}

/** Strg/Cmd + Plus/Minus/0 wie im Browser. */
export function zoomKey(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  const i = ZOOMS.indexOf(zoom.value);
  if (e.key === "+" || e.key === "=") setZoom(ZOOMS[Math.min(i + 1, ZOOMS.length - 1)]);
  else if (e.key === "-") setZoom(ZOOMS[Math.max(i - 1, 0)]);
  else if (e.key === "0") setZoom(1);
  else return;
  e.preventDefault();
}

/** Andere Fenster (Popouts) übernehmen eine Änderung. */
export function zoomStorage(e: StorageEvent) {
  if (e.key === KEY) {
    zoom.value = load();
    applyZoom();
  }
}
