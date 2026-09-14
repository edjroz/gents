import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ManagedRuntimeAuthorityPicker } from "../src/ui/components/ManagedRuntimeAuthority";
import type { ManagedRuntimePreset } from "../src/ui/lib/managedRuntimeAuthority";

function Harness({
  validateRoot,
}: {
  validateRoot: (path: string) => Promise<string>;
}) {
  const [preset, setPreset] = useState<ManagedRuntimePreset>("full-home");
  const [directory, setDirectory] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <ManagedRuntimeAuthorityPicker
        home="/Users/A Person"
        preset={preset}
        selectedDirectory={directory}
        onPresetChange={setPreset}
        onDirectoryChange={setDirectory}
        validateRoot={validateRoot}
        error={error}
        onError={setError}
      />
      <output data-testid="selected-directory">{directory ?? "unvalidated"}</output>
    </>
  );
}

describe("ManagedRuntimeAuthorityPicker", () => {
  it("progressively reveals uncommon choices and supports keyboard selection", async () => {
    const user = userEvent.setup();
    render(<Harness validateRoot={vi.fn()} />);

    expect(screen.queryByText("No files or commands")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Customize" }));
    const noFiles = screen.getByRole("radio", { name: /No files or commands/ });
    noFiles.focus();
    await user.keyboard(" ");
    expect(noFiles).toHaveAttribute("aria-checked", "true");
  });

  it("keeps a typed path unselected until native validation canonicalizes it", async () => {
    const user = userEvent.setup();
    const validate = vi.fn(async () => "/private/tmp/a folder");
    render(<Harness validateRoot={validate} />);

    await user.click(screen.getByRole("radio", { name: /Use one folder/ }));
    const input = screen.getByLabelText("Existing directory");
    await user.type(input, "/tmp/a folder");
    await user.tab();

    await waitFor(() => expect(validate).toHaveBeenCalledWith("/tmp/a folder"));
    expect(input).toHaveValue("/private/tmp/a folder");
  });

  it("shows validation failures inline without replacing the selected path", async () => {
    const user = userEvent.setup();
    render(
      <Harness
        validateRoot={vi.fn(async () => Promise.reject(new Error("No access")))}
      />,
    );

    await user.click(screen.getByRole("radio", { name: /Use one folder/ }));
    await user.type(screen.getByLabelText("Existing directory"), "/missing");
    await user.tab();

    expect(await screen.findByText("No access")).toBeInTheDocument();
    expect(screen.getByLabelText("Existing directory")).toHaveValue("/missing");
  });

  it("ignores an older validation that finishes after the current path", async () => {
    const user = userEvent.setup();
    const resolutions = new Map<string, (canonical: string) => void>();
    const validate = vi.fn(
      (path: string) =>
        new Promise<string>((resolve) => {
          resolutions.set(path, resolve);
        }),
    );
    render(<Harness validateRoot={validate} />);

    await user.click(screen.getByRole("radio", { name: /Use one folder/ }));
    const input = screen.getByLabelText("Existing directory");
    await user.type(input, "/slow");
    await user.tab();
    expect(screen.getByTestId("selected-directory")).toHaveTextContent("unvalidated");

    await user.click(input);
    await user.clear(input);
    await user.type(input, "/current");
    await user.tab();
    resolutions.get("/current")?.("/canonical/current");
    await waitFor(() => expect(input).toHaveValue("/canonical/current"));

    resolutions.get("/slow")?.("/canonical/stale");
    await waitFor(() =>
      expect(screen.getByTestId("selected-directory")).toHaveTextContent(
        "/canonical/current",
      ),
    );
    expect(input).toHaveValue("/canonical/current");
  });
});
