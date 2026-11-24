// ADR 0014: the demo bundle never includes @tauri-apps/*. Builds both modes in memory and looks
// for the Tauri IPC global, which every @tauri-apps/api call goes through.
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { describe, expect, it } from "vitest";

const root = fileURLToPath(new URL("..", import.meta.url));
const TAURI_MARKER = "__TAURI_INTERNALS__";

async function bundledCode(mode: "demo" | "desktop"): Promise<string> {
  const result = await build({ root, mode, logLevel: "silent", build: { write: false } });
  const outputs = Array.isArray(result) ? result : [result];
  let code = "";
  for (const output of outputs) {
    if ("output" in output) {
      for (const file of output.output) {
        if (file.type === "chunk") {
          code += file.code;
        }
      }
    }
  }
  return code;
}

describe("backend selection at build time", () => {
  it("keeps @tauri-apps out of the demo bundle", { timeout: 60_000 }, async () => {
    const code = await bundledCode("demo");
    expect(code.length).toBeGreaterThan(0);
    expect(code).not.toContain(TAURI_MARKER);
  });

  // Control: proves the check above can fail.
  it("links the Tauri backend in the desktop bundle", { timeout: 60_000 }, async () => {
    expect(await bundledCode("desktop")).toContain(TAURI_MARKER);
  });
});
