/// <reference lib="webworker" />

self.onmessage = async (e: MessageEvent) => {
  if (e.data?.type === "init") {
    self.postMessage({ type: "ready" });
  }
};
