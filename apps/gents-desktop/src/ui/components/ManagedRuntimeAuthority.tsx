import { useEffect, useRef, useState } from "react";
import { Circle, CircleCheck, FolderOpen } from "lucide-react";
import { Button } from "@gents/ui/components/button";
import { Input } from "@gents/ui/components/input";
import { cn } from "@gents/ui/lib/utils";
import type { ManagedServerAuthorityInput } from "@source-inc/gents-desktop-client";
import {
  authorityForPreset,
  authoritySummary,
  type ManagedRuntimePreset,
} from "@/lib/managedRuntimeAuthority";

const CHOICES: Array<{
  preset: ManagedRuntimePreset;
  title: string;
  hint: string;
  uncommon?: boolean;
}> = [
  {
    preset: "full-home",
    title: "Work across my home folder · Recommended",
    hint: "Read and change files, and run commands as me.",
  },
  {
    preset: "selected-directory",
    title: "Use one folder as the tool root",
    hint: "File tools start there; commands still run as me.",
  },
  {
    preset: "readonly-home",
    title: "Read my home folder",
    hint: "Read files and run a restricted read-only command set.",
    uncommon: true,
  },
  {
    preset: "no-files",
    title: "No files or commands",
    hint: "Only configuration and separately configured remote services.",
    uncommon: true,
  },
];

export function ManagedRuntimeAuthorityPicker({
  home,
  preset,
  selectedDirectory,
  onPresetChange,
  onDirectoryChange,
  validateRoot,
  error,
  onError,
}: {
  home: string;
  preset: ManagedRuntimePreset;
  selectedDirectory: string | null;
  onPresetChange: (preset: ManagedRuntimePreset) => void;
  onDirectoryChange: (path: string | null) => void;
  validateRoot?: (path: string) => Promise<string>;
  error?: string | null;
  onError: (error: string | null) => void;
}) {
  const [customize, setCustomize] = useState(
    preset === "readonly-home" || preset === "no-files",
  );
  const [validating, setValidating] = useState(false);
  const [directoryDraft, setDirectoryDraft] = useState(selectedDirectory ?? "");
  const input = useRef<HTMLInputElement>(null);
  const validationGeneration = useRef(0);

  useEffect(() => {
    if (selectedDirectory !== null) setDirectoryDraft(selectedDirectory);
  }, [selectedDirectory]);

  useEffect(
    () => () => {
      validationGeneration.current += 1;
    },
    [],
  );

  const validate = async (path: string) => {
    const generation = ++validationGeneration.current;
    if (!validateRoot) {
      onError("Directory selection is unavailable on this host.");
      return;
    }
    setDirectoryDraft(path);
    onDirectoryChange(null);
    setValidating(true);
    onError(null);
    try {
      const canonical = await validateRoot(path);
      if (generation !== validationGeneration.current) return;
      setDirectoryDraft(canonical);
      onDirectoryChange(canonical);
    } catch (cause) {
      if (generation !== validationGeneration.current) return;
      onError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (generation === validationGeneration.current) setValidating(false);
    }
  };

  const choose = async () => {
    if (!("__TAURI_INTERNALS__" in window)) {
      input.current?.focus();
      return;
    }
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: selectedDirectory ?? home,
        title: "Choose the managed runtime tool root",
      });
      if (typeof selected === "string") await validate(selected);
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <div className="grid gap-3" role="radiogroup" aria-label="Managed runtime access">
      {CHOICES.filter((choice) => customize || !choice.uncommon).map((choice) => (
        <button
          key={choice.preset}
          type="button"
          role="radio"
          aria-checked={preset === choice.preset}
          onClick={() => {
            validationGeneration.current += 1;
            setValidating(false);
            onError(null);
            onPresetChange(choice.preset);
          }}
          className={cn(
            "flex w-full items-start gap-3 rounded-2xl border bg-raised px-4 py-3.5 text-left",
            preset === choice.preset
              ? "border-brand ring-1 ring-brand"
              : "border-border/60 hover:bg-accent",
          )}
        >
          {preset === choice.preset ? (
            <CircleCheck className="mt-0.5 size-4 shrink-0" />
          ) : (
            <Circle className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          )}
          <span>
            <span className="block text-sm font-medium">{choice.title}</span>
            <span className="block text-xs text-muted-foreground">{choice.hint}</span>
          </span>
        </button>
      ))}
      {!customize ? (
        <button
          type="button"
          className="justify-self-start text-sm text-muted-foreground underline"
          onClick={() => setCustomize(true)}
        >
          Customize
        </button>
      ) : null}
      {preset === "selected-directory" ? (
        <div className="grid gap-2 rounded-2xl border border-border/60 bg-raised p-4">
          <label className="text-xs text-muted-foreground" htmlFor="managed-tool-root">
            Existing directory
          </label>
          <div className="flex gap-2">
            <Input
              ref={input}
              id="managed-tool-root"
              className="font-mono"
              value={directoryDraft}
              onChange={(event) => {
                validationGeneration.current += 1;
                setValidating(false);
                setDirectoryDraft(event.target.value);
                onDirectoryChange(null);
                onError(null);
              }}
              onBlur={(event) => {
                if (event.target.value.trim()) void validate(event.target.value.trim());
              }}
              placeholder="Choose a folder"
              aria-invalid={Boolean(error)}
            />
            <Button type="button" variant="outline" onClick={() => void choose()}>
              <FolderOpen /> Choose
            </Button>
          </div>
          {validating ? (
            <p className="text-xs text-muted-foreground">Checking…</p>
          ) : null}
        </div>
      ) : null}
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

export function ManagedRuntimeAuthorityReview({
  authority,
}: {
  authority: ManagedServerAuthorityInput;
}) {
  return (
    <div className="grid gap-3 rounded-2xl border border-border/60 bg-raised p-4">
      <div>
        <p className="text-xs text-muted-foreground">Tool root</p>
        <p className="break-all font-mono text-sm">
          {authority.toolRoot ?? "No host path"}
        </p>
      </div>
      <div>
        <p className="text-xs text-muted-foreground">Process authority</p>
        <p className="text-sm">{authoritySummary(authority)}</p>
      </div>
      <p className="text-xs text-muted-foreground">
        Setup remains a narrow configurator. A behavior can reduce this ceiling but
        cannot expand it.
      </p>
    </div>
  );
}

export { authorityForPreset };
