// Prüft beim Start auf ein signiertes Update (GitHub Releases, latest.json).
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { chat } from "./chat.svelte";

export async function checkForUpdate() {
  if (import.meta.env.DEV) return;
  try {
    const update = await check();
    if (!update) return;
    chat.toast(`Version ${update.version} verfügbar`, "info", {
      label: "Installieren",
      run: async () => {
        chat.toast("Update wird geladen …");
        await update.downloadAndInstall();
        await relaunch();
      },
    });
  } catch (e) {
    console.warn("Update-Prüfung fehlgeschlagen", e);
  }
}
