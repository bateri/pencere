/**
 * A view of many files, listed with the SHA-256 of each (specification §8).
 *
 * The program builds a bundle once: from a directory, or from files it holds in
 * memory. The manifest goes into `open`; the files stay here until the host
 * says, in `need`, which of them it lacks.
 */

import { createHash } from "node:crypto";
import { readFile, readdir, stat } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** The limits of a bundle (specification §8.2): constants of the protocol. */
export const BUNDLE_LIMITS = {
  files: 1024,
  file: 8 * 1024 * 1024,
  total: 64 * 1024 * 1024,
} as const;

const MAX_PATH = 255;

/**
 * The `_meta.ui` of the view (MCP Apps § UI Resource Format): what the host
 * builds the content security policy from, which the person is asked about
 * (specification §12), and whether the view wants a border.
 */
export interface ViewMeta {
  csp?: {
    connectDomains?: string[];
    resourceDomains?: string[];
    frameDomains?: string[];
    baseUriDomains?: string[];
  };
  permissions?: {
    camera?: object;
    microphone?: object;
    geolocation?: object;
    clipboardWrite?: object;
  };
  domain?: string;
  prefersBorder?: boolean;
}

/** One file of a manifest, as `open` carries it. */
export interface ManifestFile {
  readonly path: string;
  readonly sha256: string;
  readonly size: number;
  readonly type: string;
}

/** The manifest of `open.bundle`. */
export interface BundleManifest {
  readonly entry: string;
  readonly files: readonly ManifestFile[];
  readonly meta?: ViewMeta;
}

/** Why a set of files is not a bundle. */
export class BundleError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BundleError";
  }
}

/** A bundle: its manifest and the bytes of its files. */
export class Bundle {
  readonly manifest: BundleManifest;
  readonly #bytes: ReadonlyMap<string, Uint8Array>;

  constructor(manifest: BundleManifest, bytes: ReadonlyMap<string, Uint8Array>) {
    this.manifest = manifest;
    this.#bytes = bytes;
  }

  /** The file whose SHA-256 is `digest`, if the bundle has one. */
  read(digest: string): Uint8Array | undefined {
    return this.#bytes.get(digest);
  }
}

/** A file to put in a bundle. */
export interface BundleFile {
  /** Relative, `/`-separated, 1 to 255 bytes, every segment of `A-Z a-z 0-9 . _ ~ -` and not `.` or `..`. */
  path: string;
  data: Uint8Array;
  /** `type/subtype`, lowercase, without parameters; by default from the path's extension. */
  type?: string;
}

/** What `bundleFromFiles` and `bundleFromDirectory` take besides the files. */
export interface BundleOptions {
  /** The path of the document; `index.html` by default. */
  entry?: string;
  /** The view's `_meta.ui`. */
  meta?: ViewMeta;
}

/** What `bundleFromDirectory` takes. */
export interface DirectoryOptions extends BundleOptions {
  /** Media types by extension (with the dot, lowercase), over the built-in ones. */
  types?: Readonly<Record<string, string>>;
}

const MEDIA_TYPES: Readonly<Record<string, string>> = {
  ".html": "text/html",
  ".htm": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".map": "application/json",
  ".txt": "text/plain",
  ".md": "text/markdown",
  ".csv": "text/csv",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".avif": "image/avif",
  ".ico": "image/x-icon",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
  ".otf": "font/otf",
  ".wasm": "application/wasm",
  ".mp3": "audio/mpeg",
  ".ogg": "audio/ogg",
  ".wav": "audio/wav",
  ".mp4": "video/mp4",
  ".webm": "video/webm",
  ".xml": "application/xml",
  ".pdf": "application/pdf",
};

const SEGMENT = /^[A-Za-z0-9._~-]+$/;
const MEDIA_NAME = /^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$/;

/** Whether `path` is one a manifest may list (specification §8.2). */
export function isBundlePath(path: string): boolean {
  if (Buffer.byteLength(path) < 1 || Buffer.byteLength(path) > MAX_PATH) return false;
  return path.split("/").every((s) => s !== "." && s !== ".." && SEGMENT.test(s));
}

/** Whether `text` is a media type a manifest may list: `type/subtype`, lowercase, no parameters. */
export function isMediaType(text: string): boolean {
  const parts = text.split("/");
  return parts.length === 2 && parts.every((p) => MEDIA_NAME.test(p));
}

/** The media type of a path by its extension; `application/octet-stream` when it is not known. */
export function mediaTypeOf(path: string, types: Readonly<Record<string, string>> = {}): string {
  const dot = path.lastIndexOf(".");
  const slash = path.lastIndexOf("/");
  const extension = dot > slash + 1 ? path.slice(dot).toLowerCase() : "";
  return types[extension] ?? MEDIA_TYPES[extension] ?? "application/octet-stream";
}

/** The SHA-256 of `data`, as 64 lowercase hex digits. */
export function sha256(data: Uint8Array): string {
  return createHash("sha256").update(data).digest("hex");
}

/**
 * A bundle of files held in memory. The manifest lists them in the order given:
 * the digests the host asks for come in that order, and a program that wants
 * the document first puts it first.
 *
 * @throws {BundleError} when a path, a type, a limit or the entry is not what
 * specification §8.2 allows.
 */
export function bundleFromFiles(files: readonly BundleFile[], options: BundleOptions = {}): Bundle {
  if (files.length > BUNDLE_LIMITS.files)
    throw new BundleError(
      `a bundle holds at most ${BUNDLE_LIMITS.files} files, not ${files.length}`,
    );
  const listed: ManifestFile[] = [];
  const bytes = new Map<string, Uint8Array>();
  const paths = new Set<string>();
  let total = 0;
  for (const file of files) {
    if (!isBundlePath(file.path))
      throw new BundleError(
        `"${file.path}" is not a path a bundle may list: 1 to ${MAX_PATH} bytes, segments of A-Z a-z 0-9 . _ ~ - and not . or ..`,
      );
    if (paths.has(file.path)) throw new BundleError(`"${file.path}" is listed twice`);
    paths.add(file.path);
    const type = file.type ?? mediaTypeOf(file.path);
    if (!isMediaType(type))
      throw new BundleError(`"${type}" is not a media type a bundle may list for "${file.path}"`);
    if (file.data.length > BUNDLE_LIMITS.file)
      throw new BundleError(
        `"${file.path}" is ${file.data.length} bytes; a file of a bundle holds at most ${BUNDLE_LIMITS.file}`,
      );
    total += file.data.length;
    if (total > BUNDLE_LIMITS.total)
      throw new BundleError(`a bundle holds at most ${BUNDLE_LIMITS.total} bytes in all`);
    const digest = sha256(file.data);
    listed.push({ path: file.path, sha256: digest, size: file.data.length, type });
    bytes.set(digest, file.data);
  }
  const entry = options.entry ?? "index.html";
  if (!paths.has(entry)) throw new BundleError(`the entry "${entry}" is not one of the files`);
  const manifest: BundleManifest =
    options.meta === undefined
      ? { entry, files: listed }
      : { entry, files: listed, meta: options.meta };
  return new Bundle(manifest, bytes);
}

async function walk(
  root: string,
  relative: string,
  found: BundleFile[],
  types: Record<string, string>,
) {
  const dir = relative === "" ? root : join(root, relative);
  for (const item of await readdir(dir, { withFileTypes: true })) {
    // A name that begins with a dot is not part of a view: `.DS_Store`, `.git`.
    if (item.name.startsWith(".")) continue;
    const path = relative === "" ? item.name : `${relative}/${item.name}`;
    let isDirectory = item.isDirectory();
    let isFile = item.isFile();
    if (item.isSymbolicLink()) {
      // A link to a file is that file; a link to a directory is skipped, so a loop cannot trap the walk.
      const target = await stat(join(root, path)).catch(() => undefined);
      isFile = target?.isFile() ?? false;
      isDirectory = false;
    }
    if (isDirectory) await walk(root, path, found, types);
    else if (isFile) {
      const data = await readFile(join(root, path));
      found.push({ path, data, type: mediaTypeOf(path, types) });
    }
  }
}

/**
 * A bundle of the files under `directory`, recursively, listed in byte order of
 * their paths. Names that begin with a dot are left out; a link to a directory is
 * not followed.
 *
 * @throws {BundleError} as {@link bundleFromFiles} does; an unreadable directory
 * throws what the file system throws.
 */
export async function bundleFromDirectory(
  directory: string | URL,
  options: DirectoryOptions = {},
): Promise<Bundle> {
  const root = directory instanceof URL ? fileURLToPath(directory) : directory;
  const found: BundleFile[] = [];
  await walk(root, "", found, { ...options.types });
  // A directory has no order of its own: the manifest of the same files is the same manifest.
  found.sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
  const { types: _types, ...rest } = options;
  return bundleFromFiles(found, rest);
}
