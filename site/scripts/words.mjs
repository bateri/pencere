// A sentence of a source file's comment, as the HTML the pages show: the words of the Markdown that
// the catalogue (scripts/catalog.mjs) takes from the code. Apart from it so that a test can give it
// words made up.

/** Words with `code` in them, as HTML; a Markdown link is its words, and what holds them is a card, a link already. */
export function html(words) {
  const escape = (text) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  // A link is taken out of the whole text before it is cut at the code, outside a span of code that
  // has the shape of one: a link's words may be code (``[`pencere.span`](#rich-text)``).
  return words
    .replace(/(`[^`]+`)|\[([^\]]+)\]\([^)]*\)/g, (whole, code, label) => code ?? label)
    .split(/(`[^`]+`)/)
    .map((part) =>
      part.startsWith("`") ? `<code>${escape(part.slice(1, -1))}</code>` : escape(part),
    )
    .join("");
}
