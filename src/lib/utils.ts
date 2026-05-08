import clsx, { type ClassValue } from "clsx";

export function cx(...args: ClassValue[]): string {
  return clsx(...args);
}

export function formatElapsed(startedAt: string): string {
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) return "00:00:00";
  const diff = Math.max(0, Math.floor((Date.now() - start) / 1000));
  return formatHMS(diff);
}

export function formatDuration(startedAt: string, endedAt: string): string {
  const a = new Date(startedAt).getTime();
  const b = new Date(endedAt).getTime();
  if (Number.isNaN(a) || Number.isNaN(b)) return "00:00:00";
  return formatHMS(Math.max(0, Math.floor((b - a) / 1000)));
}

function formatHMS(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}

/// Friendly absolute timestamp: "Today 14:32" or "Mar 5, 14:32".
export function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const now = new Date();
  const same =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (same) return `Today ${time}`;
  const md = d.toLocaleDateString([], { month: "short", day: "numeric" });
  return `${md}, ${time}`;
}
