// The map of the site: which page shows which file of the repository, and where the page stands in
// the menus. It is the one thing the site keeps about the documentation, and the rule it is kept by:
//
//   What changes when the code changes is said in the repository, beside the code: the
//   specifications (spec/draft, kit/spec), each package's README, the command's templates, the
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
//   whole     a link, `[words, address]`, to the same text on one page, after the document
//   code      a file of an example whose marked parts (`-- #region Name`) the page shows, each under
//             the heading of its name (scripts/regions.mjs)
//   element   the element of the kit this page is where to read of (a name of the constructors' table
//             of kit/spec/elements.md): the card of the elements' overview leads here, and the page
//             shows the element's picture and code (a topic's page, by src/layouts/Doc.astro; an
//             element's own, by src/pages/kit/elements/[name].astro). A page with none is not an
//             element's: the elements with no page of the map have one made from the catalogue.
//   own       a file of src/pages makes the page: it shows the document with what else the page has
//   also      the other menus the page is in, each `{ area, group }`
//   lift      the page is one section of the document, and the section is the page: its heading is the
//             page's title (once, not as a title over a heading), and the headings inside it are one
//             level up, and the page's sections in the side
//   of        the area of a page that is in no menu of its own (it opens under the line of another
//             page of the menu): what search puts before what
//   holds     the pages whose addresses are under this page's (an element's, a component's, a
//             module's) are in no menu, and open under its line; a page that is under none (the
//             index of the page API) is under no line, though its address is under the kit's
//   components  the document lists the components where this page is, and the page leads to where they
//             are shown instead of saying them again (the components' own pages)
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

  // The page API, a file for each topic (kit/spec). A topic longer than a page is shown as two: each
  // page is some of the file's sections. `own`: the page is made by a file of src/pages that shows
  // the document with what else it has to show.
  {
    path: "kit/spec/page",
    words: "A page",
    area: "apps",
    group: "The page",
    source: "kit/spec/page.md",
    title: "A page: the pencere kit",
    description:
      "The Lua page API: what a script returns, what it can use, and the functions the host calls.",
  },
  {
    path: "kit/elements",
    words: "Elements",
    area: "apps",
    group: "The page",
    holds: true,
    source: "kit/spec/elements.md",
    own: true,
    sections: ["the-builders", "text-field", "tip", "overlay", "gradient"],
    opening: false,
    title: "Elements: the pencere kit",
    description:
      "The elements a page of the pencere kit is made of: boxes, text, pictures, paths, sparklines and the editor of text.",
  },
  // Two elements say more than a row of the table: their pages show the section that says it. They
  // stand in no menu (the reader is under Elements, which lists them).
  {
    path: "kit/elements/text",
    words: "text",
    source: "kit/spec/elements.md",
    own: true,
    sections: ["text"],
    opening: false,
    element: "text",
  },
  {
    path: "kit/elements/rich",
    words: "rich",
    source: "kit/spec/elements.md",
    own: true,
    sections: ["rich-text"],
    opening: false,
    element: "rich",
  },
  {
    path: "kit/spec/canvas",
    words: "Canvas",
    area: "apps",
    group: "The page",
    source: "kit/spec/canvas.md",
    element: "canvas",
    title: "Canvas: the pencere kit",
    description: "Paths drawn in a box of a page: strokes, fills, gradients, shadows and dashes.",
  },
  {
    path: "kit/spec/accessibility",
    words: "Accessibility",
    area: "apps",
    group: "The page",
    source: "kit/spec/accessibility.md",
    title: "Accessibility: the pencere kit",
    description:
      "How a page says what its elements are to a screen reader, and how it asks whether the kit has a thing.",
  },
  {
    path: "kit/spec/colours",
    words: "Colours",
    area: "apps",
    group: "The page",
    source: "kit/spec/colours.md",
    title: "Colours: the pencere kit",
    description: "How a page says a colour: values, the theme's roles and the derived tones.",
  },
  {
    path: "kit/spec/counting",
    words: "Counting",
    area: "apps",
    group: "The page",
    source: "kit/spec/counting.md",
    title: "Counting: the pencere kit",
    description: "How a page counts places in a text: bytes of its UTF-8 and lines from 1.",
  },

  {
    path: "kit/spec/events",
    words: "Events",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/events.md",
    sections: ["what-a-page-is-told"],
    title: "Events: the pencere kit",
    description:
      "What the host tells a page: the pointer, the keys, the wheel, fields, files and the time.",
  },
  {
    path: "kit/spec/events-replies",
    words: "Answering events",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/events.md",
    sections: ["what-a-reply-may-ask"],
    opening: false,
    heading: "Answering events",
    title: "Answering events: the pencere kit",
    description: "How a page answers an event: what a reply is, and what it may ask of an editor.",
  },
  {
    path: "kit/spec/replies",
    words: "Replies",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/replies.md",
    title: "Replies: the pencere kit",
    description:
      "What a reply asks of the host: filling a field, opening a link, taking files, picking a file, waking the page, drawing larger, vibrating the trackpad and playing a sound.",
  },
  {
    path: "kit/spec/haptics",
    words: "Haptics",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/haptics.md",
    title: "Haptics: the pencere kit",
    description:
      "What a page asks of the trackpad: a ready pattern by its name or one of its own, and what a host does with it.",
  },
  {
    path: "kit/spec/sounds",
    words: "Sounds",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/sounds.md",
    title: "Sounds: the pencere kit",
    description:
      "What a page asks of the speakers: a sound of the kit's by its name or a file of its own, and what a host does with it.",
  },
  {
    path: "kit/spec/commands",
    words: "Commands and dragging",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/commands.md",
    title: "Commands and dragging: the pencere kit",
    description:
      "Commands a person can run, what can be dragged, and what can be carried out of the view.",
  },
  {
    path: "kit/spec/motion",
    words: "Motion",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/motion.md",
    code: "examples/motion-lua/src/view/page.lua",
    title: "Motion: the pencere kit",
    description: "How a change of an element moves instead of jumping.",
  },
  {
    path: "kit/spec/layout",
    words: "Layout and scrolling",
    area: "apps",
    group: "Behaviour",
    source: "kit/spec/layout.md",
    title: "Layout and scrolling: the pencere kit",
    description:
      "What a page remembers between frames, what it asks of the layout, timers, scrolling and zooming.",
  },

  {
    path: "kit/spec/editor",
    words: "Editor",
    area: "apps",
    group: "Editor and browser",
    source: "kit/spec/editor.md",
    sections: ["an-editor-and-its-language", "what-is-not-seen-the-document-and-its-places"],
    element: "editor",
    title: "The editor: the pencere kit",
    description:
      "The editor of text a page puts in a room: its document, its language and what is not seen.",
  },
  {
    path: "kit/spec/editor-options",
    words: "Editor options",
    area: "apps",
    group: "Editor and browser",
    source: "kit/spec/editor.md",
    sections: ["how-an-editor-looks"],
    opening: false,
    heading: "The editor's options",
    title: "The editor's options: the pencere kit",
    description: "How an editor looks and behaves: the options a page gives it.",
  },
  {
    path: "kit/spec/editor-marks",
    words: "Editor marks",
    area: "apps",
    group: "Editor and browser",
    source: "kit/spec/editor-marks.md",
    sections: ["marks-zones-and-widgets"],
    opening: false,
    heading: "Marks, zones and widgets",
    title: "Marks, zones and widgets of the editor: the pencere kit",
    description:
      "What a page puts on an editor: decorations, markers, the gutter, zones, links and widgets.",
  },
  {
    path: "kit/spec/editor-keys",
    words: "Editor keys and searches",
    area: "apps",
    group: "Editor and browser",
    source: "kit/spec/editor-marks.md",
    sections: ["keys-commands-and-searches"],
    opening: false,
    heading: "Keys, commands and searches",
    title: "Keys, commands and searches of the editor: the pencere kit",
    description: "The editor's own keys, its commands and its searches.",
  },
  {
    path: "kit/spec/browser",
    words: "Browser",
    area: "apps",
    group: "Editor and browser",
    source: "kit/spec/browser.md",
    element: "browser",
    title: "Browser: the pencere kit",
    description:
      "The browser a page puts in a room of its own: its options, what a page asks of it and what it tells.",
  },

  {
    path: "kit/library",
    words: "Modules",
    area: "apps",
    group: "Library",
    holds: true,
    source: "kit/spec/library.md",
    own: true,
    // What the document says of the library, before its modules: each module has a page of its own.
    sections: [],
    title: "The library: the pencere kit",
    description:
      "The modules of the pencere kit's library: components, syntax colouring, languages, fuzzy finding, keys, Markdown, the terminal's own look and what a page asks of its program.",
  },
  ...[
    [
      "syntax",
      "How a page defines a language that colours an editor, as CodeMirror's StreamLanguage does.",
    ],
    [
      "languages",
      "The languages an editor reads: finding one, loading it and the looks of its tokens.",
    ],
    [
      "fuzzy",
      "Finding what a person types in texts, as a picker, a palette or a filter does: matching, scoring and marking.",
    ],
    [
      "keys",
      "Keys as a page writes them and as a person reads them: caps, labels, the symbols of each platform and key events.",
    ],
    [
      "markdown",
      "Reading Markdown into a tree, drawing it, and keeping a drawn document for a text that changes.",
    ],
    ["terminal", "Editors that look as the person's terminal draws its cursor and its selection."],
    [
      "ui",
      "The components of a page, written on the page API alone: frames, popovers, menus, pickers, grids and trees.",
      { components: true },
    ],
    [
      "charts",
      "Charts drawn on a canvas: the model, the painter and the sample under the pointer.",
    ],
    [
      "query",
      "What a page asks its program for, kept by the method and params: loading, ready and failed, old data while a new answer is awaited, polling and answers let go of.",
    ],
  ].map(([name, description, more]) => ({
    path: `kit/library/${name}`,
    words: `@pencere/${name}`,
    of: "apps",
    source: "kit/spec/library.md",
    sections: [`pencere${name}`],
    opening: false,
    lift: true,
    crumbs: [["The library", "/kit/library"]],
    title: `@pencere/${name}: the pencere kit's library`,
    description,
    ...more,
  })),
  {
    path: "kit/components",
    words: "Components",
    area: "apps",
    group: "Library",
    holds: true,
    reads: ["kit/spec/library.md", "kit/sandbox/library/ui"],
  },

  {
    path: "kit/spec/program",
    words: "The page and its program",
    area: "apps",
    group: "The program",
    source: "kit/spec/program.md",
    title: "The page and its program: the pencere kit",
    description:
      "How a page talks to its program, what print and warn do, and the other scripts of a bundle.",
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
      "what-the-terminal-hands-a-running-program",
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
  {
    path: "sdk/page",
    words: "A web page",
    area: "apps",
    group: "The program",
    source: "sdk/typescript/README.md",
    sections: ["the-pages-side"],
    opening: false,
    heading: "A web page",
    title: "A web page as a view: pencere",
    description:
      "The page's side of a web view: the bridge between a page and its program, the terminal's theme and font, and links.",
  },

  // The index of the topics, and the topics on one page (src/pages/kit/spec/all.astro, in no menu).
  {
    path: "kit/spec",
    words: "The page API",
    source: "kit/SPEC.md",
    whole: ["Every topic on one page, to search in the browser or to print", "/kit/spec/all"],
    title: "The page API: the pencere kit",
    description:
      "The topics of the pencere kit's page API: elements, events and replies, motion, the editor, the library, colours and what a host does.",
  },
  {
    path: "kit/spec/versions",
    words: "Versions",
    area: "apps",
    group: "Learn",
    also: [{ area: "hosts", group: "For terminals" }],
    source: "kit/spec/versions.md",
    title: "Versions: the pencere kit",
    description: "The version of the kind lua, what the kit pins, and when a version changes.",
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
    path: "host/kit-view",
    words: "What a host does for a view",
    area: "hosts",
    group: "For terminals",
    source: "kit/spec/host.md",
    title: "What a host does for a view of the kit: pencere",
    description:
      "What a terminal gives a view of the kit at least: the sandbox's floors, what it draws and does for a page, and the pictures.",
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
    for (const place of [page, ...(page.also ?? [])]) {
      if (place.area !== area) continue;
      let group = groups.find((entry) => entry.name === place.group);
      if (!group) groups.push((group = { name: place.group, pages: [] }));
      group.pages.push(page);
    }
  }
  return groups;
};

// The page at an address.
export const page = (path) => {
  const found = PAGES.find((entry) => entry.path === path);
  if (!found) throw new Error(`The site's map (src/docs.js) has no page ${path}.`);
  return found;
};

// Where an element is read of: the page of the map that names it (`element`), and for one the map
// has no page for, the page made of the catalogue (src/pages/kit/elements/[name].astro).
export const elementPath = (name) =>
  PAGES.find((entry) => entry.element === name)?.path ?? `kit/elements/${name}`;

// The elements that are read of on the page of a topic of the page API, which shows their picture
// (the others have a page of their own, made of the catalogue or of a section of the document).
export const topicElements = () =>
  PAGES.filter((entry) => entry.element && !entry.own).map((entry) => entry.element);

// The pages of the library's modules (each is a section of kit/spec/library.md that is the page), in the
// map's order, which is the document's.
export const libraryPages = () =>
  PAGES.filter((entry) => entry.source === "kit/spec/library.md" && entry.lift);
