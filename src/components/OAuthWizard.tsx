import { useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Check,
  ExternalLink,
  Loader2,
  Plug,
} from "lucide-react";
import { api } from "@/lib/api";
import type { OAuthStatus } from "@/lib/types";
import { Button } from "@/components/Button";
import { useToast } from "@/components/ToastProvider";

interface Step {
  title: string;
  body: string;
  /// Cloud Console URL the "Open in browser" button jumps to, or null on the
  /// final paste-credentials step.
  url: string | null;
}

const STEPS: Step[] = [
  {
    title: "Create a Cloud project",
    body:
      "Sign in with the same Google account that owns your YouTube channel. Name the project anything (e.g. \"Archeio\") and click Create. Wait a few seconds for it to provision.",
    url: "https://console.cloud.google.com/projectcreate",
  },
  {
    title: "Enable the YouTube Data API",
    body:
      "Make sure the project you just created is selected in the top bar, then click Enable. Done - close that tab.",
    url: "https://console.cloud.google.com/apis/library/youtube.googleapis.com",
  },
  {
    title: "Set up the OAuth overview",
    body:
      "Click Get started. App info → name it \"Archeio\" + your email → Next. Audience → External → Next. Contact information → your email again → Next. Agree → Create.",
    url: "https://console.cloud.google.com/auth/overview",
  },
  {
    title: "Add yourself as a test user",
    body:
      "Required - Testing-mode apps reject any account that isn't on the test-users list. Scroll to \"Test users\" → + Add users → paste your Gmail → Save.",
    url: "https://console.cloud.google.com/auth/audience",
  },
  {
    title: "Create a Desktop OAuth client",
    body:
      "+ Create Clients → OAuth client ID → Application type: Desktop app → Name it \"Archeio Desktop\" → Create. A popup appears with your Client ID and Client Secret.",
    url: "https://console.cloud.google.com/apis/credentials/oauthclient",
  },
  {
    title: "Paste your credentials",
    body:
      "Copy both fields from the popup Google just showed you and paste them below. Archeio stores them locally and connects in one click.",
    url: null,
  },
];

interface Props {
  onConnected: (s: OAuthStatus) => void;
}

function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return "Something went wrong.";
}

export function OAuthWizard({ onConnected }: Props) {
  const toast = useToast();
  const [step, setStep] = useState(0);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [busy, setBusy] = useState(false);

  const total = STEPS.length;
  const current = STEPS[step];
  const isLast = step === total - 1;

  const next = () => setStep((s) => Math.min(total - 1, s + 1));
  const back = () => setStep((s) => Math.max(0, s - 1));

  const saveAndConnect = async () => {
    if (!clientId.trim() || !clientSecret.trim()) {
      toast.push("Paste both Client ID and Client Secret.", "error");
      return;
    }
    setBusy(true);
    try {
      // Persist the client creds first; oauth_connect reads them from disk.
      await api.oauthSaveClient(clientId.trim(), clientSecret.trim());
      const s = await api.oauthConnect();
      onConnected(s);
      toast.push("Connected to YouTube.", "info");
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
      <header className="mb-3 flex items-center justify-between">
        <h3 className="text-sm font-medium text-zinc-200">Connect YouTube</h3>
        <div className="text-[11px] uppercase tracking-wider text-zinc-500">
          Step {step + 1} of {total}
        </div>
      </header>

      <div className="mb-3 flex gap-1">
        {STEPS.map((_, i) => (
          <div
            key={i}
            className={
              "h-1 flex-1 rounded-full transition-colors " +
              (i <= step ? "bg-accent" : "bg-zinc-800")
            }
          />
        ))}
      </div>

      <div className="space-y-3">
        <div>
          <div className="text-sm font-medium text-zinc-100">{current.title}</div>
          <p className="mt-1 text-xs leading-relaxed text-zinc-400">
            {current.body}
          </p>
        </div>

        {current.url && (
          <Button
            variant="secondary"
            size="sm"
            icon={<ExternalLink className="h-3.5 w-3.5" />}
            onClick={() => api.openExternal(current.url!)}
          >
            Open Cloud Console
          </Button>
        )}

        {isLast && (
          <div className="space-y-2">
            <label className="block">
              <span className="mb-1 block text-[11px] uppercase tracking-wider text-zinc-500">
                Client ID
              </span>
              <input
                type="text"
                autoComplete="off"
                spellCheck={false}
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                placeholder="123456789012-abcde…apps.googleusercontent.com"
                className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2 font-mono text-xs text-zinc-100 placeholder-zinc-700 focus:border-zinc-600 focus:outline-none"
              />
            </label>
            <label className="block">
              <span className="mb-1 block text-[11px] uppercase tracking-wider text-zinc-500">
                Client Secret
              </span>
              <input
                type="password"
                autoComplete="off"
                spellCheck={false}
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                placeholder="GOCSPX-…"
                className="w-full rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2 font-mono text-xs text-zinc-100 placeholder-zinc-700 focus:border-zinc-600 focus:outline-none"
                onKeyDown={(e) => {
                  if (e.key === "Enter") void saveAndConnect();
                }}
              />
            </label>
            <p className="text-[11px] leading-relaxed text-zinc-600">
              Stored at <code className="text-zinc-500">%LOCALAPPDATA%\Archeio\oauth_client.json</code>.
              On Connect, your browser opens once more for Google's
              authorisation prompt - click "Advanced → Go to Archeio (unsafe)"
              past the unverified-app warning. That warning only appears
              because you own the OAuth client; it's expected.
            </p>
          </div>
        )}
      </div>

      <footer className="mt-4 flex items-center justify-between border-t border-zinc-800 pt-3">
        <Button
          variant="ghost"
          size="sm"
          onClick={back}
          disabled={step === 0 || busy}
          icon={<ArrowLeft className="h-3.5 w-3.5" />}
        >
          Back
        </Button>
        {isLast ? (
          <Button
            variant="primary"
            size="sm"
            onClick={saveAndConnect}
            loading={busy}
            disabled={!clientId.trim() || !clientSecret.trim()}
            icon={
              busy ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Plug className="h-3.5 w-3.5" />
              )
            }
          >
            Save & Connect
          </Button>
        ) : (
          <Button
            variant="primary"
            size="sm"
            onClick={next}
            icon={
              step === total - 2 ? (
                <Check className="h-3.5 w-3.5" />
              ) : (
                <ArrowRight className="h-3.5 w-3.5" />
              )
            }
          >
            {step === total - 2 ? "Got the credentials" : "Next"}
          </Button>
        )}
      </footer>
    </div>
  );
}
