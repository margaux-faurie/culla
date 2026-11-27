// Golden decision-log fixtures shared with the Rust `fold` (docs/testing.md §3.3, US-27): every
// `testdata/event-log/<case>.jsonl` folded by the TS reducer must give `<case>.expected.json`.
// So far the only case is `empty`; the Rust side writes the others in M2 (US-10). Their event JSON
// is read by `parseEvent`, whose shape is provisional until the Rust types are exported (ADR 0019).
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parseEvent } from "../src/decisions/events";
import { fold, toExpected } from "../src/decisions/log";

const dir = fileURLToPath(new URL("../testdata/event-log/", import.meta.url));
const cases = readdirSync(dir)
  .filter((name) => name.endsWith(".jsonl"))
  .map((name) => name.slice(0, -".jsonl".length))
  .sort();

describe("golden decision logs", () => {
  it("has at least the empty case", () => {
    expect(cases).toContain("empty");
  });

  it.each(cases)("%s folds to its expected state", (name) => {
    const lines = readFileSync(`${dir}${name}.jsonl`, "utf8")
      .split(/\r?\n/)
      .filter((line) => line.trim() !== "");
    const events = lines.map((line) => parseEvent(JSON.parse(line)));
    const expected: unknown = JSON.parse(readFileSync(`${dir}${name}.expected.json`, "utf8"));
    const result = fold(events);
    expect(result.corrupt).toBeUndefined();
    expect(toExpected(result.folded)).toEqual(expected);
  });
});
