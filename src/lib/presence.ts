// Online-Status: Beschriftung und Statusmenü. Die Abbildung auf den Server macht der Kern.
import type { Presence } from "./api";

/** Beschriftung; bei den Zwischenstufen sehen andere sie auch als Status-Text. */
export const PRESENCE_LABELS: Record<Presence, string> = {
  online: "Online",
  available_to_chat: "Gesprächsbereit",
  away: "Abwesend",
  not_available: "Nicht verfügbar",
  busy: "Beschäftigt",
  do_not_disturb: "Nicht stören",
  invisible: "Unsichtbar",
  offline: "Offline",
};

/** Auswahl im Statusmenü (offline ergibt sich nur aus der Verbindung). */
export const PRESENCE_CHOICES: Presence[] = [
  "online",
  "available_to_chat",
  "away",
  "not_available",
  "busy",
  "do_not_disturb",
  "invisible",
];
