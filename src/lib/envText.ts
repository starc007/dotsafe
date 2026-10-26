// Recognizing pasted .env text in the add-variable form. Only decides how to
// handle a paste; the actual parsing for import happens in Rust.

const ASSIGNMENT = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=(.*)$/;

/** How many lines look like `KEY=value` (comments and blank lines ignored). */
export function countAssignments(text: string): number {
  return text.split(/\r?\n/).filter((line) => ASSIGNMENT.test(line)).length;
}

/** Splits a single pasted `KEY=value` line, dropping `export` and quotes. */
export function splitAssignment(text: string): { key: string; value: string } | null {
  const trimmed = text.trim();
  if (trimmed.includes("\n")) return null;
  const match = trimmed.match(ASSIGNMENT);
  if (!match) return null;
  let value = match[2].trim();
  const quote = value[0];
  if ((quote === '"' || quote === "'") && value.length > 1 && value.endsWith(quote)) {
    value = value.slice(1, -1);
  }
  return { key: match[1], value };
}
