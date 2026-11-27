// A small Markdown reader for docs/install.md, so the web install page and the repository guide
// share one source (SPEC §2, "Videos and install guide"). It reads only the subset that file
// uses: headings, paragraphs, lists (nested by indentation), fenced code, pipe tables and HTML
// comments (dropped), with inline code, bold, links, bare https URLs and <kbd>. Anything else
// stays plain text. The output is data rendered by React as text; no HTML from the file is ever
// injected into the page.

type InlineNode =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "code"; readonly text: string }
  | { readonly kind: "kbd"; readonly text: string }
  | { readonly kind: "strong"; readonly children: readonly Inline[] }
  | { readonly kind: "link"; readonly href: string; readonly children: readonly Inline[] };

/** An inline node; `key` is unique among its siblings (React keys). */
export type Inline = InlineNode & { readonly key: string };

export interface Cell {
  readonly key: string;
  readonly content: readonly Inline[];
}

export interface Row {
  readonly key: string;
  readonly cells: readonly Cell[];
}

export interface ListItem {
  readonly key: string;
  readonly blocks: readonly Block[];
}

export type Block =
  | {
      readonly kind: "heading";
      readonly key: string;
      readonly level: number;
      readonly id: string;
      readonly text: string;
      readonly content: readonly Inline[];
    }
  | { readonly kind: "paragraph"; readonly key: string; readonly content: readonly Inline[] }
  | { readonly kind: "code"; readonly key: string; readonly lang: string; readonly text: string }
  | {
      readonly kind: "list";
      readonly key: string;
      readonly ordered: boolean;
      readonly start: number;
      readonly items: readonly ListItem[];
    }
  | {
      readonly kind: "table";
      readonly key: string;
      readonly head: readonly Cell[];
      readonly rows: readonly Row[];
    };

/** Where relative links in docs/ point on the web page: the file on GitHub. */
export const DOCS_BASE = "https://github.com/margaux-faurie/culla/blob/main/docs/";

const HEADING = /^(#{1,6}) +(.*)$/;
const FENCE = /^```(.*)$/;
const LIST_MARKER = /^([-*]|\d+\.) +(.*)$/;
const TABLE_SEPARATOR = /^\|?(\s*:?-+:?\s*\|)+\s*:?-*:?\s*\|?$/;

/** Reads a Markdown document into blocks. Heading ids follow GitHub's anchors. */
export function parseMarkdown(source: string): Block[] {
  const slugs = new Map<string, number>();
  return parseBlocks(source.replaceAll("\r\n", "\n").split("\n"), "b", slugs);
}

function startsBlock(line: string): boolean {
  return (
    HEADING.test(line) ||
    FENCE.test(line) ||
    LIST_MARKER.test(line) ||
    line.startsWith("|") ||
    line.startsWith("<!--")
  );
}

const indentOf = (line: string): number => line.length - line.trimStart().length;

function parseBlocks(lines: readonly string[], prefix: string, slugs: Map<string, number>) {
  const blocks: Block[] = [];
  let i = 0;
  const key = () => `${prefix}${blocks.length}`;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    if (line.trim() === "") {
      i += 1;
      continue;
    }
    if (line.startsWith("<!--")) {
      while (i < lines.length && !(lines[i] ?? "").includes("-->")) {
        i += 1;
      }
      i += 1;
      continue;
    }
    const fence = FENCE.exec(line);
    if (fence !== null) {
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !(lines[i] ?? "").startsWith("```")) {
        body.push(lines[i] ?? "");
        i += 1;
      }
      i += 1;
      blocks.push({
        kind: "code",
        key: key(),
        lang: (fence[1] ?? "").trim(),
        text: body.join("\n"),
      });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading !== null) {
      const raw = (heading[2] ?? "").trim();
      const content = parseInline(raw);
      const text = plainText(content);
      blocks.push({
        kind: "heading",
        key: key(),
        level: (heading[1] ?? "#").length,
        id: uniqueSlug(text, slugs),
        text,
        content,
      });
      i += 1;
      continue;
    }
    if (line.startsWith("|")) {
      const rows: string[] = [];
      while (i < lines.length && (lines[i] ?? "").startsWith("|")) {
        rows.push(lines[i] ?? "");
        i += 1;
      }
      blocks.push(parseTable(rows, key()));
      continue;
    }
    if (LIST_MARKER.test(line)) {
      const listKey = key();
      const [list, next] = parseList(lines, i, listKey, slugs);
      blocks.push(list);
      i = next;
      continue;
    }
    const text: string[] = [];
    while (i < lines.length) {
      const current = lines[i] ?? "";
      if (current.trim() === "" || (text.length > 0 && startsBlock(current))) {
        break;
      }
      text.push(current.trim());
      i += 1;
    }
    blocks.push({ kind: "paragraph", key: key(), content: parseInline(text.join(" ")) });
  }
  return blocks;
}

function parseList(
  lines: readonly string[],
  from: number,
  listKey: string,
  slugs: Map<string, number>,
): [Block, number] {
  const first = LIST_MARKER.exec(lines[from] ?? "");
  const firstMarker = first?.[1] ?? "-";
  const ordered = /^\d/.test(firstMarker);
  const items: ListItem[] = [];
  let i = from;
  while (i < lines.length) {
    const match = LIST_MARKER.exec(lines[i] ?? "");
    if (match === null || /^\d/.test(match[1] ?? "") !== ordered) {
      break;
    }
    const width = (match[1] ?? "").length + 1;
    const body = [match[2] ?? ""];
    i += 1;
    while (i < lines.length) {
      const current = lines[i] ?? "";
      if (current.trim() === "") {
        // A blank line stays in the item only if the item goes on, indented, after it.
        let j = i;
        while (j < lines.length && (lines[j] ?? "").trim() === "") {
          j += 1;
        }
        if (j < lines.length && indentOf(lines[j] ?? "") >= width) {
          body.push("");
          i += 1;
          continue;
        }
        break;
      }
      if (indentOf(current) >= width) {
        body.push(current.slice(width));
        i += 1;
        continue;
      }
      if (!startsBlock(current) && indentOf(current) > 0) {
        body.push(current.trim());
        i += 1;
        continue;
      }
      break;
    }
    const itemKey = `${listKey}.${items.length}`;
    items.push({ key: itemKey, blocks: parseBlocks(body, `${itemKey}.`, slugs) });
    // Items of one list may be separated by blank lines.
    let j = i;
    while (j < lines.length && (lines[j] ?? "").trim() === "") {
      j += 1;
    }
    const again = LIST_MARKER.exec(lines[j] ?? "");
    if (again === null || /^\d/.test(again[1] ?? "") !== ordered) {
      break;
    }
    i = j;
  }
  const start = ordered ? Number.parseInt(firstMarker, 10) : 1;
  return [{ kind: "list", key: listKey, ordered, start, items }, i];
}

function splitRow(row: string): string[] {
  let inner = row.trim();
  if (inner.startsWith("|")) {
    inner = inner.slice(1);
  }
  if (inner.endsWith("|")) {
    inner = inner.slice(0, -1);
  }
  return inner.split("|").map((cell) => cell.trim());
}

const cells = (row: string, key: string): Cell[] =>
  splitRow(row).map((cell, at) => ({ key: `${key}.${String(at)}`, content: parseInline(cell) }));

function parseTable(rows: readonly string[], key: string): Block {
  const [headRow = "", separator = "", ...body] = rows;
  if (!TABLE_SEPARATOR.test(separator)) {
    // Not a table after all: keep the lines as one paragraph.
    return { kind: "paragraph", key, content: parseInline(rows.join(" ")) };
  }
  return {
    kind: "table",
    key,
    head: cells(headRow, `${key}.h`),
    rows: body.map((row, at) => {
      const rowKey = `${key}.r${String(at)}`;
      return { key: rowKey, cells: cells(row, rowKey) };
    }),
  };
}

const INLINE =
  /`([^`]+)`|\*\*(.+?)\*\*|\[([^\]]+)\]\(([^)\s]+)\)|<kbd>([^<]+)<\/kbd>|(https:\/\/[^\s<>()]+)/g;

/** Reads inline Markdown. Links get their href resolved for the web page (`resolveHref`). */
export function parseInline(source: string): Inline[] {
  const out: InlineNode[] = [];
  let last = 0;
  const push = (text: string) => {
    if (text !== "") {
      out.push({ kind: "text", text });
    }
  };
  for (const match of source.matchAll(INLINE)) {
    const at = match.index;
    let whole = match[0];
    const [, code, strong, linkText, linkHref, kbd] = match;
    let bare = match[6];
    if (bare !== undefined) {
      // A sentence's full stop or comma after a bare URL is not part of it.
      const trimmed = bare.replace(/[.,;:]+$/, "");
      whole = trimmed;
      bare = trimmed;
    }
    push(source.slice(last, at));
    last = at + whole.length;
    if (code !== undefined) {
      out.push({ kind: "code", text: code });
    } else if (strong !== undefined) {
      out.push({ kind: "strong", children: parseInline(strong) });
    } else if (linkText !== undefined && linkHref !== undefined) {
      const children = parseInline(linkText);
      const href = resolveHref(linkHref);
      if (href === null) {
        out.push(...children);
      } else {
        out.push({ kind: "link", href, children });
      }
    } else if (kbd !== undefined) {
      out.push({ kind: "kbd", text: kbd });
    } else if (bare !== undefined) {
      out.push({ kind: "link", href: bare, children: [{ kind: "text", text: bare, key: "i0" }] });
    }
  }
  push(source.slice(last));
  return out.map((node, at) => ({ ...node, key: `i${String(at)}` }));
}

/**
 * Where a link from docs/install.md points on the web page: in-page anchors stay, https links
 * stay, relative links go to the file on GitHub. Anything else (another scheme, a backslash,
 * which browsers read as a slash, or a relative link that leaves GitHub, such as `//x`) is
 * refused and shown as plain text.
 */
export function resolveHref(href: string): string | null {
  if (href.includes("\\")) {
    return null;
  }
  if (href.startsWith("#")) {
    return href;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
    return href.startsWith("https://") ? href : null;
  }
  try {
    const url = new URL(href, DOCS_BASE);
    return url.origin === new URL(DOCS_BASE).origin ? url.href : null;
  } catch {
    return null;
  }
}

export function plainText(content: readonly Inline[]): string {
  return content
    .map((node) =>
      node.kind === "strong" || node.kind === "link" ? plainText(node.children) : node.text,
    )
    .join("");
}

/** GitHub's heading anchor: lower case, punctuation dropped, spaces to hyphens. */
export function slug(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, "")
    .replace(/\s/g, "-");
}

function uniqueSlug(text: string, seen: Map<string, number>): string {
  const base = slug(text);
  const count = seen.get(base) ?? 0;
  seen.set(base, count + 1);
  return count === 0 ? base : `${base}-${count}`;
}

/** Every heading id in the blocks, nested lists included. */
export function headingIds(blocks: readonly Block[]): string[] {
  return blocks.flatMap((block) => {
    if (block.kind === "heading") {
      return [block.id];
    }
    if (block.kind === "list") {
      return block.items.flatMap((item) => headingIds(item.blocks));
    }
    return [];
  });
}

/** Every link href in the blocks, in document order. */
export function linkHrefs(blocks: readonly Block[]): string[] {
  const fromInline = (content: readonly Inline[]): string[] =>
    content.flatMap((node) => {
      if (node.kind === "link") {
        return [node.href, ...fromInline(node.children)];
      }
      return node.kind === "strong" ? fromInline(node.children) : [];
    });
  return blocks.flatMap((block) => {
    switch (block.kind) {
      case "heading":
      case "paragraph":
        return fromInline(block.content);
      case "list":
        return block.items.flatMap((item) => linkHrefs(item.blocks));
      case "table":
        return [...block.head, ...block.rows.flatMap((row) => row.cells)].flatMap((cell) =>
          fromInline(cell.content),
        );
      default:
        return [];
    }
  });
}
