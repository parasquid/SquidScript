import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { strToU8, zipSync } from "fflate";
import { DEFAULT_SOURCE } from "../src/compiler/defaultSource";
import init, { compile_sqbc } from "../src/compiler/wasm/squidc_wasm.js";

test("compile, upload, run, input, diagnostics, and reset flow", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByLabel("XTEINK X4 device simulator")).toBeVisible();
  await expect(page.getByLabel("Squid source")).toBeVisible();
  await expect(page.getByRole("button", { name: /Compile/ })).toBeVisible();
  await expect(page.getByLabel("display diagnostics")).toContainText("Display 480x800");
  await expect(page.getByLabel("X4 display")).toHaveAttribute("data-render-ok", "true");
  await expect(page.getByLabel("display diagnostics")).toContainText("2 commands");
  await expectCanvasPixel(page, 10, 10, [255, 255, 255]);

  await page.getByRole("button", { name: /Compile/ }).click();
  await expect(page.getByRole("status")).toContainText("Compiled main.sqbc");
  await expect(page.getByLabel("compiler backend")).toContainText("Compiler: WASM");
  await expect(page.getByLabel("debug log")).toContainText("compile succeeded");

  await page.getByRole("button", { name: /Upload/ }).click();
  await expect(page.getByRole("status")).toContainText("/sd/apps/hello-menu");
  await expect(page.getByLabel("installed apps")).toContainText("1 installed");
  await expect(page.getByLabel("installed app selector")).toHaveValue("hello-menu");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/main.sqbc");
  await expect(page.getByLabel("storage files")).not.toContainText("/sd/apps/hello-menu/app.json");

  await page.reload();
  await page.getByRole("button", { name: /Run/ }).click();
  await expect(page.getByRole("status")).toContainText("Running hello-menu from /sd/apps/hello-menu");
  await expect(page.getByLabel("debug log")).toContainText("runtime started");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-app-id", "hello-menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-view", "menu");
  await expect(page.getByLabel("display diagnostics")).toContainText("first clear");
  await expect(page.getByLabel("X4 display")).toHaveAttribute("data-command-count", /[1-9][0-9]*/);
  await expectCanvasPixel(page, 10, 10, [255, 255, 255]);
  await expectCanvasPixel(page, 40, 170, [0, 0, 0]);
  await expectCanvasPixel(page, 40, 226, [255, 255, 255]);

  await page.keyboard.press("ArrowUp");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");

  await page.getByRole("button", { name: "Down" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "1");
  await expectCanvasPixel(page, 40, 170, [255, 255, 255]);
  await expectCanvasPixel(page, 40, 226, [0, 0, 0]);
  await expect(page.getByLabel("debug log")).toContainText("button outcome");
  await expect(page.getByLabel("debug log")).toContainText("key dispatched");

  await page.reload();
  await page.getByRole("button", { name: /Run/ }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "1");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");

  await page.keyboard.press("Enter");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "about");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-view", "about");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "about");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "1");
  await page.keyboard.press("Backspace");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "false");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-view", "menu");

  await page.getByRole("button", { name: /Reset App State/ }).click();
  await expect(page.getByRole("status")).toContainText("Reset app state");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");

  await page.getByRole("button", { name: /Run/ }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "false");
  await page.getByRole("button", { name: "Select" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "hello");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-view", "hello");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "hello");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");
  await page.keyboard.press("Backspace");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-view", "menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "false");
  await page.keyboard.press("Backspace");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "true");

  await page.getByRole("button", { name: /Run/ }).click();
  await page.getByRole("button", { name: "Down" }).click();
  await page.getByRole("button", { name: "Down" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "2");
  await expectCanvasPixel(page, 40, 282, [0, 0, 0]);
  await page.getByRole("button", { name: "Down" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "2");
  await page.getByRole("button", { name: "Select" }).click();
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-exited", "true");

  await page.getByLabel("Squid source").fill('screen("main") {}\n');
  await page.getByRole("button", { name: /Compile/ }).click();
  await expect(page.getByLabel("diagnostics", { exact: true })).toContainText("E_APP_REQUIRED");

  await page.getByRole("button", { name: /Reset Storage/ }).click();
  await expect(page.getByRole("status")).toContainText("Reset simulated /sd");
  await expect(page.getByLabel("installed apps")).toContainText("0 installed");
  await expect(page.getByLabel("storage files")).toContainText("No /sd files");

  await page.getByLabel("package import").setInputFiles({
    name: "hello-menu.squid.zip",
    mimeType: "application/zip",
    buffer: await helloMenuPackage()
  });
  await expect(page.getByRole("status")).toContainText("Imported /sd/apps/hello-menu");
  await expect(page.getByLabel("installed apps")).toContainText("1 installed");
  await expect(page.getByLabel("installed app selector")).toHaveValue("hello-menu");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/main.sqbc");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/web/index.html");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/web/js/app.js");
  await expect(page.getByLabel("storage files")).toContainText("/sd/apps/hello-menu/resources/icon.bmp");
  await page.getByRole("button", { name: /Run/ }).click();
  await expect(page.getByRole("status")).toContainText("Running hello-menu from /sd/apps/hello-menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");
  await expectCanvasPixel(page, 40, 170, [0, 0, 0]);

  await page.getByLabel("Squid source").fill('screen("stale") {}\n');
  await page.getByRole("button", { name: /Reset Simulator/ }).click();
  await expect(page.getByRole("status")).toContainText("Reset simulator");
  await expect(page.getByLabel("Squid source")).toContainText('app "hello-menu" target "xteink-x4"');
  await expect(page.getByLabel("compiler backend")).toContainText("Compiler: UNKNOWN");
  await expect(page.getByLabel("installed apps")).toContainText("0 installed");
  await expect(page.getByLabel("storage files")).toContainText("No /sd files");

  await page.getByRole("button", { name: /Clean Launch/ }).click();
  await expect(page.getByRole("status")).toContainText("Running hello-menu from /sd/apps/hello-menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-app-id", "hello-menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-current-screen", "menu");
  await expect(page.getByTestId("runtime-state")).toHaveAttribute("data-selected", "0");
  await expectCanvasPixel(page, 10, 10, [255, 255, 255]);
  await expectCanvasPixel(page, 40, 170, [0, 0, 0]);
});

async function expectCanvasPixel(page: Page, x: number, y: number, rgb: [number, number, number]): Promise<void> {
  await expect.poll(async () => page.getByLabel("X4 display").evaluate((canvas, point) => {
    const context = (canvas as HTMLCanvasElement).getContext("2d");
    if (!context) return null;
    return Array.from(context.getImageData(point.x, point.y, 1, 1).data.slice(0, 3));
  }, { x, y }), { message: `canvas pixel ${x},${y}` }).toEqual(rgb);
}

let wasmReady: Promise<void> | null = null;

async function helloMenuPackage(): Promise<Buffer> {
  wasmReady ??= init({
    module_or_path: readFileSync(join(process.cwd(), "src/compiler/wasm/squidc_wasm_bg.wasm"))
  }).then(() => undefined);
  await wasmReady;
  const sqbc = compile_sqbc(DEFAULT_SOURCE, "xteink-x4");
  return Buffer.from(zipSync({
    "main.sqbc": sqbc,
    "web/index.html": strToU8("<script type=\"module\" src=\"./js/app.js\"></script>"),
    "web/js/app.js": strToU8("fetch('./data/menu.json')"),
    "web/css/app.css": strToU8("body { color: black; }"),
    "web/data/menu.json": strToU8("{\"items\":[\"hello\"]}"),
    "resources/icon.bmp": new Uint8Array([0, 1, 2, 255])
  }));
}
