import { describe, expect, it } from "vitest";
import { canRead } from "./canRead";

describe("canRead", () => {
  it("never shows amber order sizes to the server", () => {
    expect(canRead("Server", "ordersA")).toBe(false);
    expect(canRead("Server", "ordersB")).toBe(false);
    expect(canRead("Server", "cipher")).toBe(true);
  });
  it("public sees trades and pi only", () => {
    expect(canRead("Public", "ordersA")).toBe(false);
    expect(canRead("Public", "trades")).toBe(true);
  });
});
