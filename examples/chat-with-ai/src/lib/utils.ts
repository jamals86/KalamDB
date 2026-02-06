import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function parseTimestamp(value: string | number | Date | null | undefined): Date | null {
  if (!value) return null;
  if (value instanceof Date) return value;
  if (typeof value === 'number') {
    return new Date(normalizeEpochToMs(value));
  }
  const parsed = new Date(value);
  if (!Number.isNaN(parsed.getTime())) return parsed;
  const asNumber = Number(value);
  if (!Number.isNaN(asNumber)) {
    return new Date(normalizeEpochToMs(asNumber));
  }
  return null;
}

function normalizeEpochToMs(value: number): number {
  if (value < 1_000_000_000_000) return value * 1000; // seconds
  if (value >= 1_000_000_000_000_000) return Math.floor(value / 1000); // microseconds
  if (value >= 1_000_000_000_000_000_000) return Math.floor(value / 1_000_000); // nanoseconds
  return value; // milliseconds
}
