// Typen und Aufrufe des Rust-Kerns (src-tauri/src/app.rs, model.rs).
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type BackendKind = "google" | "mattermost";
export type SpaceKind = "channel" | "direct" | "group";

export interface Account {
  id: string;
  kind: BackendKind;
  server?: string | null;
  label: string;
  login?: string | null;
}

export interface AccountInfo extends Account {
  online: boolean;
}

export interface AppInfo {
  theme: Theme;
  notifications: boolean;
  google_configured: boolean;
  google_client_id: string;
  accounts: AccountInfo[];
}

export interface User {
  id: string;
  name: string;
  avatar_url?: string | null;
}

/** Im Dialog gewählte Datei (Pfad bleibt im Backend). */
export interface PickedFile {
  id: number;
  name: string;
}

export type Theme = "mattermost" | "slack" | "ehex";

export interface Space {
  id: string;
  name: string;
  kind: SpaceKind;
  unread: boolean;
  mention_count: number;
  last_active: number;
  msg_count: number;
  favorite: boolean;
  muted: boolean;
  /** Status-Emoji des DM-Partners: Unicode oder ":name:" (eigenes Server-Emoji). */
  status_emoji?: string | null;
}

export interface Reaction {
  emoji: string;
  count: number;
  me: boolean;
}

export interface Attachment {
  id: string;
  name: string;
  mime?: string | null;
  size?: number | null;
  url?: string | null;
}

export interface Message {
  id: string;
  space_id: string;
  thread_id?: string | null;
  is_reply: boolean;
  sender: User;
  text: string;
  created: number;
  edited?: number | null;
  reactions: Reaction[];
  attachments: Attachment[];
  reply_count: number;
  mentions_me: boolean;
  mine: boolean;
}

export interface Page {
  messages: Message[];
  next_cursor: string | null;
}

export type BackendEvent =
  | { type: "message_created"; message: Message }
  | { type: "message_updated"; message: Message }
  | { type: "message_deleted"; space_id: string; message_id: string }
  | { type: "space_updated"; space: Space }
  | { type: "status"; online: boolean; detail: string | null };

export interface ChatEvent {
  account: string;
  event: BackendEvent;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  setSettings: (theme: string, notifications: boolean) =>
    invoke<void>("set_settings", { theme, notifications }),
  setGoogleClient: (clientId: string, clientSecret: string) =>
    invoke<void>("set_google_client", { clientId, clientSecret }),
  addGoogle: (accountId?: string) => invoke<Account>("add_google_account", { accountId }),
  addMattermost: (a: { accountId?: string; server: string; login: string; password: string; mfa?: string }) =>
    invoke<Account>("add_mattermost_account", a),
  removeAccount: (account: string) => invoke<void>("remove_account", { account }),
  cachedSpaces: (account: string) => invoke<Space[]>("cached_spaces", { account }),
  listSpaces: (account: string) => invoke<Space[]>("list_spaces", { account }),
  browseSpaces: (account: string, query: string) => invoke<Space[]>("browse_spaces", { account, query }),
  customEmojis: (account: string) => invoke<string[]>("custom_emojis", { account }),
  joinSpace: (account: string, space: string) => invoke<Space>("join_space", { account, space }),
  setSpaceFlag: (account: string, space: string, flag: { favorite?: boolean; muted?: boolean }) =>
    invoke<void>("set_space_flag", { account, space, favorite: flag.favorite ?? null, muted: flag.muted ?? null }),
  cachedMessages: (account: string, space: string) => invoke<Message[]>("cached_messages", { account, space }),
  listMessages: (account: string, space: string, cursor?: string | null) =>
    invoke<Page>("list_messages", { account, space, cursor }),
  listThread: (account: string, space: string, thread: string) =>
    invoke<Message[]>("list_thread", { account, space, thread }),
  /** Öffnen-Dialog im Backend; liefert IDs statt Pfaden. */
  pickFiles: () => invoke<PickedFile[]>("pick_files"),
  send: (account: string, space: string, text: string, thread: string | null, files: number[]) =>
    invoke<Message>("send_message", { account, space, text, thread, files }),
  edit: (account: string, space: string, id: string, text: string) =>
    invoke<Message>("edit_message", { account, space, id, text }),
  remove: (account: string, space: string, id: string) => invoke<void>("delete_message", { account, space, id }),
  react: (account: string, space: string, id: string, emoji: string, add: boolean) =>
    invoke<void>("react", { account, space, id, emoji, add }),
  /** Speichern-Dialog im Backend; `false` = abgebrochen. */
  saveAttachment: (account: string, attachment: Attachment) =>
    invoke<boolean>("save_attachment", { account, attachment }),
  search: (account: string, query: string) => invoke<Message[]>("search", { account, query }),
  markRead: (account: string, space: string) => invoke<void>("mark_read", { account, space }),
  setActive: (account: string, space: string | null) => invoke<void>("set_active", { account, space }),
  openWindow: (account: string, space: string, title: string) =>
    invoke<void>("open_space_window", { account, space, title }),
};

/** URL des Profilbilds über das Backend-Schema `avatar://` (siehe app.rs `avatar_protocol`). */
export function avatarSrc(account: string, userId: string): string {
  return `${convertFileSrc(account, "avatar")}/${encodeURIComponent(userId)}`;
}

/** URL eines eigenen Server-Emojis über dasselbe Schema. */
export function emojiSrc(account: string, name: string): string {
  return `${convertFileSrc(account, "avatar")}/emoji/${encodeURIComponent(name)}`;
}
