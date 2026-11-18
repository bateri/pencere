// The map of the site: which page shows which file of the repository, and where the page stands in
// the menus. It is the one thing the site keeps about the documentation, and the rule it is kept by:
//
//   What changes when the code changes is said in the repository, beside the code: the
//   specifications (spec/draft, kit/SPEC.md), each package's README, the command's templates, the
//   comments of the kit's library, the examples, and the README at the top for what the whole is.
//   The site reads the files when it is built and shows them: a document whole or by its sections
//   (a page with a `source`, below), or values taken from one: a command, a block of code, a
//   list, a table, the sentence a section opens with (src/lib/project.js for the pages,
//   scripts/catalog.mjs for the elements and the components).
//
//   What the site writes itself is this map, how a page looks, the pictures, and the words that
//   lead a reader: the home page's account of what pencere is, the opening of the kit's overview,
//   a line under a heading that says where to go. Those words give no command and no block of
//   code, and a name they set as code is one a file of the repository has (scripts/check.mjs).
//   They are few, and when one of them would have to change with the code it is moved to the
//   repository and read from there.
//
// So a new document is a line here, and a new fact is a line in the repository. The site is built
// only when the two agree: every section of a document that is shown in parts is on one page or is
// named in LEFT, every document of the repository is shown, read or named in APART, and a page
// that asks a file for something it no longer has stops the build.

// A page: `path` is its address, `words` its name in a menu, `area` and `group` where the menu
// has it. A page with a `source` is that file of the repository, made by src/pages/[...path].astro;
// the others are pages of their own under src/pages, which read what they show (`reads`).
//   sections  the page is a part of the document: these sections, by the ids of their headings
//   opening   whether what the document says before its first section is shown (default: yes)
//   heading   a title in place of the document's own
//   alone     the page stands outside the menus, with the list of its own sections beside it
//   note      what else is said of the file at the page's end
export const PAGES = [
  // --- For those who write apps ---------------------------------------------------------------
  {
    path: "kit",
    words: "Overview",
    area: "apps",
    group: "Learn",
    reads: ["kit/SPEC.md", "kit/README.md"],
  },
  {
    path: "kit/start",
    words: "Start",
    area: "apps",
    group: "Learn",
    reads: ["README.md", "cli/README.md", "cli/src/new.rs", "cli/templates/lua/README.md"],
  },
  {
    path: "kit/examples",
    words: "Examples",
    area: "apps",
    group: "Learn",
    reads: ["examples/*/README.md"],
  },

  {
    path: "kit/elements",
    words: "Elements",
    area: "apps",
    group: "The page",
    reads: ["kit/SPEC.md"],
  },
  {
    path: "kit/components",
    words: "Components",
    area: "apps",
    group: "The page",
    reads: ["kit/SPEC.md", "kit/sandbox/library/ui"],
  },
  {
    path: "kit/library",
    words: "Library",
    area: "apps",
    group: "The page",
    reads: ["kit/SPEC.md"],
  },
  {
    path: "kit/spec",
    words: "The page API",
    area: "apps",
    group: "The page",
    source: "kit/SPEC.md",
    title: "The page API: the pencere kit",
    description:
      "The pencere kit in full: the elements and builders of a page, its events and replies, how it talks to its program, the library, the colours and the sandbox's floors.",
  },
  {
    path: "sdk/page",
    words: "A web page",
    area: "apps",
    group: "The page",
    source: "sdk/typescript/README.md",
    sections: ["the-pages-side"],
    opening: false,
    heading: "A web page",
    title: "A web page as a view: pencere",
    description:
      "The page's side of a web view: the bridge between a page and its program, the terminal's theme and font, and links.",
  },

  {
    path: "sdk/lua",
    words: "In Lua",
    area: "apps",
    group: "The program",
    source: "cli/README.md",
    sections: [
      "where-things-go",
      "an-app",
      "the-view-a-page-of-the-kit-a-web-page-or-both",
      "files-that-change",
      "files-with-the-view",
      "a-filter-in-a-pipeline",
      "giving-the-terminal-to-a-program",
    ],
    opening: false,
    heading: "A program in Lua",
    title: "A program in Lua: pencere",
    description:
      'A pencere app\'s program in Lua: how an app is laid out, what require("pencere") has, the views it can show, files, pipelines and lending the terminal to another program.',
  },
  {
    path: "sdk/typescript",
    words: "In TypeScript",
    area: "apps",
    group: "The program",
    source: "sdk/typescript/README.md",
    sections: [
      "views",
      "calls-and-topics",
      "files",
      "runtimes",
      "the-router",
      "versions",
      "license",
    ],
    heading: "A program in TypeScript",
    title: "A program in TypeScript: pencere",
    description:
      "The pencere SDK for TypeScript: showing a view from a program on Node, Deno or Bun, answering its calls and feeding its topics.",
  },
  {
    path: "sdk/rust",
    words: "In Rust",
    area: "apps",
    group: "The program",
    source: "program/README.md",
    heading: "A program in Rust",
    title: "A program in Rust: pencere",
    description:
      "The pencere crate: showing a view from a Rust program, answering its calls and feeding its subscriptions, and leaving the terminal as it was.",
  },
  {
    path: "kit/command",
    words: "The command",
    area: "apps",
    group: "The program",
    source: "cli/README.md",
    sections: ["install", "use", "releasing-an-app", "testing-an-app", "what-it-is-and-is-not"],
    heading: "The pencere command",
    title: "The pencere command",
    description:
      "The pencere command: installing it, making, running, testing, building and releasing an app.",
  },

  // --- For those who write a terminal ---------------------------------------------------------
  // They are few, and they come from the home page's part about terminals and from the footer:
  // these pages are in no menu of the pages for apps.
  {
    path: "host",
    words: "The library",
    area: "hosts",
    group: "For terminals",
    source: "host/rust/README.md",
    heading: "Add pencere to a terminal",
    title: "Add pencere to a terminal",
    description:
      "pencere-host, the Rust library for terminals that host views: what it holds of the protocol, and what the terminal supplies.",
  },
  {
    path: "host/kit",
    words: "The kit",
    area: "hosts",
    group: "For terminals",
    source: "kit/README.md",
    heading: "The kit in a terminal",
    title: "The kit in a terminal: pencere",
    description:
      "The pencere kit from a terminal's side: its layers, its library, pictures, versions, and how it is built.",
  },
  {
    path: "host/apps",
    words: "Releases of an app",
    area: "hosts",
    group: "For terminals",
    source: "spec/draft/apps.md",
    title: "Releases of an app: pencere",
    description:
      "The files that release a pencere app, so that a terminal can follow it: app.json and its signature.",
  },

  // --- The protocol ---------------------------------------------------------------------------
  {
    path: "spec",
    words: "The specification",
    source: "spec/draft/pencere.md",
    alone: true,
    note: ", where its fixtures are too",
    title: "The specification: pencere",
    description:
      "The pencere protocol in full: how a program in a terminal finds a host, opens a view in its pane, and talks to it.",
  },
];

// The sections of a document shown in parts that no page shows, and why not.
export const LEFT = {};

// The documents of the repository that are not pages, each with the reason. A pattern is the
// file's path, `*` for one name in it.
export const APART = [
  ["*/CHANGELOG.md", "what each release of a package changed: its page links to it"],
  ["*/*/CHANGELOG.md", "the same"],
  ["RELEASING.md", "for who releases the packages"],
  [
    "host/fake/README.md",
    "a tool run from a checkout of the repository: the libraries' documents lead to it there",
  ],
  ["wire/README.md", "a crate a host and a program take through their own libraries"],
  [
    "host/tui/README.md",
    "a crate a program links to draw its page in a terminal that hosts no view: its program leads to it",
  ],
  [
    "kit/drive/fonts/README.md",
    "where the faces the drawing layer carries come from, and their licenses",
  ],
  ["kit/*/README.md", "the crates inside the kit, for who works on it"],
  ["kit/editor/fixtures/README.md", "the fixtures of the editor's tests"],
  ["spec/draft/fixtures/README.md", "the fixtures of the wire, for who implements it"],
  [
    "cli/templates/*/README.md",
    "the README of a new project of another kind than the one Start makes",
  ],
];

// A page reads a file of the repository when it is the page's `source` or matches one of its
// `reads` (the home page's are here: it is no page of a menu).
export const HOME = ["README.md"];

// The pages of a menu: the groups of an area, in the order they are written above.
export const menu = (area) => {
  const groups = [];
  for (const page of PAGES) {
    if (page.area !== area) continue;
    let group = groups.find((entry) => entry.name === page.group);
    if (!group) groups.push((group = { name: page.group, pages: [] }));
    group.pages.push(page);
  }
  return groups;
};

// The page at an address.
export const page = (path) => {
  const found = PAGES.find((entry) => entry.path === path);
  if (!found) throw new Error(`The site's map (src/docs.js) has no page ${path}.`);
  return found;
};
