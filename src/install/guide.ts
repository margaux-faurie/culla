import { type Block, parseMarkdown } from "./markdown";

export type Heading = Extract<Block, { kind: "heading" }>;

/** The guide's blocks for the web: its title apart, the repository's status line left out. */
export function guideForWeb(source: string): { title: Heading | null; body: Block[] } {
  const blocks = parseMarkdown(source);
  const [first, ...rest] = blocks;
  const title = first?.kind === "heading" && first.level === 1 ? first : null;
  const body = (title === null ? blocks : rest).filter((block) => !isStatusLine(block));
  return { title, body };
}

function isStatusLine(block: Block): boolean {
  if (block.kind !== "paragraph") {
    return false;
  }
  const [first] = block.content;
  return first?.kind === "text" && first.text.startsWith("Status:");
}
