// Breadcrumbs for the pane path bar: a directory path split into the
// folders it passes through, each with the path to navigate to.
//
// Pane paths come in three shapes: POSIX (`/var/www`, also FTP and S3, whose
// pane paths are `/bucket/prefix`), Windows drive paths (`C:\Users`, either
// slash), and UNC shares (`\\server\share\dir`, the share is the root —
// `parentPath` never goes above it either).

export interface PathSegment {
  /** Text shown for this crumb. */
  label: string;
  /** Path to open when the crumb is clicked. */
  path: string;
}

export interface PathCrumbs {
  segments: PathSegment[];
  /** Separator drawn between folder crumbs (none after the root). */
  separator: "/" | "\\";
}

export function pathCrumbs(path: string): PathCrumbs {
  if (path.startsWith("\\\\")) return uncCrumbs(path);
  const drive = path.match(/^([A-Za-z]:)(?:[\\/]|$)/);
  if (drive) return driveCrumbs(path, drive[1]);
  return posixCrumbs(path);
}

function posixCrumbs(path: string): PathCrumbs {
  const absolute = path.startsWith("/");
  const names = path.split("/").filter(Boolean);
  const segments: PathSegment[] = absolute ? [{ label: "/", path: "/" }] : [];
  let current = absolute ? "" : null;
  for (const name of names) {
    current = current === null ? name : `${current}/${name}`;
    segments.push({ label: name, path: current });
  }
  return { segments, separator: "/" };
}

function driveCrumbs(path: string, drive: string): PathCrumbs {
  // Keep the slash style the path already uses; backslash by default.
  const separator = path.charAt(2) === "/" ? "/" : "\\";
  const root = `${drive}${separator}`;
  const names = path.slice(2).split(/[\\/]/).filter(Boolean);
  const segments: PathSegment[] = [{ label: root, path: root }];
  let current = drive;
  for (const name of names) {
    current = `${current}${separator}${name}`;
    segments.push({ label: name, path: current });
  }
  return { segments, separator };
}

function uncCrumbs(path: string): PathCrumbs {
  const parts = path.slice(2).split(/[\\/]/).filter(Boolean);
  if (parts.length === 0) return { segments: [], separator: "\\" };
  const root = `\\\\${parts.slice(0, 2).join("\\")}`;
  const segments: PathSegment[] = [{ label: root, path: root }];
  let current = root;
  for (const name of parts.slice(2)) {
    current = `${current}\\${name}`;
    segments.push({ label: name, path: current });
  }
  return { segments, separator: "\\" };
}
