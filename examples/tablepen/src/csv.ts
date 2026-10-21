/**
 * CSV as RFC 4180 writes it: fields separated by commas, a field in double quotes
 * when it holds a comma, a quote or a line break, and a quote inside one doubled.
 * Lines end in LF or CRLF. Nothing here guesses another separator.
 */

/** The records of `text`, each a list of fields. A last empty line is no record. */
export function parse(text: string): string[][] {
  const records: string[][] = [];
  let record: string[] = [];
  let field = "";
  let quoted = false;
  let at = 0;
  const endField = () => {
    record.push(field);
    field = "";
  };
  const endRecord = () => {
    endField();
    records.push(record);
    record = [];
  };
  while (at < text.length) {
    const char = text[at];
    if (quoted) {
      if (char === '"' && text[at + 1] === '"') {
        field += '"';
        at += 2;
        continue;
      }
      if (char === '"') quoted = false;
      else field += char;
      at += 1;
      continue;
    }
    if (char === '"' && field === "") quoted = true;
    else if (char === ",") endField();
    else if (char === "\n") endRecord();
    else if (char === "\r" && text[at + 1] === "\n") {
      endRecord();
      at += 1;
    } else field += char;
    at += 1;
  }
  if (field !== "" || record.length > 0 || quoted) endRecord();
  return records;
}

/** A field as CSV writes it: in quotes only when it has to be. */
function field(value: string): string {
  return /[",\r\n]|^\s|\s$/.test(value) ? `"${value.replaceAll('"', '""')}"` : value;
}

/** `records` as CSV text, each line ended with LF. */
export function format(records: readonly (readonly string[])[]): string {
  return records.map((record) => `${record.map(field).join(",")}\n`).join("");
}
