import { expect, test } from "@playwright/test";

test("loads the console shell", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "Liz Console" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Chat" })).toHaveClass(/active/);
  await expect(page.getByPlaceholder("Message Liz")).toBeVisible();
});
