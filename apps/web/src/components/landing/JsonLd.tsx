/** Structured data (schema.org JSON-LD) for search engines and agents. The
 * builders are in lib/structured-data.ts. */
export function JsonLd({ data }: { data: object }) {
  // Escapes `<` so nothing in the data can close the script tag.
  return (
    <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(data).replace(/</g, '\\u003c') }} />
  );
}
