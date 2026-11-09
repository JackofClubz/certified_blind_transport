/// <reference lib="webworker" />

self.onmessage = async (e: MessageEvent) => {
  const { type } = e.data ?? {};
  if (type === "init") {
    self.postMessage({ type: "ready", role: e.data.payload?.role ?? "A" });
  } else if (type === "msg") {
    self.postMessage({ type: "msg", payload: e.data.payload });
  }
};
