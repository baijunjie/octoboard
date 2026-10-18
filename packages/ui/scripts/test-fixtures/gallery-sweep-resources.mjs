const mode = process.env.GALLERY_TEST_MODE;

function close(name) {
  console.log(`closed ${name}`);
  if (mode === `hang-${name}`) {
    setInterval(() => {}, 1_000);
    return new Promise(() => {});
  }
  if (mode?.endsWith(`reject-${name}`)) throw new Error(`${name} close rejected`);
}

export async function createServer() {
  return {
    async listen() {
      if (mode === "listen-failure") throw new Error("listen failed");
    },
    httpServer: { address: () => ({ port: 1234 }) },
    close: () => close("server"),
  };
}

const page = {
  async goto() {},
  async close() {},
  on() {},
  async setViewportSize() {},
  async waitForSelector() {},
  async waitForTimeout() {},
  async evaluate(fn) {
    if (String(fn).includes("SCENARIOS")) {
      return { scenarios: [{ id: "test", group: "Test" }], languages: ["en", "zh-Hans"] };
    }
    return mode?.startsWith("scenario-failure") ? ["fixture failed"] : [];
  },
};

export const chromium = {
  async launch() {
    if (mode === "launch-failure") throw new Error("launch failed");
    if (mode === "signal") {
      console.log("ready for signal");
      setInterval(() => {}, 1_000);
      return new Promise(() => {});
    }
    return {
      async newPage() { return page; },
      async newContext() { return { newPage: async () => page, async close() {} }; },
      close: () => close("browser"),
    };
  },
};
