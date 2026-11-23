// The notes of a document. A Markdown footnote (`[^name]`) says what holds nothing a page or a host has
// to do: a comparison, an example. The document keeps it as GitHub shows it, after the text; the site
// shows it where it is said. On a wide screen the note stands in the margin beside the line it is said
// at, and on a narrow one it is at the end of the page, as the document has it: the two are one text,
// one of them shown (the styles of spec.css).
//
// What the builder makes of a footnote is a number after the words, `<sup><a href="#user-content-fn-x"
// id="user-content-fnref-x" ...>1</a></sup>`, and at the end a section of the notes, a list, each with a
// link back. `takeNotes` takes that section out of the document (and with it the heading the builder
// gives it, "Footnotes", that no reader of the page is to meet twice); `placeNotes` says each note after
// its number, in the part of the document that is shown, and gives back the section with the notes of
// that part alone.

const SECTION = /<section data-footnotes[^>]*>[\s\S]*?<\/section>\s*/g;
const ITEM = /<li id="user-content-fn-([^"]*)">\s*([\s\S]*?)\s*<\/li>/g;
const BACK = /\s*<a [^>]*data-footnote-backref[^>]*>[^<]*<\/a>/g;

/**
 * The html without its notes, and the notes: a map of name to the html of the note. A page of several
 * documents (the topics of the kit on one page) has a section of notes for each, and they are taken
 * together: a name belongs to one note (scripts/links.mjs sees to that).
 */
export function takeNotes(html) {
  const notes = new Map();
  for (const found of html.match(SECTION) ?? [])
    for (const [, name, body] of found.matchAll(ITEM)) notes.set(name, body);
  return { html: html.replace(SECTION, ""), notes };
}

/**
 * The html of a part of the document with each note said after its number, and the section of the
 * notes the part says (an empty text where it says none). A note of several paragraphs is one, said in
 * a line: a note is a sentence or two.
 */
export function placeNotes(html, notes) {
  const used = new Set();
  const placed = html.replace(
    /<sup><a href="#user-content-fn-([^"]*)" id="user-content-fnref-[^"]*"[^>]*>\d+<\/a><\/sup>/g,
    (whole, name) => {
      const body = notes.get(name);
      if (body === undefined) return whole;
      used.add(name);
      // Numbered as they come on this page (the builder numbers by the document, and a page of several
      // documents, or of a part of one, would not agree with the list of the notes at its end).
      const number = used.size;
      const words = body
        .replace(BACK, "")
        .replace(/<\/p>\s*<p>/g, " ")
        .replace(/<\/?p>/g, "")
        .trim();
      // Said twice: the number links to the note at the end of the page, where the margin has no room for
      // it, and is no link where the note stands beside it (the styles of spec.css show one of the two).
      return (
        `<sup class="note__mark"><a class="note__end" href="#user-content-fn-${name}" id="user-content-fnref-${name}">${number}</a>` +
        `<span class="note__here" aria-hidden="true" data-pagefind-ignore>${number}</span></sup>` +
        `<span class="note" role="note"><span class="note__number" aria-hidden="true" data-pagefind-ignore>${number}</span> ${words}</span>`
      );
    },
  );
  if (used.size === 0) return { html: placed, end: "" };
  const items = [...used]
    .map(
      (name, index) =>
        `<li id="user-content-fn-${name}">\n${notes.get(name).replace(/Back to reference \d+/, `Back to reference ${index + 1}`)}\n</li>`,
    )
    .join("\n");
  return {
    html: placed,
    end: `<section data-footnotes data-pagefind-ignore class="footnotes" aria-label="Notes">\n<ol>\n${items}\n</ol>\n</section>`,
  };
}
