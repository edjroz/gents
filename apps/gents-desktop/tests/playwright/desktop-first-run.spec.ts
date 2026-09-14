import {
  composer,
  expect,
  gotoHarness,
  openConfig,
  sendButton,
  test,
} from "./desktopTest";

test.describe("first-run install", () => {
  test("creates a local agent, adds inference, and starts a conversation", async ({
    page,
  }) => {
    test.setTimeout(60_000);
    await gotoHarness(page, "empty-fleet");
    await expect(page.getByTestId("setup-screen")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Let’s get set up" })).toBeVisible();

    await page.getByTestId("setup-next").click();
    await expect(
      page.getByRole("heading", { name: "Configure your agent" }),
    ).toBeVisible();
    await expect(page.getByRole("textbox", { name: "Agent name" })).toHaveValue(
      "Forge",
    );

    await page.getByTestId("setup-next").click();
    await expect(
      page.getByRole("heading", {
        name: "What can the hosted agent do on this computer?",
      }),
    ).toBeVisible();
    await expect(
      page.getByRole("radio", { name: /Work across my home folder/ }),
    ).toBeChecked();

    await page.getByTestId("setup-next").click();
    await expect(page.getByRole("heading", { name: "Review access" })).toBeVisible();
    await expect(page.getByText("/tmp/gents-bombadil/workspace")).toBeVisible();

    await page.getByTestId("setup-next").click();
    await expect(
      page.getByRole("heading", { name: "Choose an inference provider" }),
    ).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.getByRole("radiogroup", { name: "Inference provider" }),
    ).toBeVisible();
    for (const provider of ["OpenAI", "Anthropic", "Grok", "Local", "OpenRouter"]) {
      await expect(
        page.getByRole("radio", { name: new RegExp(`^${provider}`) }),
      ).toBeVisible();
    }
    await page.getByTestId("setup-provider-local").click();
    await page.getByTestId("setup-next").click();
    await expect(page.getByRole("heading", { name: "Connect Local" })).toBeVisible();
    await page.getByTestId("setup-next").click();
    await expect(page.getByRole("heading", { name: "Choose a model" })).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByTestId("setup-next")).toBeDisabled();
    await page.getByRole("option", { name: "GLM-5.3-Flash-NVFP4" }).click();
    await expect(page.getByTestId("setup-next")).toBeEnabled();
    await page.getByTestId("setup-next").click();
    await expect(page.getByRole("heading", { name: "Review inference" })).toBeVisible();
    await expect(page.getByText(/temperature 1 and top-p 0.95/)).toBeVisible();
    await page.getByRole("button", { name: "Customize" }).click();
    await expect(page.getByLabel("Temperature")).toHaveValue("1");
    await expect(page.getByLabel("Top-p")).toHaveValue("0.95");
    await page.getByTestId("setup-next").click();

    await expect(page.getByRole("heading", { name: "You’re in" })).toBeVisible();
    await page.getByTestId("setup-next").click();

    await expect(page.getByTestId("session-screen")).toBeVisible({ timeout: 10_000 });
    await expect(
      page.getByRole("heading", { name: /Start a new chat with Forge/ }),
    ).toBeVisible();

    await composer(page).fill("hello from first-run e2e");
    await expect(sendButton(page)).toBeEnabled();
    await sendButton(page).click();
    await expect(page.getByText(/Bombadil harness response/)).toBeVisible({
      timeout: 10_000,
    });

    await openConfig(page);
    await expect(page.getByText("Local server")).toBeVisible();
    await page.getByRole("button", { name: "Change access…" }).click();
    await page.getByRole("button", { name: "Customize" }).click();
    await page.getByRole("radio", { name: /No files or commands/ }).click();
    await page.getByRole("button", { name: "Review complete — restart" }).click();
    await expect(page.getByText("meta-only")).toBeVisible();
    await expect(page.getByText("No host path").first()).toBeVisible();
  });

  test("keeps invalid directories inline and reviews an alternate preset", async ({
    page,
  }) => {
    await gotoHarness(page, "empty-fleet");
    await page.getByTestId("setup-next").click();
    await page.getByTestId("setup-next").click();

    await page.getByRole("radio", { name: /Use one folder/ }).click();
    await page.getByLabel("Existing directory").fill("/missing folder");
    await page.getByLabel("Existing directory").press("Tab");
    await expect(page.getByText(/Cannot access \/missing folder/)).toBeVisible();
    await expect(page.getByTestId("setup-next")).toBeDisabled();

    await page.getByRole("button", { name: "Customize" }).click();
    await page.getByRole("radio", { name: /No files or commands/ }).click();
    await page.getByTestId("setup-next").click();
    await expect(page.getByRole("heading", { name: "Review access" })).toBeVisible();
    await expect(page.getByText("No host path")).toBeVisible();
    await expect(page.getByText(/no host file or shell authority/i)).toBeVisible();
  });
});
