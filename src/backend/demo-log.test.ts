import { describe, expect, it } from "vitest";
import { escaped, type LogHeader, type LogOp, logTime, renderLog } from "./demo-log";

const header: LogHeader = {
  version: "0.1.0",
  plan: 12,
  purpose: "apply",
  run: "first",
  started: "2026-11-28 09:14:03 +01:00",
  source: "E:\\DCIM\\100NIKON",
  destination: "D:\\Photos\\2026",
};

const op = (
  id: number,
  group: number,
  kind: LogOp["kind"],
  root: LogOp["root"],
  src: string[],
  dst: string[],
  rest: Partial<LogOp> = {},
): LogOp => ({
  id,
  group,
  action: root === "rejected" ? "reject" : "keep",
  kind,
  src,
  dst,
  root,
  state: "pending",
  reason: null,
  code: null,
  attempts: 0,
  hash: null,
  ...rest,
});

describe("demo apply log (the core's applylog::render)", () => {
  it("escapes names as the core does (threat model T6b)", () => {
    const log = renderLog(
      { ...header, source: "E:\\\u202eMICD", destination: null },
      [
        op(1, 1, "rename", "rejected", ["dir", "u{a}x"], ["dir\nx"]),
        op(2, 2, "rename", "rejected", ["d\u202dir", 'x"'], ['"y']),
      ],
      { end: { kind: "cancelled" }, ended: "09:01" },
    );
    expect(log).toContain('source "dir\\u\\u{7b}a}x" → rejected "dir\\u{a}x"');
    expect(log).toContain('source "d\\u{202d}ir\\x\\u{22}" → rejected "\\u{22}y"');
    expect(log).toContain("source       E:\\\\u{202e}MICD\n");
    expect(log).toContain("destination  none\n");
    expect(log).toContain("end          cancelled\n");
    expect(log).not.toContain("alarms");
  });

  it("escapes a backslash and a brace inside a name, and a brace in a display string", () => {
    const log = renderLog(
      { ...header, source: "E:\\{x}\\DCIM", destination: "D:\\a{b" },
      [op(1, 1, "rename", "rejected", ["a\\b", "c{d}"], ["e"])],
      { end: { kind: "cancelled" }, ended: "09:01" },
    );
    expect(log).toContain('source "a\\u{5c}b\\c\\u{7b}d}" → rejected "e"');
    expect(log).toContain("source       E:\\\\u{7b}x}\\DCIM\n");
    expect(log).toContain("destination  D:\\a\\u{7b}b\n");
  });

  it("escapes C1 controls, line and paragraph separators and the byte order mark", () => {
    expect(escaped("~\u007f\u0080\u009f\u00a0", () => false)).toBe("~\\u{7f}\\u{80}\\u{9f}\u00a0");
    expect(escaped("a\u2028b\u2029c", () => false)).toBe("a\\u{2028}b\\u{2029}c");
    expect(escaped("\ufeffDSC.JPG", () => false)).toBe("\\u{feff}DSC.JPG");
    const log = renderLog(
      { ...header, destination: null },
      [op(1, 1, "rename", "rejected", ["\u0085x\u2028"], ["\ufeffy"])],
      { end: { kind: "cancelled" }, ended: "09:01" },
    );
    expect(log).toContain('source "\\u{85}x\\u{2028}" → rejected "\\u{feff}y"');
  });

  it("escapes control, bidi and invisible characters, and only those", () => {
    expect(escaped("a\u001b[31mb", () => false)).toBe("a\\u{1b}[31mb");
    expect(escaped("é 📷 ok", () => false)).toBe("é 📷 ok");
    expect(escaped("\u{e0041}", () => false)).toBe("\\u{e0041}");
  });

  it("formats the time as the adapter does, with the offset", () => {
    expect(logTime(new Date(2026, 10, 28, 9, 4, 3))).toMatch(/^2026-11-28 09:04:03 [+-]\d\d:\d\d$/);
  });
});
