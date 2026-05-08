import { useEffect, useMemo, useState } from "react";
import {
  Check,
  ExternalLink,
  Eye,
  EyeOff,
  Film,
  LayoutGrid,
  List,
  Pencil,
  Play,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { api, embedUrl, events, studioEditUrl } from "@/lib/api";
import type { Broadcast } from "@/lib/types";
import { cx, formatDuration, formatWhen } from "@/lib/utils";
import { useToast } from "@/components/ToastProvider";

function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return "Something went wrong.";
}

export function Library({
  gridView,
  setGridView,
}: {
  gridView: boolean;
  setGridView: (v: boolean) => void;
}) {
  const toast = useToast();
  const [items, setItems] = useState<Broadcast[] | null>(null);
  const [query, setQuery] = useState("");
  const [showAll, setShowAll] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      try {
        setItems(await api.libraryList());
      } catch (e) {
        toast.push(errMsg(e), "error");
      }
    };
    void refresh();
    events.onLibraryChanged(refresh).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [toast]);

  const filtered = useMemo(() => {
    if (items === null) return [];
    const q = query.trim().toLowerCase();
    if (!q) return items;
    return items.filter((r) => (r.title ?? "").toLowerCase().includes(q));
  }, [items, query]);

  if (items === null) return null;

  return (
    <section className="space-y-3">
      <header className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-600" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search broadcasts…"
            className="w-full rounded-md border border-zinc-800 bg-zinc-900/40 py-1.5 pl-7 pr-2 text-xs text-zinc-100 placeholder-zinc-600 focus:border-zinc-700 focus:outline-none"
          />
        </div>
        <h2 className="flex shrink-0 items-center gap-1.5 text-xs font-medium uppercase tracking-wider text-zinc-500">
          <Film className="h-3.5 w-3.5" />
          Library
        </h2>
        <button
          onClick={() => setShowAll((v) => !v)}
          className={cx(
            "shrink-0 rounded p-1 transition-colors",
            showAll
              ? "bg-zinc-800 text-zinc-200"
              : "text-zinc-600 hover:text-zinc-300",
          )}
          title={
            showAll
              ? "Hide all embeds by default"
              : "Show all embeds by default"
          }
          aria-label="Toggle default embed visibility"
          aria-pressed={showAll}
        >
          {showAll ? (
            <Eye className="h-3.5 w-3.5" />
          ) : (
            <EyeOff className="h-3.5 w-3.5" />
          )}
        </button>
        <button
          onClick={() => setGridView(!gridView)}
          className={cx(
            "shrink-0 rounded p-1 transition-colors",
            gridView
              ? "bg-zinc-800 text-zinc-200"
              : "text-zinc-600 hover:text-zinc-300",
          )}
          title={gridView ? "Switch to list view" : "Switch to grid view"}
          aria-label="Toggle list/grid layout"
          aria-pressed={gridView}
        >
          {gridView ? (
            <LayoutGrid className="h-3.5 w-3.5" />
          ) : (
            <List className="h-3.5 w-3.5" />
          )}
        </button>
      </header>

      {items.length === 0 ? (
        <EmptyState message="No broadcasts yet." hint="Press your hotkey to start one." />
      ) : filtered.length === 0 ? (
        <EmptyState
          message="No matches."
          hint={`Nothing in the library matches “${query.trim()}”.`}
        />
      ) : (
        <ul
          className={cx(
            gridView
              ? "grid grid-cols-2 gap-2 md:grid-cols-3 xl:grid-cols-4"
              : "space-y-2",
          )}
        >
          {filtered.map((row) => (
            <LibraryRow
              key={row.id}
              row={row}
              defaultExpanded={showAll}
              compact={gridView}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function EmptyState({ message, hint }: { message: string; hint: string }) {
  return (
    <div className="rounded-lg border border-dashed border-zinc-800 bg-zinc-900/20 p-6 text-center">
      <p className="text-sm text-zinc-400">{message}</p>
      <p className="mt-1 text-xs text-zinc-600">{hint}</p>
    </div>
  );
}

function LibraryRow({
  row,
  defaultExpanded,
  compact,
}: {
  row: Broadcast;
  defaultExpanded: boolean;
  compact: boolean;
}) {
  const toast = useToast();
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleValue, setTitleValue] = useState(row.title ?? "");
  const [busy, setBusy] = useState(false);

  // Re-sync per-row expansion when the global toggle flips. Users can still
  // override individually after; this only fires on global change.
  useEffect(() => {
    setExpanded(defaultExpanded);
  }, [defaultExpanded]);

  const stillRunning = row.ended_at === null;

  const saveTitle = async () => {
    const trimmed = titleValue.trim();
    if (!trimmed) {
      toast.push("Title cannot be empty.", "error");
      return;
    }
    setBusy(true);
    try {
      await api.youtubeUpdateTitle(row.id, trimmed);
      toast.push("Title updated on YouTube.", "info");
      setEditingTitle(false);
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await api.libraryRemove(row.id);
    } catch (e) {
      toast.push(errMsg(e), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <li
      className={cx(
        "rounded-lg border border-zinc-800 bg-zinc-900/40",
        compact ? "p-2" : "p-3",
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <div
            className={cx(
              "flex items-center gap-1.5 text-zinc-500",
              compact ? "text-[10px]" : "text-xs gap-2",
            )}
          >
            <span className="truncate">{formatWhen(row.started_at)}</span>
            <span>·</span>
            {stillRunning ? (
              <span className="text-accent">live</span>
            ) : (
              <span className="font-mono tabular-nums">
                {formatDuration(row.started_at, row.ended_at!)}
              </span>
            )}
          </div>

          <TitleRow
            title={row.title ?? "Untitled broadcast"}
            editing={editingTitle}
            value={titleValue}
            onChange={setTitleValue}
            onSave={saveTitle}
            onEdit={() => {
              setTitleValue(row.title ?? "");
              setEditingTitle(true);
            }}
            onCancel={() => setEditingTitle(false)}
            busy={busy}
            canEdit={!!row.youtube_video_id}
          />
        </div>

        <button
          onClick={remove}
          disabled={busy || stillRunning}
          className="shrink-0 text-zinc-600 hover:text-zinc-300 disabled:opacity-40"
          aria-label="Remove from library"
          title={stillRunning ? "Stop broadcast first" : "Remove"}
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>

      {row.youtube_video_id && (
        <div className="mt-3 space-y-2">
          {expanded ? (
            <div className="aspect-video w-full overflow-hidden rounded-md bg-black">
              <iframe
                src={embedUrl(row.youtube_video_id)}
                title={row.title ?? "Broadcast"}
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                allowFullScreen
                className="h-full w-full"
              />
            </div>
          ) : (
            <button
              onClick={() => setExpanded(true)}
              className="flex w-full items-center justify-center gap-2 rounded-md border border-zinc-800 bg-zinc-950 py-3 text-xs text-zinc-400 hover:border-zinc-700 hover:text-zinc-200"
            >
              {stillRunning ? "Watch live" : "Play video"}
            </button>
          )}
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <button
              onClick={() =>
                api.openExternal(
                  `https://www.youtube.com/watch?v=${row.youtube_video_id}`,
                )
              }
              className="inline-flex items-center gap-1.5 rounded-md bg-accent/10 px-2 py-1 font-medium text-accent ring-1 ring-accent/20 transition-colors hover:bg-accent/20"
              title="Open on YouTube - embed above is a low-res preview"
            >
              <Play className="h-3.5 w-3.5 fill-current" />
              {compact ? "YT" : "Open full Quality"}
            </button>
            {!compact && (
              <button
                onClick={() => api.openExternal(studioEditUrl(row.youtube_video_id!))}
                className="inline-flex items-center gap-1.5 px-1 py-1 text-zinc-400 hover:text-zinc-200"
              >
                <ExternalLink className="h-3 w-3" />
                Edit
              </button>
            )}
          </div>
        </div>
      )}
    </li>
  );
}

function TitleRow({
  title,
  editing,
  value,
  onChange,
  onSave,
  onEdit,
  onCancel,
  busy,
  canEdit,
}: {
  title: string;
  editing: boolean;
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
  onEdit: () => void;
  onCancel: () => void;
  busy: boolean;
  canEdit: boolean;
}) {
  if (editing) {
    return (
      <div className="mt-1 flex items-center gap-2">
        <input
          value={value}
          onChange={(e) => onChange(e.target.value)}
          autoFocus
          maxLength={100}
          className="flex-1 rounded-md border border-zinc-800 bg-zinc-950 px-2 py-1 text-sm text-zinc-100 focus:border-zinc-600 focus:outline-none"
          onKeyDown={(e) => {
            if (e.key === "Enter") onSave();
            if (e.key === "Escape") onCancel();
          }}
        />
        <button
          onClick={onSave}
          disabled={busy}
          className="text-zinc-400 hover:text-zinc-100 disabled:opacity-40"
          aria-label="Save"
        >
          <Check className="h-4 w-4" />
        </button>
        <button
          onClick={onCancel}
          disabled={busy}
          className="text-zinc-500 hover:text-zinc-300 disabled:opacity-40"
          aria-label="Cancel"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    );
  }
  return (
    <div className="mt-1 flex items-center gap-2">
      <p
        className={cx(
          "min-w-0 flex-1 truncate text-sm",
          title === "Untitled broadcast" ? "italic text-zinc-500" : "text-zinc-100",
        )}
      >
        {title}
      </p>
      {canEdit && (
        <button
          onClick={onEdit}
          className="shrink-0 text-zinc-600 hover:text-zinc-300"
          title="Edit title on YouTube"
          aria-label="Edit title"
        >
          <Pencil className="h-3 w-3" />
        </button>
      )}
    </div>
  );
}
