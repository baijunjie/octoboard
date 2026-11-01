import { describe, expect, it } from "vitest";

import { FEATURE_MESSAGE, PREVIEW_MESSAGE, readFeatureMessage, readPreviewMessage } from "./messages";

describe("readPreviewMessage", () => {
  it("reads the interaction modalities and nothing else", () => {
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.interaction, modality: "keyboard" })).toEqual({ type: PREVIEW_MESSAGE.interaction, modality: "keyboard" });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.interaction, modality: "pointer" })).toEqual({ type: PREVIEW_MESSAGE.interaction, modality: "pointer" });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.interaction, modality: "touch" })).toBeUndefined();
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.interaction })).toBeUndefined();
  });

  it("reads a scroll with a finite deltaY, a finite velocity, and a stop only when it is true", () => {
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 12 })).toEqual({ type: PREVIEW_MESSAGE.scroll, deltaY: 12, stop: false });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, velocity: 0.5 })).toEqual({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, velocity: 0.5, stop: false });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, stop: true })).toEqual({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, stop: true });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, stop: "true" })).toMatchObject({ stop: false });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, velocity: Infinity })).not.toHaveProperty("velocity");
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY: 0, velocity: "1" })).not.toHaveProperty("velocity");
  });

  it("rejects a scroll without a finite deltaY", () => {
    for (const deltaY of [undefined, "5", NaN, Infinity, null]) {
      expect(readPreviewMessage({ type: PREVIEW_MESSAGE.scroll, deltaY })).toBeUndefined();
    }
  });

  it("reads ready and error, and ignores everything else", () => {
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.ready })).toEqual({ type: PREVIEW_MESSAGE.ready });
    expect(readPreviewMessage({ type: PREVIEW_MESSAGE.error })).toEqual({ type: PREVIEW_MESSAGE.error });
    for (const data of [null, undefined, "octoboard-preview-ready", 3, {}, { type: "other" }, { type: FEATURE_MESSAGE.ready, feature: "x" }]) {
      expect(readPreviewMessage(data)).toBeUndefined();
    }
  });
});

describe("readFeatureMessage", () => {
  it("reads ready and error with the feature they name", () => {
    expect(readFeatureMessage({ type: FEATURE_MESSAGE.ready, feature: "git" })).toEqual({ type: FEATURE_MESSAGE.ready, feature: "git" });
    expect(readFeatureMessage({ type: FEATURE_MESSAGE.error, feature: "git" })).toEqual({ type: FEATURE_MESSAGE.error, feature: "git" });
  });

  it("reads an error that names no feature", () => {
    expect(readFeatureMessage({ type: FEATURE_MESSAGE.error })).toEqual({ type: FEATURE_MESSAGE.error });
  });

  it("ignores anything else", () => {
    for (const data of [null, undefined, {}, { type: FEATURE_MESSAGE.ready }, { type: FEATURE_MESSAGE.ready, feature: 3 }, { type: PREVIEW_MESSAGE.ready, feature: "git" }]) {
      expect(readFeatureMessage(data)).toBeUndefined();
    }
  });
});
