import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Loader2,
  RefreshCw,
  Square,
  Video,
} from "lucide-react";
import { api, events } from "@/lib/api";
import type { LiveState, OAuthStatus } from "@/lib/types";
import { Button } from "@/components/Button";
import { ToastProvider, useToast } from "@/components/ToastProvider";
import { Library } from "@/components/Library";
import { AccountCard } from "@/components/AccountCard";
import { OAuthWizard } from "@/components/OAuthWizard";
import { cx, formatElapsed } from "@/lib/utils";

const isOverlayWindow = new URLSearchParams(window.location.search).has(
  "overlay",
);

function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return "Something went wrong.";
}

export default function App() {
  if (isOverlayWindow) {
    return <Overlay />;
  }
  return (
    <ToastProvider>
      <Shell />
    </ToastProvider>
  );
}

const EMPTY_LIVE: LiveState = {
  is_running: false,
  started_at: null,
};

function Overlay() {
  const [live, setLive] = useState<LiveState>(EMPTY_LIVE);
  const [, tick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void api
      .liveState()
      .then((s) => {
        if (!cancelled) setLive(s);
      })
      .catch(() => undefined);
    events.onLiveStateChanged(setLive).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!live.is_running) return;
    const id = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [live.is_running]);

  if (!live.is_running) return null;

  const elapsed = live.started_at ? formatElapsed(live.started_at) : "00:00:00";
  return (
    <div className="flex h-screen w-screen items-center justify-center gap-2 bg-black/85 px-3">
      <span className="h-2 w-2 rounded-full bg-accent" />
      <span className="text-[10px] font-bold uppercase tracking-widest text-white">
        REC
      </span>
      <span className="font-mono text-[11px] tabular-nums text-zinc-200">
        {elapsed}
      </span>
    </div>
  );
}

function Shell() {
  const toast = useToast();
  const [live, setLive] = useState<LiveState>(EMPTY_LIVE);
  const [busy, setBusy] = useState(false);
  const [hotkeyOk, setHotkeyOk] = useState<boolean | null>(null);
  const [hotkeyLabel, setHotkeyLabel] = useState<string>("Alt+X");
  const [retryingHotkey, setRetryingHotkey] = useState(false);
  const [oauth, setOauth] = useState<OAuthStatus | null>(null);
  const [streamTitle, setStreamTitle] = useState("");
  const [gridView, setGridView] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void api
      .oauthStatus()
      .then((s) => {
        if (!cancelled) setOauth(s);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void api
      .hotkeyLabel()
      .then((l) => {
        if (!cancelled) setHotkeyLabel(l);
      })
      .catch(() => undefined);
    void api
      .hotkeyStatus()
      .then((ok) => {
        if (!cancelled) setHotkeyOk(ok);
      })
      .catch(() => undefined);
    events.onHotkeyStatus(setHotkeyOk).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const retryHotkey = async () => {
    setRetryingHotkey(true);
    try {
      const ok = await api.retryHotkey();
      if (ok) toast.push("Hotkey is now active.", "info");
      else toast.push("Hotkey is still in use by another app.", "error");
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setRetryingHotkey(false);
    }
  };

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void api
      .liveState()
      .then((s) => {
        if (!cancelled) setLive(s);
      })
      .catch(() => undefined);
    events.onLiveStateChanged(setLive).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const start = async () => {
    if (busy || live.is_running) return;
    setBusy(true);
    try {
      const trimmed = streamTitle.trim();
      const s = await api.startBroadcast(trimmed || undefined);
      setLive(s);
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    if (busy || !live.is_running) return;
    setBusy(true);
    try {
      await api.stopBroadcast();
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const isLive = live.is_running;

  return (
    <div className="flex h-full flex-col bg-zinc-950 text-zinc-100">
      <header className="flex items-center justify-between px-6 py-4">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-medium text-zinc-300">
            <span className="h-2 w-2 rounded-full bg-accent" />
            Archeio
          </div>
          <span
            className={cx(
              "mt-0.5 text-[12px] uppercase tracking-wider transition-colors",
              isLive ? "text-accent" : "text-zinc-500",
            )}
          >
            {isLive ? "Live" : "Idle"}
          </span>
        </div>
        <div className="text-[11px] uppercase tracking-wider text-zinc-600">
          v0.1
        </div>
      </header>

      <main className="flex-1 overflow-y-auto px-6 pb-6">
        <div className="space-y-8 pt-6">
          {oauth === null ? (
            <Loading />
          ) : !oauth.connected ? (
            <Onboarding onConnected={setOauth} />
          ) : (
            <ConnectedView
              live={live}
              busy={busy}
              hotkeyOk={hotkeyOk}
              hotkeyLabel={hotkeyLabel}
              retryingHotkey={retryingHotkey}
              start={start}
              stop={stop}
              retryHotkey={retryHotkey}
              oauth={oauth}
              setOauth={setOauth}
              streamTitle={streamTitle}
              setStreamTitle={setStreamTitle}
              gridView={gridView}
              setGridView={setGridView}
            />
          )}
        </div>
      </main>

      <footer className="shrink-0 px-6 py-4 text-center text-[11px] text-zinc-600">
        Recordings streamed to YouTube to act as your unlimited storage, 12h per recording.
      </footer>
    </div>
  );
}

function Loading() {
  return (
    <div className="flex justify-center pt-12">
      <Loader2 className="h-5 w-5 animate-spin text-zinc-600" />
    </div>
  );
}

function Onboarding({ onConnected }: { onConnected: (s: OAuthStatus) => void }) {
  return (
    <div className="mx-auto w-full max-w-md space-y-6 pt-2">
      <div className="space-y-2 text-center">
        <h1 className="text-2xl font-semibold text-zinc-100">
          Connect YouTube to start
        </h1>
        <p className="mx-auto max-w-sm text-sm leading-relaxed text-zinc-400">
          Archeio uses your YouTube channel as the storage backend. Press the
          hotkey, your screen streams as a private broadcast that auto-archives
          to a video on your channel. One-time setup, ~5 minutes.
        </p>
      </div>
      <OAuthWizard onConnected={onConnected} />
    </div>
  );
}

function ConnectedView({
  live,
  busy,
  hotkeyOk,
  hotkeyLabel,
  retryingHotkey,
  start,
  stop,
  retryHotkey,
  oauth,
  setOauth,
  streamTitle,
  setStreamTitle,
  gridView,
  setGridView,
}: {
  live: LiveState;
  busy: boolean;
  hotkeyOk: boolean | null;
  hotkeyLabel: string;
  retryingHotkey: boolean;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  retryHotkey: () => Promise<void>;
  oauth: OAuthStatus;
  setOauth: (s: OAuthStatus) => void;
  streamTitle: string;
  setStreamTitle: (v: string) => void;
  gridView: boolean;
  setGridView: (v: boolean) => void;
}) {
  return (
    <>
      {/* Focal column (status, button, title, hotkey warning) stays narrow
          and centered regardless of window width. Extra top padding pushes
          the timer toward visual centre - black space above is intentional. */}
      <div className="mx-auto w-full max-w-md space-y-8 pt-16">
        <StatusDisplay live={live} />

        <div className="flex flex-col items-center">
          {live.is_running ? (
            <StopButton onClick={stop} busy={busy} />
          ) : (
            <div className="flex flex-col items-center gap-2">
              <StartButton onClick={start} busy={busy} />
              <input
                type="text"
                value={streamTitle}
                onChange={(e) => setStreamTitle(e.target.value)}
                placeholder="Stream title (optional)"
                maxLength={100}
                className="w-56 rounded-md border border-zinc-800 bg-zinc-900/40 px-3 py-1.5 text-center text-xs text-zinc-100 placeholder-zinc-600 focus:border-zinc-700 focus:outline-none"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void start();
                }}
              />
            </div>
          )}
          <p className="mt-3 text-xs text-zinc-500">
            or press{" "}
            <kbd className="rounded border border-zinc-800 bg-zinc-900 px-1.5 py-0.5 font-mono text-[11px] text-zinc-300">
              {hotkeyLabel}
            </kbd>{" "}
            anywhere
          </p>
        </div>

        {hotkeyOk === false && (
          <HotkeyWarning
            hotkey={hotkeyLabel}
            onRetry={retryHotkey}
            busy={retryingHotkey}
          />
        )}
      </div>

      {/* Library expands to the full window width when in grid view so the
          embed thumbnails have room to breathe; list mode stays narrow. */}
      <div
        className={cx(
          "mx-auto w-full",
          gridView ? "max-w-screen-2xl" : "max-w-md",
        )}
      >
        <Library gridView={gridView} setGridView={setGridView} />
      </div>

      <div className="mx-auto w-full max-w-md">
        <AccountCard status={oauth} onChange={setOauth} />
      </div>
    </>
  );
}

function StatusDisplay({ live }: { live: LiveState }) {
  const [, tick] = useState(0);
  useEffect(() => {
    if (!live.is_running || !live.started_at) return;
    const id = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [live.is_running, live.started_at]);

  const isLive = live.is_running;
  const elapsed = live.started_at ? formatElapsed(live.started_at) : "00:00:00";

  return (
    <div className="flex flex-col items-center">
      <div
        className={cx(
          "font-mono text-5xl font-light tabular-nums tracking-tight transition-colors",
          isLive ? "text-zinc-100" : "text-zinc-700",
        )}
      >
        {elapsed}
      </div>
    </div>
  );
}

function StartButton({ onClick, busy }: { onClick: () => void; busy: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={cx(
        "group flex h-14 w-56 items-center justify-center gap-3 rounded-lg text-base font-semibold transition-all",
        "focus:outline-none focus-visible:ring-2 focus-visible:ring-accent/50",
        busy
          ? "cursor-wait bg-accent/70 text-white"
          : "bg-accent text-white shadow-lg shadow-accent/20 hover:bg-accent-hover active:scale-[0.98]",
      )}
    >
      {busy ? (
        <Loader2 className="h-5 w-5 animate-spin" />
      ) : (
        <Video className="h-5 w-5" />
      )}
      Start recording
    </button>
  );
}

function StopButton({ onClick, busy }: { onClick: () => void; busy: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={busy}
      className={cx(
        "group flex h-14 w-56 items-center justify-center gap-3 rounded-lg bg-zinc-900 text-base font-semibold text-zinc-100 ring-1 ring-zinc-800 transition-all",
        "hover:bg-zinc-800 hover:ring-zinc-700 active:scale-[0.98]",
        "focus:outline-none focus-visible:ring-2 focus-visible:ring-zinc-600",
        busy && "cursor-wait opacity-60",
      )}
    >
      {busy ? (
        <Loader2 className="h-5 w-5 animate-spin" />
      ) : (
        <Square className="h-4 w-4 fill-accent text-accent" />
      )}
      Stop stream
    </button>
  );
}

function HotkeyWarning({
  hotkey,
  onRetry,
  busy,
}: {
  hotkey: string;
  onRetry: () => void;
  busy: boolean;
}) {
  return (
    <div className="rounded-lg border border-yellow-900/40 bg-yellow-950/30 p-4">
      <div className="flex items-start gap-3">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-yellow-400" />
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-yellow-100">
            Hotkey unavailable
          </div>
          <p className="mt-1 text-xs leading-relaxed text-yellow-200/70">
            <span className="font-mono">{hotkey}</span> is being held by another
            process. Retry will force-kill any leftover Archeio instances and
            try again. If a different app (Discord, NVIDIA Overlay, OBS) is
            holding it, close that app first.
          </p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onRetry}
          loading={busy}
          icon={<RefreshCw className="h-3.5 w-3.5" />}
        >
          Retry
        </Button>
      </div>
    </div>
  );
}
