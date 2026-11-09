export type Route = "A" | "B" | "S" | "V";

export type NetEntry = {
  seq: number;
  from: string;
  to: string;
  kind: string;
  bytes: number;
  sha256: string;
  t: number;
};

export class Bus {
  log: NetEntry[] = [];
  seq = 0;

  record(from: string, to: string, kind: string, bytes: number, sha256: string) {
    this.log.push({ seq: this.seq++, from, to, kind, bytes, sha256, t: performance.now() });
  }
}

export async function sha256hex(buf: ArrayBuffer): Promise<string> {
  const d = await crypto.subtle.digest("SHA-256", buf);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
