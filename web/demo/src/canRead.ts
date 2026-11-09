export type View = "All" | "A" | "B" | "Server" | "Public";
export type Datum = "ordersA" | "ordersB" | "trades" | "cipher" | "commitments" | "pi";

export function canRead(view: View, datum: Datum): boolean {
  switch (view) {
    case "All":
      return true;
    case "A":
      return datum === "ordersA" || datum === "trades" || datum === "pi" || datum === "commitments";
    case "B":
      return datum === "ordersB" || datum === "trades" || datum === "pi" || datum === "commitments";
    case "Server":
      return datum === "cipher" || datum === "commitments" || datum === "pi";
    case "Public":
      return datum === "trades" || datum === "pi" || datum === "commitments";
  }
}
