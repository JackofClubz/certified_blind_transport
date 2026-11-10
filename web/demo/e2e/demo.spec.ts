import { test, expect } from "@playwright/test";

test("honesty box is flag-driven and does not claim private matching as a slogan", async ({
  page,
}) => {
  await page.goto("/");
  const box = page.locator(".honesty");
  await expect(box).toContainText("Honesty box");
  await expect(box).toContainText("toy");
  await expect(box).toContainText("λ");
  const body = await page.textContent("body");
  expect(body?.toLowerCase().includes("server never trusted")).toBeFalsy();
});

test("view-as server hides both books", async ({ page }) => {
  await page.goto("/");
  await page.selectOption("#view", "Server");
  await expect(page.locator(".cipher").first()).toBeVisible();
  await expect(page.locator('#ladder input[data-side="a"]')).toHaveCount(0);
  await expect(page.locator('#ladder input[data-side="b"]')).toHaveCount(0);
});

test("honest run reports a certificate (wasm or stub)", async ({ page }) => {
  await page.goto("/");
  await page.click("#run");
  await expect(page.locator("#readout")).not.toHaveText("Running protocol", {
    timeout: 90_000,
  });
  const cert = await page.locator("#cert").innerText();
  expect(cert.length).toBeGreaterThan(10);
});
