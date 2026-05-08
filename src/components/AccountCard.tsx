import { useEffect, useState } from "react";
import {
  Check,
  Loader2,
  LogOut,
  Plug,
  UserCircle2,
} from "lucide-react";
import { api, events } from "@/lib/api";
import type { OAuthStatus } from "@/lib/types";
import { Button } from "@/components/Button";
import { OAuthWizard } from "@/components/OAuthWizard";
import { useToast } from "@/components/ToastProvider";

interface Props {
  status: OAuthStatus;
  onChange: (s: OAuthStatus) => void;
}

function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return "Something went wrong.";
}

export function AccountCard({ status, onChange }: Props) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [channel, setChannel] = useState<string | null>(null);
  const [wizardOpen, setWizardOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    events.onOAuthStatusChanged(onChange).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onChange]);

  // Fetch the channel title once we're connected so we can show
  // "Connected as <channel>" instead of just a green dot.
  useEffect(() => {
    if (!status.connected) {
      setChannel(null);
      return;
    }
    let cancelled = false;
    void api
      .youtubeChannelTitle()
      .then((t) => {
        if (!cancelled) setChannel(t);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [status.connected]);

  const connect = async () => {
    setBusy(true);
    try {
      const s = await api.oauthConnect();
      onChange(s);
      toast.push("Connected to YouTube.", "info");
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    setBusy(true);
    try {
      const s = await api.oauthDisconnect();
      onChange(s);
      toast.push("Disconnected.", "info");
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  if (status.connected) {
    return (
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-sm">
            <Check className="h-4 w-4 shrink-0 text-emerald-400" />
            <span className="truncate text-zinc-300">
              Connected as{" "}
              <span className="font-medium text-zinc-100">
                {channel ?? "your channel"}
              </span>
            </span>
          </div>
          <Button
            variant="ghost"
            size="sm"
            icon={<LogOut className="h-3.5 w-3.5" />}
            onClick={disconnect}
            loading={busy}
          >
            Disconnect
          </Button>
        </div>
        <p className="mt-2 text-[11px] leading-relaxed text-zinc-600">
          Auto-link finds the matching upload after each broadcast. Edit-title
          changes show up immediately on YouTube.
        </p>
      </div>
    );
  }

  if (!status.client_present) {
    if (wizardOpen) {
      return (
        <OAuthWizard
          onConnected={(s) => {
            onChange(s);
            setWizardOpen(false);
          }}
        />
      );
    }
    return (
      <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
        <div className="flex items-start gap-3">
          <UserCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-zinc-500" />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-medium text-zinc-200">
              Connect YouTube{" "}
              <span className="ml-1 rounded bg-zinc-800 px-1.5 py-0.5 text-[10px] font-normal uppercase tracking-wider text-zinc-400">
                advanced
              </span>
            </div>
            <p className="mt-1 text-xs leading-relaxed text-zinc-500">
              Optional. Auto-link new broadcasts and edit video titles in-app.
              5-step guided setup, ~5 minutes. Skip this and the paste-URL flow
              above still works.
            </p>
            <div className="mt-3">
              <Button
                variant="secondary"
                size="sm"
                icon={<Plug className="h-3.5 w-3.5" />}
                onClick={() => setWizardOpen(true)}
              >
                Start setup
              </Button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Client is present, just not connected - most common case after the user
  // has done the one-time Cloud Console setup.
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2 text-sm text-zinc-300">
          <UserCircle2 className="h-4 w-4 shrink-0 text-zinc-500" />
          <span>Connect YouTube to auto-link and edit titles in-app.</span>
        </div>
        <Button
          variant="primary"
          size="sm"
          onClick={connect}
          loading={busy}
          icon={busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : undefined}
        >
          Connect
        </Button>
      </div>
    </div>
  );
}
