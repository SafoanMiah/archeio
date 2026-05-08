import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Broadcast, LiveState, OAuthStatus } from "./types";

export const api = {
  startBroadcast: (title?: string) =>
    invoke<LiveState>("start_broadcast", { title }),
  stopBroadcast: () => invoke<void>("stop_broadcast"),
  liveState: () => invoke<LiveState>("live_state"),

  checkFfmpeg: () => invoke<string>("check_ffmpeg"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),

  retryHotkey: () => invoke<boolean>("retry_hotkey"),
  hotkeyLabel: () => invoke<string>("hotkey_label"),
  hotkeyStatus: () => invoke<boolean>("hotkey_status"),

  libraryList: () => invoke<Broadcast[]>("library_list"),
  libraryRemove: (id: string) => invoke<void>("library_remove", { id }),

  oauthStatus: () => invoke<OAuthStatus>("oauth_status"),
  oauthConnect: () => invoke<OAuthStatus>("oauth_connect"),
  oauthDisconnect: () => invoke<OAuthStatus>("oauth_disconnect"),
  oauthSaveClient: (clientId: string, clientSecret: string) =>
    invoke<OAuthStatus>("oauth_save_client", { clientId, clientSecret }),

  youtubeChannelTitle: () => invoke<string>("youtube_channel_title"),
  youtubeUpdateTitle: (id: string, newTitle: string) =>
    invoke<Broadcast>("youtube_update_title", { id, newTitle }),
  youtubeUpdatePrivacy: (id: string, newPrivacy: "private" | "unlisted" | "public") =>
    invoke<Broadcast>("youtube_update_privacy", { id, newPrivacy }),
};

export const events = {
  onLiveStateChanged: (cb: (s: LiveState) => void): Promise<UnlistenFn> =>
    listen<LiveState>("live-state-changed", (e) => cb(e.payload)),
  onErrorToast: (cb: (msg: string) => void): Promise<UnlistenFn> =>
    listen<string>("error-toast", (e) => cb(e.payload)),
  onHotkeyStatus: (cb: (ok: boolean) => void): Promise<UnlistenFn> =>
    listen<boolean>("hotkey-status", (e) => cb(e.payload)),
  onLibraryChanged: (cb: () => void): Promise<UnlistenFn> =>
    listen<null>("library-changed", () => cb()),
  onOAuthStatusChanged: (cb: (s: OAuthStatus) => void): Promise<UnlistenFn> =>
    listen<OAuthStatus>("oauth-status-changed", (e) => cb(e.payload)),
};

export function studioEditUrl(videoId: string): string {
  return `https://studio.youtube.com/video/${videoId}/edit`;
}

export function embedUrl(videoId: string): string {
  // Params hide most YouTube chrome that doesn't scale with iframe size:
  // modestbranding=1 trims the YouTube logo, rel=0 suppresses the "more videos"
  // wall on pause, iv_load_policy=3 hides annotations, playsinline=1 keeps
  // playback in-frame on mobile webview.
  return `https://www.youtube.com/embed/${videoId}?modestbranding=1&rel=0&iv_load_policy=3&playsinline=1`;
}
