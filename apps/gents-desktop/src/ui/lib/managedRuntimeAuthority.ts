import type { ManagedServerAuthorityInput } from "@source-inc/gents-desktop-client";

export type ManagedRuntimePreset =
  "full-home" | "readonly-home" | "selected-directory" | "no-files";

export function authorityForPreset(
  preset: ManagedRuntimePreset,
  home: string,
  selectedDirectory: string | null,
): ManagedServerAuthorityInput | null {
  switch (preset) {
    case "full-home":
      return { toolCeiling: "readwrite", toolRoot: home };
    case "readonly-home":
      return { toolCeiling: "readonly", toolRoot: home };
    case "selected-directory":
      return selectedDirectory
        ? { toolCeiling: "readwrite", toolRoot: selectedDirectory }
        : null;
    case "no-files":
      return { toolCeiling: "meta-only", toolRoot: null };
  }
}

export function presetForAuthority(
  ceiling: ManagedServerAuthorityInput["toolCeiling"] | null | undefined,
  root: string | null | undefined,
  home: string,
): ManagedRuntimePreset {
  if (ceiling === "meta-only") return "no-files";
  if (ceiling === "readonly" && root === home) return "readonly-home";
  if (ceiling === "readwrite" && root !== home) return "selected-directory";
  return "full-home";
}

export function authoritySummary(authority: ManagedServerAuthorityInput): string {
  switch (authority.toolCeiling) {
    case "readwrite":
      return "The managed runtime can read and modify files under this root and run unrestricted commands as your user. Commands are not a filesystem sandbox and may reach other locations your OS account can access; operating-system privacy controls still apply.";
    case "readonly":
      return "The managed runtime can read files under this root and run its restricted read-only command set. It cannot modify files through host tools.";
    case "meta-only":
      return "The managed runtime receives no host file or shell authority. Configuration and remote services remain available where separately configured.";
  }
}

export function authoritiesEqual(
  left: ManagedServerAuthorityInput,
  right: ManagedServerAuthorityInput,
): boolean {
  return left.toolCeiling === right.toolCeiling && left.toolRoot === right.toolRoot;
}
