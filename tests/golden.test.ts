// Golden decision-log fixtures shared with the Rust `fold` (docs/testing.md §3.3, INV-15, US-27):
// every `testdata/event-log/<case>.jsonl` is parsed and folded by the TS reducer, and the fold's
// text must equal `<case>.expected.json` byte for byte, as in `crates/core/tests/golden.rs`. The
// cases are read from the directory, so a new one cannot be skipped here; a case without its
// expected file, or an expected file without its log, fails.
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parseLog, renderFold, renderLog } from "../src/decisions/codec";
import { fold, toExpected } from "../src/decisions/log";

const dir = fileURLToPath(new URL("../testdata/event-log/", import.meta.url));
const files = readdirSync(dir);
const LOG = ".jsonl";
const EXPECTED = ".expected.json";
const names = (suffix: string) =>
  files
    .filter((name) => name.endsWith(suffix))
    .map((name) => name.slice(0, -suffix.length))
    .sort();
const cases = names(LOG);

describe("golden decision logs", () => {
  it("has the cases, each with its expected file and nothing else", () => {
    expect(cases).toContain("empty");
    expect(cases.length).toBeGreaterThan(1);
    expect(cases.filter((name) => !files.includes(`${name}${EXPECTED}`))).toEqual([]);
    expect(names(EXPECTED).filter((name) => !cases.includes(name))).toEqual([]);
    expect(files.filter((f) => !f.endsWith(LOG) && !f.endsWith(EXPECTED))).toEqual([]);
  });

  it.each(cases)("%s folds to its expected state", (name) => {
    const log = readFileSync(`${dir}${name}${LOG}`, "utf8");
    const expectedText = readFileSync(`${dir}${name}${EXPECTED}`, "utf8");
    const events = parseLog(log);
    // The text form is canonical: rendering the parsed log gives the file back.
    expect(renderLog(events)).toBe(log);
    const result = fold(events);
    // Structured first, for a readable diff; then the exact text.
    expect(toExpected(result)).toEqual(JSON.parse(expectedText));
    expect(renderFold({ ...result.folded, corrupt: result.corrupt })).toBe(expectedText);
  });

  it.each(cases)("every prefix of %s before any bad event folds without error", (name) => {
    const events = parseLog(readFileSync(`${dir}${name}${LOG}`, "utf8"));
    const bad = fold(events).corrupt;
    const valid = bad === undefined ? events.length : events.findIndex((e) => e.seq === bad.seq);
    for (let n = 0; n <= valid; n += 1) {
      expect(fold(events.slice(0, n)).corrupt).toBeUndefined();
    }
  });
});
