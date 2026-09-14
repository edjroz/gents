import { describe, expect, it } from "vitest";
import {
  authoritiesEqual,
  authorityForPreset,
  authoritySummary,
  presetForAuthority,
} from "../src/ui/lib/managedRuntimeAuthority";

describe("managed runtime authority presets", () => {
  const home = "/Users/A Person";

  it("maps each presentation preset to the canonical process ceiling", () => {
    expect(authorityForPreset("full-home", home, null)).toEqual({
      toolCeiling: "readwrite",
      toolRoot: home,
    });
    expect(authorityForPreset("readonly-home", home, null)).toEqual({
      toolCeiling: "readonly",
      toolRoot: home,
    });
    expect(authorityForPreset("selected-directory", home, "/tmp/a folder")).toEqual({
      toolCeiling: "readwrite",
      toolRoot: "/tmp/a folder",
    });
    expect(authorityForPreset("no-files", home, null)).toEqual({
      toolCeiling: "meta-only",
      toolRoot: null,
    });
  });

  it("does not invent a fallback for an unselected directory", () => {
    expect(authorityForPreset("selected-directory", home, null)).toBeNull();
  });

  it("projects confirmed settings back to presentation state", () => {
    expect(presetForAuthority("readonly", home, home)).toBe("readonly-home");
    expect(presetForAuthority("readwrite", "/work", home)).toBe("selected-directory");
    expect(
      authoritiesEqual(
        { toolCeiling: "readwrite", toolRoot: home },
        { toolCeiling: "readwrite", toolRoot: home },
      ),
    ).toBe(true);
    expect(authoritySummary({ toolCeiling: "readwrite", toolRoot: home })).toContain(
      "operating-system privacy controls",
    );
  });
});
