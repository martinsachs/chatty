// macOS: Die Titelleiste ist durchsichtig (tauri.conf.json `titleBarStyle`) und zeigt die Fensterfarbe.
// Fensterfarbe = Farbe der Kontoleiste der Optik; Titeltext hell oder dunkel je nach Helligkeit.
import { getCurrentWindow } from "@tauri-apps/api/window";

const IS_MAC = navigator.userAgent.includes("Mac OS X");

function isDark(hex: string): boolean {
  const rgb = hex.match(/^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
  if (!rgb) return false;
  const [red, green, blue] = rgb.slice(1).map((part) => parseInt(part, 16));
  return 0.299 * red + 0.587 * green + 0.114 * blue < 128;
}

/** Nach jedem Optikwechsel aufrufen, wenn `data-theme` schon gesetzt ist. */
export function applyTitleBar() {
  if (!IS_MAC) return;
  const railColor = getComputedStyle(document.documentElement).getPropertyValue("--rail-bg").trim();
  if (!railColor) return;
  const appWindow = getCurrentWindow();
  appWindow.setBackgroundColor(railColor).catch(() => {});
  appWindow.setTheme(isDark(railColor) ? "dark" : "light").catch(() => {});
}
