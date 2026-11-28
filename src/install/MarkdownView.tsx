import { createElement } from "react";
import type { Block, Inline } from "./markdown";

/** Renders blocks from `parseMarkdown` as React elements: text only, never injected HTML. */
export function MarkdownBlocks({ blocks }: { readonly blocks: readonly Block[] }) {
  return blocks.map((block) => <MarkdownBlock key={block.key} block={block} />);
}

function MarkdownBlock({ block }: { readonly block: Block }) {
  switch (block.kind) {
    case "heading":
      return createElement(
        `h${Math.min(Math.max(block.level, 1), 6)}`,
        { id: block.id },
        <InlineNodes nodes={block.content} />,
      );
    case "paragraph":
      return (
        <p>
          <InlineNodes nodes={block.content} />
        </p>
      );
    case "code":
      return (
        <pre className="doc-code">
          <code>{block.text}</code>
        </pre>
      );
    case "list": {
      const items = block.items.map((item) => {
        const [only] = item.blocks;
        const tight = item.blocks.length === 1 && only?.kind === "paragraph";
        return (
          <li key={item.key}>
            {tight ? <InlineNodes nodes={only.content} /> : <MarkdownBlocks blocks={item.blocks} />}
          </li>
        );
      });
      return block.ordered ? <ol start={block.start}>{items}</ol> : <ul>{items}</ul>;
    }
    case "table":
      return (
        <div className="doc-table">
          <table>
            <thead>
              <tr>
                {block.head.map((cell) => (
                  <th key={cell.key} scope="col">
                    <InlineNodes nodes={cell.content} />
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row) => (
                <tr key={row.key}>
                  {row.cells.map((cell) => (
                    <td key={cell.key}>
                      <InlineNodes nodes={cell.content} />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
  }
}

function InlineNodes({ nodes }: { readonly nodes: readonly Inline[] }) {
  return nodes.map((node) => {
    const key = node.key;
    switch (node.kind) {
      case "text":
        return <span key={key}>{node.text}</span>;
      case "code":
        return <code key={key}>{node.text}</code>;
      case "kbd":
        return <kbd key={key}>{node.text}</kbd>;
      case "strong":
        return (
          <strong key={key}>
            <InlineNodes nodes={node.children} />
          </strong>
        );
      default:
        return (
          <a key={key} href={node.href}>
            <InlineNodes nodes={node.children} />
          </a>
        );
    }
  });
}
