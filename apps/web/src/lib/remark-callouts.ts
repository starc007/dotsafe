/** Turns GitHub-style alerts (`> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`) into
 * callouts: the blockquote gets `data-callout="note"` and loses the marker.
 * The .md files keep the GitHub syntax, which agents and GitHub both read. */
type Node = { type: string; value?: string; children?: Node[]; data?: { hProperties?: Record<string, string> } };

const MARKER = /^\[!(NOTE|TIP|WARNING)\]\s*/;

export function remarkCallouts() {
  return (tree: Node) => {
    const walk = (node: Node) => {
      if (node.type === 'blockquote') {
        const text = node.children?.[0]?.children?.[0];
        const match = text?.type === 'text' ? text.value?.match(MARKER) : null;
        if (text && match) {
          text.value = text.value!.slice(match[0].length);
          node.data = { hProperties: { 'data-callout': match[1].toLowerCase() } };
        }
      }
      node.children?.forEach(walk);
    };
    walk(tree);
  };
}
