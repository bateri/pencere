/**
 * Watching files with `node:fs`, for `pencere dev`.
 *
 * `fs.watch` says that something under a path changed, not what, and says it more
 * than once for one write; that is enough, because `Reloader` waits for the files
 * to be quiet and looks at what they hold before it acts. A directory is watched
 * recursively; a file is watched through the directory it is in, because an
 * editor that saves by writing a new file and renaming it over the old one leaves
 * a watch on the file itself watching a file that is gone.
 */

import { statSync, watch as watchPath } from "node:fs";
import { readdir, stat } from "node:fs/promises";
import { basename, dirname, join } from "node:path";

/** What a watched path is for: the view's files, or the server's own. */
export type Role = "view" | "server";

/** A path to watch and what a change to it means. */
export interface Root {
  readonly path: string;
  readonly role: Role;
}

/** A set of watches, to end. */
export interface Watching {
  close(): void;
}

/** Starts watching `roots`; `onChange` is told the role of a root something changed under. */
export type Watch = (
  roots: readonly Root[],
  onChange: (role: Role) => void,
  onError: (message: string) => void,
) => Watching;

/** A path that cannot be watched: it is not there, or the system would not. */
export class WatchError extends Error {
  readonly missing: boolean;

  constructor(message: string, missing: boolean) {
    super(message);
    this.name = "WatchError";
    this.missing = missing;
  }
}

/**
 * Whether a path under a watched directory is nothing a person edits: a name that
 * begins with a dot (a view's own walk leaves them out too), the packages a
 * project installs and the bytecode a language writes beside its source, which a
 * server writes by running and so would end every run with a change.
 */
export function isNoise(relative: string): boolean {
  return relative
    .split(/[\\/]/)
    .some((part) => part.startsWith(".") || part === "node_modules" || part === "__pycache__");
}

function describe(error: unknown): string {
  const code = (error as NodeJS.ErrnoException | undefined)?.code;
  if (code === "ENOENT") return "no such file or directory";
  if (code === "EACCES") return "permission denied";
  if (code === "EMFILE" || code === "ENOSPC") return "the system has no more watches to give";
  return error instanceof Error ? error.message : String(error);
}

/**
 * Watches `roots` with `fs.watch`.
 *
 * @throws {WatchError} when a root cannot be watched; what was started before it is ended.
 */
export const watchRoots: Watch = (roots, onChange, onError) => {
  const watchers: { close(): void }[] = [];
  const close = (): void => {
    for (const watcher of watchers.splice(0)) watcher.close();
  };
  try {
    for (const root of roots) watchers.push(watchRoot(root, onChange, onError));
  } catch (error) {
    close();
    throw error;
  }
  return { close };
};

function watchRoot(
  root: Root,
  onChange: (role: Role) => void,
  onError: (message: string) => void,
): { close(): void } {
  let isDirectory: boolean;
  try {
    isDirectory = statSync(root.path).isDirectory();
  } catch (error) {
    const missing = (error as NodeJS.ErrnoException | undefined)?.code === "ENOENT";
    throw new WatchError(`cannot watch ${root.path}: ${describe(error)}`, missing);
  }
  const heard = (name: string | Buffer | null): void => {
    // `null` is a system that does not say which file: it may be this one.
    if (name === null) return onChange(root.role);
    const text = name.toString();
    if (isDirectory ? !isNoise(text) : text === basename(root.path)) onChange(root.role);
  };
  let watcher: ReturnType<typeof watchPath>;
  try {
    watcher = isDirectory
      ? watchPath(root.path, { recursive: true }, (_event, name) => heard(name))
      : watchPath(dirname(root.path), (_event, name) => heard(name));
  } catch (error) {
    throw new WatchError(`cannot watch ${root.path}: ${describe(error)}`, false);
  }
  watcher.on("error", (error) => {
    watcher.close();
    onError(`stopped watching ${root.path}: ${describe(error)}`);
  });
  return watcher;
}

/**
 * What the paths hold, in a form that changes when they do: the size and the
 * modification time of every file under them, noise left out. A path that is not
 * there is part of it, so that a file that went away is a change too.
 */
export async function fingerprintOf(paths: readonly string[]): Promise<string> {
  const lines: string[] = [];
  const visit = async (path: string): Promise<void> => {
    let info;
    try {
      info = await stat(path);
    } catch {
      lines.push(`${path} missing`);
      return;
    }
    if (!info.isDirectory()) {
      lines.push(`${path} ${info.size} ${info.mtimeMs}`);
      return;
    }
    let entries;
    try {
      entries = await readdir(path, { withFileTypes: true });
    } catch {
      lines.push(`${path} unreadable`);
      return;
    }
    entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    for (const entry of entries) {
      // A link to a directory is not followed, so that a loop cannot trap the walk.
      if (
        isNoise(entry.name) ||
        (entry.isSymbolicLink() && (await isDirectoryLink(join(path, entry.name))))
      )
        continue;
      await visit(join(path, entry.name));
    }
  };
  for (const path of paths) await visit(path);
  return lines.join("\n");
}

async function isDirectoryLink(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isDirectory();
  } catch {
    return false;
  }
}
