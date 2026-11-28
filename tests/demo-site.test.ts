// The demo's CSP (public-demo/_headers) allows no inline script or style and no data: URLs.
// These checks keep the built demo inside it, and keep the _headers file well formed.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build, resolveConfig } from "vite";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("..", import.meta.url));
const headersFile = fileURLToPath(new URL("../public-demo/_headers", import.meta.url));

interface DemoOutput {
  // Built HTML pages by file name: index.html (the demo) and install.html (the install guide).
  html: Map<string, string>;
  css: string[];
  // Every other emitted asset's file name, e.g. assets/BricolageGrotesque-Regular-<hash>.woff.
  files: string[];
}

async function demoOutput(): Promise<DemoOutput> {
  // Build as `pnpm build:demo` does: vitest sets NODE_ENV to "test", which would change the output.
  const nodeEnv = process.env.NODE_ENV;
  process.env.NODE_ENV = "production";
  let result: Awaited<ReturnType<typeof build>>;
  try {
    result = await build({ root, mode: "demo", logLevel: "silent", build: { write: false } });
  } finally {
    if (nodeEnv === undefined) {
      delete process.env.NODE_ENV;
    } else {
      process.env.NODE_ENV = nodeEnv;
    }
  }
  const outputs = Array.isArray(result) ? result : [result];
  const out: DemoOutput = { html: new Map(), css: [], files: [] };
  for (const output of outputs) {
    if ("output" in output) {
      for (const file of output.output) {
        if (file.type !== "asset") {
          continue;
        }
        const source =
          typeof file.source === "string" ? file.source : new TextDecoder().decode(file.source);
        if (file.fileName.endsWith(".html")) {
          out.html.set(file.fileName, source);
        } else if (file.fileName.endsWith(".css")) {
          out.css.push(source);
        } else {
          out.files.push(file.fileName);
        }
      }
    }
  }
  return out;
}

interface Rule {
  path: string;
  headers: Map<string, string>;
}

// The `_headers` format: a path line, then indented `Name: value` lines; `#` starts a comment.
function parseHeaders(text: string): Rule[] {
  const rules: Rule[] = [];
  for (const line of text.split(/\r?\n/)) {
    expect(line.length).toBeLessThanOrEqual(2000);
    if (line.trim() === "" || line.trimStart().startsWith("#")) {
      continue;
    }
    if (!/^\s/.test(line)) {
      rules.push({ path: line.trim(), headers: new Map() });
      continue;
    }
    const rule = rules.at(-1);
    const colon = line.indexOf(":");
    expect(rule, `header line before any path: ${line}`).toBeDefined();
    expect(colon, `header line without a colon: ${line}`).toBeGreaterThan(0);
    const name = line.slice(0, colon).trim().toLowerCase();
    expect(rule?.headers.has(name), `${name} set twice in one rule`).toBe(false);
    rule?.headers.set(name, line.slice(colon + 1).trim());
  }
  return rules;
}

function csp(value: string): Map<string, string> {
  const directives = new Map<string, string>();
  for (const part of value.split(";")) {
    const [name, ...sources] = part.trim().split(/\s+/);
    if (name) {
      directives.set(name, sources.join(" "));
    }
  }
  return directives;
}

describe("demo build stays inside its CSP", () => {
  it("has no inline script or style and no data: URLs", { timeout: 60_000 }, async () => {
    const { html: pages, css } = await demoOutput();
    expect([...pages.keys()].sort()).toEqual(["index.html", "install.html"]);
    expect(pages.get("index.html")).toContain('<div id="root">');
    for (const [name, html] of pages) {
      const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\b[^>]*>/gi)];
      expect(scripts.length, name).toBeGreaterThan(0);
      // Every opening tag must be one the pattern matched, so none escapes the checks below.
      expect(html.match(/<script\b/gi)?.length, name).toBe(scripts.length);
      for (const [, attributes, body] of scripts) {
        expect(attributes, name).toMatch(/\bsrc="\/assets\/[^"]+\.js"/);
        expect(body?.trim(), name).toBe("");
      }
      expect(html, name).not.toMatch(/<style\b/i);
      expect(html, name).not.toMatch(/\sstyle\s*=/i);
      expect(html, name).not.toMatch(/\son[a-z]+\s*=/i);
      expect(html, name).not.toMatch(/data:/i);
      // Stylesheets and module preloads come from the demo's own /assets/ only.
      for (const [, attributes] of html.matchAll(/<link\b([^>]*)>/gi)) {
        expect(attributes, name).toMatch(/\bhref="\/assets\/[^"]+"/);
      }
    }
    for (const sheet of css) {
      expect(sheet).not.toMatch(/url\(\s*["']?data:/i);
    }
  });

  it("self-hosts the typeface as files under /assets/ (SPEC §5.6)", {
    timeout: 60_000,
  }, async () => {
    const { css, files } = await demoOutput();
    const fonts = files.filter((name) => /\.woff2?$/.test(name));
    expect(fonts.length).toBe(2);
    for (const name of fonts) {
      expect(name).toMatch(/^assets\/BricolageGrotesque-(Regular|Bold)-[^/]+\.woff$/);
    }
    const sheet = css.join("\n");
    expect(sheet).toMatch(/font-family:\s*"?Bricolage Grotesque"?/);
    expect(sheet).toMatch(/font-display:\s*swap/);
    // No remote stylesheet pulled in by @import, and the only url()s are the two font files.
    expect(sheet).not.toMatch(/@import/i);
    const urls = [...sheet.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)].map(([, url]) => url);
    expect([...urls].sort()).toEqual(fonts.map((name) => `/${name}`).sort());
  });

  it("copies public-demo only in demo mode", async () => {
    const demo = await resolveConfig({ root, mode: "demo", logLevel: "silent" }, "build");
    expect(demo.publicDir.replaceAll("\\", "/")).toMatch(/\/public-demo$/);
    const desktop = await resolveConfig({ root, mode: "desktop", logLevel: "silent" }, "build");
    expect(desktop.publicDir).toBe("");
  });
});

describe("public-demo/_headers", () => {
  const rules = parseHeaders(readFileSync(headersFile, "utf8"));
  const all = rules.find((rule) => rule.path === "/*");
  const assets = rules.find((rule) => rule.path === "/assets/*");

  it("has a rule for every path and one for hashed assets", () => {
    expect(rules.map((rule) => rule.path)).toEqual(["/*", "/assets/*"]);
  });

  it("sets each header in one rule only, so Cloudflare never joins two values", () => {
    const names = rules.flatMap((rule) => [...rule.headers.keys()]);
    expect(new Set(names).size).toBe(names.length);
  });

  it("sends a strict CSP", () => {
    const value = all?.headers.get("content-security-policy") ?? "";
    expect(value).not.toContain("unsafe-");
    const directives = csp(value);
    expect(directives.get("default-src")).toBe("'self'");
    expect(directives.get("script-src")).toBe("'self'");
    expect(directives.get("style-src")).toBe("'self'");
    expect(directives.get("connect-src")).toBe("'self'");
    expect(directives.get("object-src")).toBe("'none'");
    expect(directives.get("base-uri")).toBe("'none'");
    expect(directives.get("form-action")).toBe("'none'");
    expect(directives.get("frame-ancestors")).toBe("'none'");
  });

  it("sends the other security headers on every path", () => {
    expect(all?.headers.get("x-content-type-options")).toBe("nosniff");
    expect(all?.headers.get("x-frame-options")).toBe("DENY");
    expect(all?.headers.get("referrer-policy")).toBe("no-referrer");
    expect(all?.headers.get("cross-origin-opener-policy")).toBe("same-origin");
    expect(all?.headers.get("strict-transport-security")).toBe(
      "max-age=31536000; includeSubDomains",
    );
    for (const feature of ["camera", "microphone", "geolocation"]) {
      expect(all?.headers.get("permissions-policy")).toContain(`${feature}=()`);
    }
  });

  it("caches only hashed assets for long", () => {
    expect(all?.headers.has("cache-control")).toBe(false);
    expect(assets?.headers.get("cache-control")).toBe("public, max-age=31536000, immutable");
  });
});
