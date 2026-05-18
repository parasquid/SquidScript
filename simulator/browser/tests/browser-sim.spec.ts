import { expect, test } from "@playwright/test";

test("compile, upload, run, input, diagnostics, and reset flow", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByLabel("XTEINK X4 device simulator")).toBeVisible();
  await expect(page.getByLabel("Squid source")).toBeVisible();
  await expect(page.getByRole("button", { name: /Compile/ })).toBeVisible();

  await page.getByRole("button", { name: /Compile/ }).click();
  await expect(page.getByRole("status")).toContainText("Compiled main.ir.json");
  await expect(page.getByLabel("compiler backend")).toContainText("Compiler: WASM");
  await expect(page.getByLabel("debug log")).toContainText("compile succeeded");

  await page.getByRole("button", { name: /Upload/ }).click();
  await expect(page.getByRole("status")).toContainText("/sd/apps/hello-menu");
  await expect(page.getByLabel("installed apps")).toContainText("1 installed");
  await expect(page.getByLabel("installed app selector")).toHaveValue("hello-menu");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/app.json");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/main.ir.json");

  await page.reload();
  await page.getByRole("button", { name: /Run/ }).click();
  await expect(page.getByRole("status")).toContainText("Running Hello Menu from /sd/apps/hello-menu");
  await expect(page.getByLabel("debug log")).toContainText("runtime started");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-app-id", "hello-menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "main");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");

  await page.getByRole("button", { name: "Down" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "1");
  await expect(page.getByLabel("debug log")).toContainText("button outcome");
  await expect(page.getByLabel("debug log")).toContainText("key dispatched");
  await page.keyboard.press("ArrowUp");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "detail");
  await page.keyboard.press("Backspace");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "true");

  await page.getByRole("button", { name: /Reset App State/ }).click();
  await expect(page.getByRole("status")).toContainText("Reset app state");

  await page.getByLabel("Squid source").fill('screen("main") {}\n');
  await page.getByRole("button", { name: /Compile/ }).click();
  await expect(page.getByLabel("diagnostics")).toContainText("E_APP_REQUIRED");

  await page.getByRole("button", { name: /Reset Storage/ }).click();
  await expect(page.getByRole("status")).toContainText("Reset simulated /sd");
  await expect(page.getByLabel("installed apps")).toContainText("0 installed");
  await expect(page.getByLabel("storage files")).toContainText("No /sd files");
});
