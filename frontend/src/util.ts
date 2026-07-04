// Small formatting + presentation helpers shared across components.

export function fmtTime(ts: number | null | undefined): string {
  if (ts == null) return "—";
  const d = new Date(ts * 1000);
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function fmtDate(ts: number | null | undefined): string {
  if (ts == null) return "—";
  return new Date(ts * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
  });
}

export function fmtDay(ts: number): string {
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: "short", day: "2-digit" });
}

// Stable color per platform so the same source reads the same everywhere.
const PLATFORM_COLORS: Record<string, string> = {
  facebook: "#4267B2",
  instagram: "#C13584",
  snapchat: "#FFB800",
  whatsapp: "#25D366",
  telegram: "#229ED9",
  discord: "#5865F2",
  sms: "#34C759",
  calls: "#FF9F0A",
  contacts: "#64D2FF",
  browser: "#AF52DE",
  location: "#FF375F",
  email: "#5E5CE6",
  instagram_dm: "#C13584",
};

export function platformColor(p: string): string {
  return PLATFORM_COLORS[p] ?? "#8A8A8E";
}

const ENTITY_ICONS: Record<string, string> = {
  person: "◉",
  username: "@",
  phone: "☎",
  email: "✉",
  device_id: "▣",
  cookie: "◍",
  session_id: "⧉",
  location: "⌖",
  ip: "⇄",
  file_hash: "#",
  url: "↗",
};

export function entityIcon(kind: string): string {
  return ENTITY_ICONS[kind] ?? "•";
}

export const ENTITY_KINDS = [
  "person",
  "phone",
  "email",
  "username",
  "location",
  "device_id",
  "ip",
  "file_hash",
  "url",
] as const;
