// Local natural-language assistant.
//
// The design requires an assistant that answers questions over the imported
// data and *cites the underlying records instead of inventing information*.
// This implementation honours that literally: it is a deterministic query
// translator, not a generative model. It maps a question to concrete search
// filters, runs the same engine the rest of the UI uses, and returns the real
// records as citations. It never asserts anything not backed by a record.

import { api } from "./api";
import type { RecordRow, SearchFilters } from "./types";
import { fmtDate } from "./util";

export interface AiAnswer {
  text: string;
  interpretation: string;
  citations: RecordRow[];
}

const MONTHS = [
  "january", "february", "march", "april", "may", "june",
  "july", "august", "september", "october", "november", "december",
];

const PLATFORMS = [
  "facebook", "instagram", "snapchat", "whatsapp", "telegram", "discord",
  "sms", "calls", "contacts", "browser", "location", "email",
];

function monthRange(name: string, year: number): [number, number] {
  const m = MONTHS.indexOf(name);
  const start = Date.UTC(year, m, 1) / 1000;
  const end = Date.UTC(m === 11 ? year + 1 : year, (m + 1) % 12, 1) / 1000 - 1;
  return [start, end];
}

function dayRange(y: number, m: number, d: number): [number, number] {
  const start = Date.UTC(y, m, d) / 1000;
  return [start, start + 86399];
}

/** Interpret and answer a question. */
export async function ask(question: string): Promise<AiAnswer> {
  const q = question.toLowerCase();
  const filters: SearchFilters = { limit: 100 };
  const notes: string[] = [];

  // Platform mention -> platform filter.
  const platform = PLATFORMS.find((p) => q.includes(p));
  if (platform) {
    filters.platform = platform;
    notes.push(`platform=${platform}`);
  }

  // Explicit date "july 12" / "july 12 2024" / "march 2024".
  const now = new Date();
  const monthDay = q.match(
    /\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\s+(\d{1,2})(?:,?\s*(\d{4}))?/
  );
  const monthOnly = q.match(
    /\b(january|february|march|april|may|june|july|august|september|october|november|december)\s+(\d{4})/
  );
  if (monthDay) {
    const mi = MONTHS.findIndex((m) => m.startsWith(monthDay[1]));
    const year = monthDay[3] ? +monthDay[3] : now.getUTCFullYear();
    const [a, b] = dayRange(year, mi, +monthDay[2]);
    filters.after = a;
    filters.before = b;
    notes.push(`day=${fmtDate(a)}`);
  } else if (monthOnly) {
    const [a, b] = monthRange(monthOnly[1], +monthOnly[2]);
    filters.after = a;
    filters.before = b;
    notes.push(`month=${monthOnly[1]} ${monthOnly[2]}`);
  }

  // Pull an explicit identifier or quoted/target phrase to search on.
  const idMatch =
    q.match(/[\w.+-]+@[\w.-]+\.\w+/) || q.match(/\+?\d[\d\s().-]{6,}\d/);
  const withMatch = question.match(
    /(?:with|to|from|mentioning|about|contact)\s+([A-Za-z0-9 ._@+-]{2,40})/i
  );

  let term = "";
  if (idMatch) {
    term = idMatch[0];
    notes.push(`identifier="${term.trim()}"`);
  } else if (withMatch) {
    term = withMatch[1].replace(/\b(on|in|during|the|all|imports?)\b.*$/i, "").trim();
    notes.push(`subject="${term}"`);
  }

  // Run the query.
  const hits = term
    ? await api.search(term, filters)
    : await api.timeline(filters);

  // Compose a grounded answer.
  const n = hits.length;
  let text: string;
  if (n === 0) {
    text = "I found no records matching that. Try a name, number, email, date, or platform.";
  } else {
    const platforms = [...new Set(hits.map((h) => h.platform))];
    const span =
      hits.map((h) => h.timestamp).filter((t): t is number => t != null);
    const subject = term ? `for “${term.trim()}”` : "in this window";
    const range =
      span.length > 0
        ? ` between ${fmtDate(Math.min(...span))} and ${fmtDate(Math.max(...span))}`
        : "";
    text =
      `Found ${n} record${n === 1 ? "" : "s"} ${subject}${range}, ` +
      `across ${platforms.length} source${platforms.length === 1 ? "" : "s"} ` +
      `(${platforms.join(", ")}). Citations below reference the exact records.`;
  }

  return {
    text,
    interpretation: notes.length ? notes.join("  ·  ") : "free-text search",
    citations: hits.slice(0, 12),
  };
}

export const SUGGESTED_PROMPTS = [
  "Show every conversation with John",
  "Activity on March 9",
  "Messages mentioning cafe",
  "Find 555-123-4567",
];
