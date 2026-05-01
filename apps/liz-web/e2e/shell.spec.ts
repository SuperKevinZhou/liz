import { expect, test } from "@playwright/test";

test("loads the owner-facing home shell", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "Home" })).toBeVisible();
  await expect(page.getByLabel("Primary navigation")).toBeVisible();
  await expect(page.getByRole("button", { name: "Home" })).toHaveClass(/active/);
  await expect(page.getByRole("button", { name: "People" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Devices" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Workspaces" })).toBeVisible();
  await expect(page.getByText("Liz Console")).toHaveCount(0);
  await expect(page.getByPlaceholder("Message Liz")).toBeVisible();
});
