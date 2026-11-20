// Zentraler UI-Zustand. Ein Fenster = eine Instanz (Hauptfenster oder abgelöstes Chatfenster).
import { listen } from "@tauri-apps/api/event";
import { api, emojiSrc, type AccountInfo, type ChatEvent, type Message, type Space, type Theme } from "./api";

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error" | "message";
  action?: { label: string; run: () => void };
}

/** Rechte Seitenleiste: Thread oder Suchergebnisse. */
export type Rhs =
  | { kind: "thread"; root: Message; replies: Message[] }
  | { kind: "search"; query: string; results: Message[]; loading: boolean }
  | null;

class Chat {
  theme = $state<Theme>("mattermost");
  notifications = $state(true);
  googleConfigured = $state(false);
  googleClientId = $state("");
  accounts = $state<AccountInfo[]>([]);
  /** Konto-ID → Spaces */
  spaces = $state<Record<string, Space[]>>({});
  account = $state<string | null>(null);
  space = $state<string | null>(null);
  messages = $state<Message[]>([]);
  cursor = $state<string | null>(null);
  /** Erste Seite eines Space lädt. */
  loadingInitial = $state(false);
  /** Ältere Nachrichten laden. */
  loadingOlder = $state(false);
  rhs = $state<Rhs>(null);
  toasts = $state<Toast[]>([]);
  /** true im abgelösten Chatfenster */
  popout = $state(false);
  /** Namen eigener Server-Emojis je Konto. */
  customEmojis = $state.raw<Record<string, ReadonlySet<string>>>({});
  private toastSeq = 0;
  private countedReplies = new Set<string>();
  /** Zählt Ladevorgänge, die den Verlauf ersetzen; ältere Antworten werden verworfen. */
  private loadSeq = 0;

  get currentSpace(): Space | undefined {
    return this.account ? this.spaces[this.account]?.find((s) => s.id === this.space) : undefined;
  }

  /** Bild-URL für ":name:", wenn es ein eigenes Server-Emoji des Kontos ist. */
  emojiUrl(account: string | null, name: string): string | null {
    return account && this.customEmojis[account]?.has(name) ? emojiSrc(account, name) : null;
  }

  get currentAccount(): AccountInfo | undefined {
    return this.accounts.find((a) => a.id === this.account);
  }

  /** `only`: abgelöstes Fenster lädt nur die Spaces seines Kontos. */
  async init(only?: string) {
    await this.refreshInfo();
    await listen<ChatEvent>("chat-event", (e) => this.onEvent(e.payload));
    for (const a of this.accounts) if (!only || a.id === only) await this.loadSpaces(a.id);
  }

  async refreshInfo() {
    const info = await api.appInfo();
    this.theme = info.theme;
    this.notifications = info.notifications;
    this.googleConfigured = info.google_configured;
    this.googleClientId = info.google_client_id;
    this.accounts = info.accounts;
    if (!this.account && this.accounts.length) this.account = this.accounts[0].id;
  }

  async saveSettings(theme: Theme, notifications: boolean) {
    this.theme = theme;
    this.notifications = notifications;
    await this.guard(api.setSettings(theme, notifications));
  }

  /** Erst Cache zeigen, dann vom Server aktualisieren. */
  async loadSpaces(account: string) {
    const cached = await api.cachedSpaces(account).catch(() => []);
    if (cached.length && !this.spaces[account]) this.spaces[account] = sortSpaces(cached);
    const online = this.accounts.find((a) => a.id === account)?.online;
    if (!online) return;
    if (!this.customEmojis[account]) {
      api
        .customEmojis(account)
        .then((names) => (this.customEmojis = { ...this.customEmojis, [account]: new Set(names) }))
        .catch(() => {});
    }
    try {
      this.spaces[account] = sortSpaces(await api.listSpaces(account));
    } catch (e) {
      this.toast(`Spaces laden: ${e}`, "error");
    }
  }

  /** Favorit / Stumm umschalten (sofort in der Liste, Server bzw. lokal im Hintergrund). */
  async setFlag(account: string, id: string, flag: { favorite?: boolean; muted?: boolean }) {
    const s = this.spaces[account]?.find((s) => s.id === id);
    if (!s) return;
    const before = { favorite: s.favorite, muted: s.muted };
    Object.assign(s, flag);
    try {
      await api.setSpaceFlag(account, id, flag);
    } catch (e) {
      Object.assign(s, before);
      this.toast(String(e), "error");
    }
  }

  /** Kanal beitreten, in die Seitenleiste aufnehmen und öffnen. */
  async joinSpace(account: string, id: string) {
    const space = await this.guard(api.joinSpace(account, id));
    if (!space) return;
    this.spaces[account] = sortSpaces([...(this.spaces[account] ?? []).filter((s) => s.id !== id), space]);
    await this.openSpace(account, id);
  }

  async selectAccount(account: string) {
    if (this.account && this.account !== account) api.setActive(this.account, null).catch(() => {});
    this.loadSeq++;
    this.loadingInitial = false;
    this.loadingOlder = false;
    this.account = account;
    this.space = null;
    this.messages = [];
    this.rhs = null;
    if (!this.spaces[account]) await this.loadSpaces(account);
  }

  async openSpace(account: string, space: string) {
    if (this.account && this.account !== account) await api.setActive(this.account, null).catch(() => {});
    const seq = ++this.loadSeq;
    this.account = account;
    this.space = space;
    this.rhs = this.rhs?.kind === "search" ? this.rhs : null;
    this.cursor = null;
    this.messages = [];
    this.loadingOlder = false;
    this.loadingInitial = true;
    try {
      const cached = await api.cachedMessages(account, space).catch(() => []);
      if (seq !== this.loadSeq) return;
      this.messages = cached;
      api.setActive(account, space).catch(() => {});
      const page = await api.listMessages(account, space);
      if (seq !== this.loadSeq) return;
      this.messages = page.messages;
      this.cursor = page.next_cursor;
      this.markRead();
    } catch (e) {
      if (seq === this.loadSeq) this.toast(`Verlauf laden: ${e}`, "error");
    } finally {
      if (seq === this.loadSeq) this.loadingInitial = false;
    }
  }

  /** Nach (Neu-)Verbindung: im Abriss verpasste Nachrichten des offenen Space holen. */
  private async catchUp() {
    const { account, space } = this;
    // Während des Erstladens unnötig: die Seite ist ohnehin frisch.
    if (!account || !space || this.loadingInitial) return;
    const seq = this.loadSeq;
    try {
      const page = await api.listMessages(account, space);
      if (seq !== this.loadSeq) return;
      const known = new Set(this.messages.map((m) => m.id));
      if (this.messages.length && !page.messages.some((m) => known.has(m.id))) {
        // Lücke zum Bestand: Verlauf ersetzen statt mischen, laufendes „ältere laden“ verwerfen.
        this.loadSeq++;
        this.loadingOlder = false;
        this.messages = page.messages;
        this.cursor = page.next_cursor;
        return;
      }
      for (const m of page.messages) this.upsert(m);
      this.messages.sort((a, b) => a.created - b.created);
    } catch {
      // nächste Verbindung versucht es erneut
    }
  }

  async loadOlder() {
    const { account, space, cursor } = this;
    if (!account || !space || !cursor || this.loadingOlder || this.loadingInitial) return;
    const seq = this.loadSeq;
    this.loadingOlder = true;
    try {
      const page = await api.listMessages(account, space, cursor);
      if (seq !== this.loadSeq) return;
      const known = new Set(this.messages.map((m) => m.id));
      this.messages = [...page.messages.filter((m) => !known.has(m.id)), ...this.messages];
      this.cursor = page.next_cursor;
    } catch (e) {
      if (seq === this.loadSeq) this.toast(`Ältere Nachrichten: ${e}`, "error");
    } finally {
      if (seq === this.loadSeq) this.loadingOlder = false;
    }
  }

  markRead() {
    const s = this.currentSpace;
    if (!this.account || !s) return;
    if (s.unread || s.mention_count) {
      s.unread = false;
      s.mention_count = 0;
      api.markRead(this.account, s.id).catch(() => {});
    }
  }

  async send(text: string, files: string[], thread: string | null = null) {
    if (!this.account || !this.space) return;
    const msg = await this.guard(api.send(this.account, this.space, text, thread, files));
    if (!msg) return;
    this.upsert(msg, true);
    if (thread && this.rhs?.kind === "thread") this.upsertReply(msg);
  }

  async edit(m: Message, text: string) {
    if (!this.account) return;
    const msg = await this.guard(api.edit(this.account, m.space_id, m.id, text));
    if (msg) this.upsert(msg);
  }

  async remove(m: Message) {
    if (!this.account) return;
    await this.guard(api.remove(this.account, m.space_id, m.id));
    this.drop(m.id);
  }

  async toggleReaction(m: Message, emoji: string) {
    if (!this.account) return;
    const r = m.reactions.find((r) => r.emoji === emoji);
    const add = !r?.me;
    // optimistisch anzeigen, Server-Ereignis korrigiert
    if (add) {
      if (r) {
        r.count++;
        r.me = true;
      } else m.reactions.push({ emoji, count: 1, me: true });
    } else if (r) {
      r.count--;
      r.me = false;
      if (r.count <= 0) m.reactions = m.reactions.filter((x) => x !== r);
    }
    await this.guard(api.react(this.account, m.space_id, m.id, emoji, add));
  }

  async openThread(root: Message) {
    if (!this.account || !root.thread_id) return;
    this.rhs = { kind: "thread", root, replies: [] };
    const all = await this.guard(api.listThread(this.account, root.space_id, root.thread_id));
    if (all && this.rhs?.kind === "thread" && this.rhs.root.id === root.id) {
      this.rhs.replies = all.filter((m) => m.id !== root.id);
    }
  }

  async search(query: string) {
    if (!this.account || !query.trim()) return;
    this.rhs = { kind: "search", query, results: [], loading: true };
    const results = await this.guard(api.search(this.account, query));
    if (this.rhs?.kind === "search" && this.rhs.query === query) {
      this.rhs.results = results ?? [];
      this.rhs.loading = false;
    }
  }

  toast(text: string, kind: Toast["kind"] = "info", action?: Toast["action"]) {
    const id = ++this.toastSeq;
    this.toasts.push({ id, text, kind, action });
    setTimeout(() => this.closeToast(id), kind === "error" ? 8000 : 5000);
  }

  closeToast(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  /** Fehler als Toast statt Exception. */
  async guard<T>(p: Promise<T>): Promise<T | undefined> {
    try {
      return await p;
    } catch (e) {
      this.toast(String(e), "error");
      return undefined;
    }
  }

  private upsert(msg: Message, created = false) {
    if (msg.space_id !== this.space) return;
    if (msg.is_reply) {
      // Antwortzähler der Startnachricht erhöhen
      // (jede Antwort nur einmal zählen: kommt nach dem Senden oft noch per Ereignis)
      if (!created || this.countedReplies.has(msg.id)) return;
      this.countedReplies.add(msg.id);
      const root = this.messages.find((m) => m.thread_id && m.thread_id === msg.thread_id && !m.is_reply);
      if (root) root.reply_count++;
      return;
    }
    const i = this.messages.findIndex((m) => m.id === msg.id);
    if (i >= 0) this.messages[i] = msg;
    else this.messages.push(msg);
  }

  private upsertReply(msg: Message) {
    if (this.rhs?.kind !== "thread" || msg.thread_id !== this.rhs.root.thread_id) return;
    if (msg.id === this.rhs.root.id) {
      this.rhs.root = msg;
      return;
    }
    const i = this.rhs.replies.findIndex((m) => m.id === msg.id);
    if (i >= 0) this.rhs.replies[i] = msg;
    else this.rhs.replies.push(msg);
  }

  private drop(id: string) {
    this.messages = this.messages.filter((m) => m.id !== id);
    if (this.rhs?.kind === "thread") this.rhs.replies = this.rhs.replies.filter((m) => m.id !== id);
  }

  private onEvent({ account, event }: ChatEvent) {
    const acc = this.accounts.find((a) => a.id === account);
    switch (event.type) {
      case "status":
        if (acc) acc.online = event.online;
        if (event.online) {
          // Nach jedem (Neu-)Verbinden: offenen Space nachholen; die Space-Liste lädt nur das Hauptfenster.
          if (!this.popout) this.loadSpaces(account);
          if (account === this.account) this.catchUp();
        }
        else if (event.detail && !this.popout) this.toast(`${acc?.label ?? "Konto"}: ${event.detail}`, "error");
        break;
      case "space_updated": {
        const list = this.spaces[account] ?? [];
        const i = list.findIndex((s) => s.id === event.space.id);
        const visible = account === this.account && event.space.id === this.space && document.hasFocus();
        const space = visible ? { ...event.space, unread: false, mention_count: 0 } : event.space;
        if (i >= 0) list[i] = space;
        else list.push(space);
        this.spaces[account] = sortSpaces(list);
        break;
      }
      case "message_created":
      case "message_updated": {
        const m = event.message;
        if (account === this.account) {
          this.upsert(m, event.type === "message_created");
          this.upsertReply(m);
        }
        const visible = account === this.account && m.space_id === this.space && document.hasFocus();
        if (event.type === "message_created" && !m.mine) {
          if (visible) this.markRead();
          else this.markUnread(account, m);
        }
        break;
      }
      case "message_deleted":
        if (account === this.account) this.drop(event.message_id);
        break;
    }
  }

  private markUnread(account: string, m: Message) {
    const s = this.spaces[account]?.find((s) => s.id === m.space_id);
    if (!s) return;
    s.unread = true;
    s.last_active = Math.max(s.last_active, m.created);
    if (m.mentions_me || (s.kind !== "channel" && !s.muted)) {
      s.mention_count++;
      if (!this.popout) {
        this.toast(`${m.sender.name}${s.kind === "channel" ? ` in ${s.name}` : ""}: ${m.text.slice(0, 120)}`, "message", {
          label: "Öffnen",
          run: () => this.openSpace(account, s.id),
        });
      }
    }
  }
}

function sortSpaces(list: Space[]): Space[] {
  return [...list].sort((a, b) => a.name.localeCompare(b.name, "de"));
}

export const chat = new Chat();

