// Formatting helpers for file listings and transfers.

import type { PublicConnection, SizeFormat } from "$lib/api";

export function formatSize(bytes: number, format: SizeFormat = "kib"): string {
  const base = format === "kb" ? 1000 : 1024;
  const units =
    format === "kb" ? ["B", "KB", "MB", "GB", "TB"] : ["B", "KiB", "MiB", "GiB", "TiB"];
  if (bytes < base) return `${bytes} B`;
  let value = bytes;
  let unit = 0;
  while (value >= base && unit < units.length - 1) {
    value /= base;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function formatMtime(mtimeUnix: number | null | undefined): string {
  if (!mtimeUnix) return "";
  const date = new Date(mtimeUnix * 1000);
  const now = new Date();
  const sameYear = date.getFullYear() === now.getFullYear();
  const pad = (n: number) => String(n).padStart(2, "0");
  const day = `${pad(date.getDate())}.${pad(date.getMonth() + 1)}`;
  return sameYear
    ? `${day} ${pad(date.getHours())}:${pad(date.getMinutes())}`
    : `${day}.${date.getFullYear()}`;
}

/** `0o755` → `rwxr-xr-x`. */
export function formatPermissions(mode: number | null | undefined): string {
  if (mode == null) return "";
  const bits = "rwxrwxrwx";
  let out = "";
  for (let i = 0; i < 9; i++) {
    out += mode & (0o400 >> i) ? bits[i] : "-";
  }
  return out;
}

export function formatSpeed(bps: number): string {
  if (bps <= 0) return "";
  return `${formatSize(bps)}/s`;
}

export function formatEta(bytesLeft: number, bps: number): string {
  if (bps <= 0 || bytesLeft <= 0) return "";
  const s = Math.round(bytesLeft / bps);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}

/** Parent of a path, both `/a/b` and `C:`-less local unix paths. */
export function parentPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  if (idx <= 0) return "/";
  return trimmed.slice(0, idx);
}

export function joinPath(dir: string, name: string): string {
  return dir.endsWith("/") ? dir + name : `${dir}/${name}`;
}

/**
 * Public URL of an S3 object shown in the remote pane (SPEC §4.4).
 * Prefers the configured CDN/custom-domain base; otherwise builds a
 * virtual-host (or path-style) URL from the endpoint. `path` is the pane
 * path: `/key…` with a fixed bucket, `/bucket/key…` without one.
 */
export function s3PublicUrl(conn: PublicConnection, path: string): string | null {
  if (conn.protocol !== "s3") return null;
  const rel = path.replace(/^\/+/, "");
  let bucket = conn.s3?.bucket ?? null;
  let key = rel;
  if (!bucket) {
    const slash = rel.indexOf("/");
    if (slash <= 0) return null; // bucket itself has no object URL
    bucket = rel.slice(0, slash);
    key = rel.slice(slash + 1);
  }
  if (!key) return null;
  const encodedKey = key.split("/").map(encodeURIComponent).join("/");
  const base = conn.s3?.public_base_url?.replace(/\/+$/, "");
  if (base) {
    // A CDN base maps to the bucket root when the bucket is fixed.
    return conn.s3?.bucket ? `${base}/${encodedKey}` : `${base}/${bucket}/${encodedKey}`;
  }
  const hostPort = conn.port === 443 || conn.port === 0 ? conn.host : `${conn.host}:${conn.port}`;
  return conn.s3?.path_style
    ? `https://${hostPort}/${bucket}/${encodedKey}`
    : `https://${bucket}.${hostPort}/${encodedKey}`;
}
