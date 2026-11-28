import { describe, expect, it } from "vitest";
import installGuide from "../../docs/install.md?raw";
import demoCaptionsEn from "../../public-demo/videos/culla-demo.en.vtt?raw";
import demoCaptionsFr from "../../public-demo/videos/culla-demo.fr.vtt?raw";
import { guideForWeb } from "./guide";
import {
  type Block,
  DOCS_BASE,
  headingIds,
  type Inline,
  linkHrefs,
  parseInline,
  parseMarkdown,
  plainText,
  resolveHref,
  slug,
} from "./markdown";
import { isReleaseUrl, isSelfHosted, STATUS } from "./status";

/** Every piece of text in the blocks, to look for Markdown left unread. */
function allText(blocks: readonly Block[]): string[] {
  const inline = (nodes: readonly Inline[]): string[] =>
    nodes.flatMap((node) =>
      node.kind === "strong" || node.kind === "link" ? inline(node.children) : [node.text],
    );
  return blocks.flatMap((block) => {
    switch (block.kind) {
      case "heading":
      case "paragraph":
        return inline(block.content);
      case "list":
        return block.items.flatMap((item) => allText(item.blocks));
      case "table":
        return [...block.head, ...block.rows.flatMap((row) => row.cells)].flatMap((cell) =>
          inline(cell.content),
        );
      default:
        return [];
    }
  });
}

describe("parseInline", () => {
  it("reads code, bold, links, kbd and bare https URLs", () => {
    const nodes = parseInline(
      "Run `gh` with **More info**, see [the ADR](adr/0016.md), press <kbd>U</kbd> at https://example.org/a.",
    );
    expect(nodes.map((node) => node.kind)).toEqual([
      "text",
      "code",
      "text",
      "strong",
      "text",
      "link",
      "text",
      "kbd",
      "text",
      "link",
      "text",
    ]);
    expect(nodes[5]).toMatchObject({ href: `${DOCS_BASE}adr/0016.md` });
    expect(nodes[9]).toMatchObject({ href: "https://example.org/a" });
    expect(nodes[10]).toMatchObject({ text: "." });
    expect(plainText(nodes)).toBe(
      "Run gh with More info, see the ADR, press U at https://example.org/a.",
    );
  });

  it("gives siblings distinct keys", () => {
    const keys = parseInline("a `b` c **d** e").map((node) => node.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("shows a link with another scheme as plain text", () => {
    const nodes = parseInline("[click](javascript:alert(1)) and [mail](mailto:a@b.c)");
    expect(nodes.some((node) => node.kind === "link")).toBe(false);
  });
});

describe("resolveHref", () => {
  it("keeps anchors and https, sends relative links to GitHub, refuses other schemes", () => {
    expect(resolveHref("#step")).toBe("#step");
    expect(resolveHref("https://example.org/")).toBe("https://example.org/");
    expect(resolveHref("../README.md#building-from-source")).toBe(
      "https://github.com/margaux-faurie/culla/blob/main/README.md#building-from-source",
    );
    expect(resolveHref("http://example.org/")).toBeNull();
    expect(resolveHref("javascript:alert(1)")).toBeNull();
    expect(resolveHref("data:text/html,x")).toBeNull();
  });

  it("refuses backslashes and relative links that leave GitHub", () => {
    expect(resolveHref("//x")).toBeNull();
    expect(resolveHref("//evil.com/x")).toBeNull();
    expect(resolveHref("/\\evil.com/x")).toBeNull();
    expect(resolveHref("\\\\evil.com\\x")).toBeNull();
    expect(resolveHref("adr\\0016.md")).toBeNull();
    expect(resolveHref("\t//evil.com/x")).toBeNull();
    expect(resolveHref("/margaux-faurie/culla")).toBe("https://github.com/margaux-faurie/culla");
  });
});

describe("slug", () => {
  it("follows GitHub's heading anchors", () => {
    expect(slug('3. The "Windows protected your PC" screen')).toBe(
      "3-the-windows-protected-your-pc-screen",
    );
    expect(slug("2. Check the download (optional, from the first release)")).toBe(
      "2-check-the-download-optional-from-the-first-release",
    );
  });
});

describe("parseMarkdown", () => {
  it("reads headings, nested lists with code, tables and drops comments", () => {
    const blocks = parseMarkdown(
      [
        "# Title",
        "",
        "<!-- screenshot: hidden -->",
        "",
        "1. First step",
        "   - nested",
        "2. Second step:",
        "",
        "   ```powershell",
        "   Get-FileHash x",
        "   ```",
        "",
        "3. Third",
        "",
        "| A | B |",
        "|---|---|",
        "| `a` | b |",
        "",
        "Last line",
        "continues here.",
      ].join("\n"),
    );
    expect(blocks.map((block) => block.kind)).toEqual(["heading", "list", "table", "paragraph"]);
    const list = blocks[1];
    expect(list?.kind === "list" && list.ordered && list.items.length).toBe(3);
    if (list?.kind === "list") {
      expect(list.items[0]?.blocks.map((block) => block.kind)).toEqual(["paragraph", "list"]);
      expect(list.items[1]?.blocks.map((block) => block.kind)).toEqual(["paragraph", "code"]);
      expect(list.items[1]?.blocks[1]).toMatchObject({
        lang: "powershell",
        text: "Get-FileHash x",
      });
    }
    expect(blocks[3]).toMatchObject({
      content: [{ kind: "text", text: "Last line continues here." }],
    });
    expect(allText(blocks).join(" ")).not.toContain("screenshot");
  });

  it("numbers repeated headings like GitHub", () => {
    expect(headingIds(parseMarkdown("## Notes\n\n## Notes"))).toEqual(["notes", "notes-1"]);
  });
});

describe("the install guide on the web (docs/install.md)", () => {
  const guide = guideForWeb(installGuide);

  it("has its title apart and leaves out the repository's status line", () => {
    expect(guide.title?.text).toBe("Install Culla on Windows 11");
    expect(allText(guide.body).some((text) => text.startsWith("Status:"))).toBe(false);
  });

  it("keeps the pointer to building from source", () => {
    expect(linkHrefs(guide.body)).toContain(
      "https://github.com/margaux-faurie/culla/blob/main/README.md#building-from-source",
    );
  });

  it("links only to https pages or headings that exist on the page", () => {
    const ids = new Set(headingIds(guide.body));
    for (const href of linkHrefs(guide.body)) {
      if (href.startsWith("#")) {
        expect(ids, href).toContain(href.slice(1));
      } else {
        expect(href, href).toMatch(/^https:\/\//);
      }
    }
  });

  it("leaves no Markdown unread", () => {
    // Markers of bold, fences, comments, links and kbd that the reader should have consumed.
    const markers = ["**", "```", "<!--", "](", "<kbd>"];
    for (const text of allText(guide.body)) {
      for (const marker of markers) {
        expect(text, text).not.toContain(marker);
      }
    }
  });

  it("offers no installer download before the first release", () => {
    expect(STATUS.release).toBeNull();
    expect(linkHrefs(guide.body).some((href) => /\.(exe|msi)$/i.test(href))).toBe(false);
  });
});

describe("isReleaseUrl", () => {
  it("accepts only the project's GitHub Releases pages", () => {
    expect(isReleaseUrl("https://github.com/margaux-faurie/culla/releases/tag/v1.0.0")).toBe(true);
    expect(
      isReleaseUrl("https://github.com/margaux-faurie/culla/releases/download/v1.0.0/x.exe"),
    ).toBe(true);
    expect(isReleaseUrl("http://github.com/margaux-faurie/culla/releases/tag/v1.0.0")).toBe(false);
    expect(isReleaseUrl("https://github.com/someone/culla/releases/tag/v1.0.0")).toBe(false);
    expect(isReleaseUrl("https://github.com.evil.org/margaux-faurie/culla/releases/")).toBe(false);
    expect(isReleaseUrl("https://github.com/margaux-faurie/culla/releases/../../../x")).toBe(false);
    expect(isReleaseUrl("https://github.com/margaux-faurie/culla/releases\\x")).toBe(false);
    expect(isReleaseUrl("/releases/")).toBe(false);
    expect(isReleaseUrl("https://github.com@evil.org/margaux-faurie/culla/releases/")).toBe(false);
    expect(isReleaseUrl("https://github.com:x@evil.org/margaux-faurie/culla/releases/")).toBe(
      false,
    );
    expect(isReleaseUrl("https://github.com/margaux-faurie/culla/releases/%2e%2e/%2e%2e/x")).toBe(
      false,
    );
  });

  it("holds for the release in STATUS, if any", () => {
    if (STATUS.release !== null) {
      expect(isReleaseUrl(STATUS.release.url)).toBe(true);
    }
  });
});

describe("isSelfHosted", () => {
  it("accepts only paths on the demo site itself", () => {
    expect(isSelfHosted("/videos/culla-demo.mp4")).toBe(true);
    expect(isSelfHosted("//cdn.example.org/v.mp4")).toBe(false);
    expect(isSelfHosted("https://www.youtube.com/embed/x")).toBe(false);
    expect(isSelfHosted("videos/v.mp4")).toBe(false);
    expect(isSelfHosted("//x")).toBe(false);
    expect(isSelfHosted("/\\evil.com/x.mp4")).toBe(false);
    expect(isSelfHosted("/videos\\v.mp4")).toBe(false);
    expect(isSelfHosted("/\t/evil.com/x.mp4")).toBe(false);
    expect(isSelfHosted("/videos/a b.mp4")).toBe(true);
  });

  it("holds for every video the page lists", () => {
    for (const video of [STATUS.demoVideo, STATUS.installVideo]) {
      if (video !== null) {
        expect(isSelfHosted(video.src)).toBe(true);
        expect(isSelfHosted(video.poster)).toBe(true);
        for (const track of video.captions) {
          expect(isSelfHosted(track.src)).toBe(true);
        }
      }
    }
  });
});

interface Cue {
  readonly start: number;
  readonly end: number;
  readonly lines: readonly string[];
}

/** The cues of a WebVTT file as written in public-demo/videos/: a timing line, then text lines. */
function cues(vtt: string): Cue[] {
  const seconds = (stamp: string): number => {
    const [h, m, s] = stamp.split(":").map(Number);
    return (h ?? 0) * 3600 + (m ?? 0) * 60 + (s ?? 0);
  };
  return vtt
    .replace(/\r\n/g, "\n")
    .split(/\n{2,}/)
    .map((block) => block.trim().split("\n"))
    .filter(([timing]) => timing?.includes("-->"))
    .map(([timing = "", ...lines]) => {
      const [start = "", end = ""] = timing.split(" --> ");
      return { start: seconds(start), end: seconds(end), lines };
    });
}

describe("demo video text", () => {
  const video = STATUS.demoVideo;

  it("has a transcript that matches its English captions line by line", () => {
    expect(video).not.toBeNull();
    expect(video?.transcript).toEqual(cues(demoCaptionsEn).flatMap((cue) => cue.lines));
  });

  it("keeps each caption on screen for at least 1.5 s, at the same times in both languages", () => {
    const en = cues(demoCaptionsEn);
    const fr = cues(demoCaptionsFr);
    expect(en.length).toBeGreaterThan(0);
    for (const cue of [...en, ...fr]) {
      expect(cue.end - cue.start, cue.lines.join(" ")).toBeGreaterThanOrEqual(1.5);
    }
    expect(fr.map(({ start, end }) => [start, end])).toEqual(
      en.map(({ start, end }) => [start, end]),
    );
  });
});
