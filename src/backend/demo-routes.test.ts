import { describe, expect, it } from "vitest";
import {
  type DemoGroup,
  filesOf,
  joinPath,
  keepVerb,
  kindOf,
  label,
  rejectVerb,
  routeOf,
} from "./demo-routes";

const group = (extra: Partial<DemoGroup> = {}): DemoGroup => ({
  name: "MGF_7273",
  folder: ["Barcelona"],
  primary: ".JPG",
  raw: ".NEF",
  sidecars: [
    { suffix: ".xmp", follows: "raw" },
    { suffix: ".JPG.xmp", follows: "primary" },
  ],
  cameraProtected: false,
  ...extra,
});

const options = {
  sourcePath: "E:\\2026",
  rawFollows: true,
  keepMode: "move",
  destinationPath: "D:\\Selects",
} as const;

describe("demo routes", () => {
  it("joins display folders with backslashes", () => {
    expect(joinPath("E:\\2026", ["Barcelona"])).toBe("E:\\2026\\Barcelona");
    expect(joinPath("E:\\", ["2026"])).toBe("E:\\2026");
    expect(joinPath("E:\\2026", [])).toBe("E:\\2026");
  });

  it("mirrors subfolders for kept and rejected photos", () => {
    const route = routeOf(group(), options);
    expect(route.folder).toBe("E:\\2026\\Barcelona");
    expect(route.origin).toBe("E:\\2026\\Barcelona\\MGF_7273");
    expect(route.keptTo).toBe("moved to D:\\Selects\\Barcelona");
    expect(route.rejectedTo).toBe("moved to E:\\2026\\_rejected\\Barcelona");
    expect(routeOf(group({ folder: [] }), options).rejectedTo).toBe("moved to E:\\2026\\_rejected");
  });

  it("moves the NEF and its sidecars with the JPEG when RAW follows", () => {
    const files = filesOf(group(), true);
    expect(files.going).toEqual([
      "MGF_7273.JPG",
      "MGF_7273.NEF",
      "MGF_7273.xmp",
      "MGF_7273.JPG.xmp",
    ]);
    expect(files.staying).toEqual([]);
    expect(label(group(), files.going)).toEqual({
      name: "MGF_7273",
      extensions: ".JPG .NEF .xmp .JPG.xmp",
    });
    expect(routeOf(group(), options).staying).toBeNull();
  });

  it("leaves the NEF and the sidecars that describe it when RAW does not follow (ADR 0023)", () => {
    const files = filesOf(group(), false);
    expect(files.going).toEqual(["MGF_7273.JPG", "MGF_7273.JPG.xmp"]);
    expect(files.staying).toEqual(["MGF_7273.NEF", "MGF_7273.xmp"]);
    const route = routeOf(group(), { ...options, rawFollows: false });
    expect(route.staying).toEqual({ name: "MGF_7273", extensions: ".NEF .xmp" });
  });

  it("moves a lone NEF's sidecars with it whatever the toggle", () => {
    const sidecars = [{ suffix: ".xmp", follows: "raw" }] as const;
    const lone = group({ primary: ".NEF", raw: null, sidecars });
    expect(filesOf(lone, false)).toEqual({ going: ["MGF_7273.NEF", "MGF_7273.xmp"], staying: [] });
    expect(kindOf(lone)).toBe("NEF");
    expect(kindOf(group())).toBe("JPEG + NEF");
    expect(kindOf(group({ raw: null }))).toBe("JPEG");
  });

  it("copies camera-protected photos, for keeps and rejects", () => {
    expect(keepVerb("move", group())).toBe("moved");
    expect(keepVerb("copy", group())).toBe("copied");
    expect(keepVerb("move", group({ cameraProtected: true }))).toBe("copied");
    expect(rejectVerb(group())).toBe("moved");
    expect(rejectVerb(group({ cameraProtected: true }))).toBe("copied");
  });

  it("names no verb before moved or copied is chosen, and no folder before a destination", () => {
    expect(routeOf(group(), { ...options, keepMode: null }).keptTo).toBe("D:\\Selects\\Barcelona");
    const protectedGroup = group({ cameraProtected: true });
    expect(routeOf(protectedGroup, { ...options, keepMode: null }).keptTo).toBe(
      "copied to D:\\Selects\\Barcelona",
    );
    expect(routeOf(group(), { ...options, destinationPath: null }).keptTo).toBeNull();
  });
});
