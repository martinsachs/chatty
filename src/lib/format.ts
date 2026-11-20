// Kleine, sichere Textformatierung: erst HTML escapen, dann wenige Markdown-Regeln.

const ESC: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ESC[c]);
}

/** Markdown-Teilmenge (Mattermost/Slack/Google): ```code```, `code`, **fett**, *fett*, _kursiv_, ~durch~, Links, @Erwähnungen. */
export function formatText(text: string): string {
  const blocks: string[] = [];
  let s = escapeHtml(text).replace(/```(?:[a-z]*\n)?([\s\S]*?)\n?```\n?/g, (_, code) => {
    blocks.push(`<pre><code>${code}</code></pre>`);
    return `\u0000${blocks.length - 1}\u0000`;
  });
  s = s
    .replace(/`([^`\n]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[\s(\u0000])\*([^*\n]+)\*(?=$|[\s.,!?)])/g, "$1<strong>$2</strong>")
    .replace(/(^|[\s(\u0000])_([^_\n]+)_(?=$|[\s.,!?)])/g, "$1<em>$2</em>")
    .replace(/(^|[\s(\u0000])~~?([^~\n]+)~~?(?=$|[\s.,!?)])/g, "$1<del>$2</del>")
    .replace(/(https?:\/\/[^\s<]+[^\s<.,;:!?)'"])/g, '<a href="$1" target="_blank" rel="noreferrer">$1</a>')
    .replace(/(^|[\s\u0000])(@[\w.-]+)/g, '$1<span class="mention">$2</span>')
    .replace(/\n/g, "<br>");
  return s.replace(/\u0000(\d+)\u0000/g, (_, i) => blocks[Number(i)]);
}

export function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "?") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
}

/** Stabile Farbe je Name für Avatare ohne Bild. */
export function colorFor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `hsl(${h % 360} 45% 45%)`;
}

const timeFmt = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });
const dayFmt = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long", year: "numeric" });

export function time(ms: number): string {
  return timeFmt.format(ms);
}

export function day(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const yesterday = new Date(Date.now() - 86_400_000);
  if (d.toDateString() === today.toDateString()) return "Heute";
  if (d.toDateString() === yesterday.toDateString()) return "Gestern";
  return dayFmt.format(d);
}

export function size(bytes?: number | null): string {
  if (!bytes) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export const QUICK_EMOJI = ["👍", "❤️", "😄", "🎉", "👀", "✅", "🙏", "🚀", "😂", "🤔", "👎", "🔥"];
