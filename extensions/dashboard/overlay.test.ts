import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { INSPECTION_OVERLAY_OPTIONS } from "./overlay.ts";

describe("dashboard inspection overlay layout", () => {
  it("uses a wide centered blocking layout", () => {
    assert.equal(INSPECTION_OVERLAY_OPTIONS.anchor, "center");
    assert.equal(INSPECTION_OVERLAY_OPTIONS.width, "88%");
    assert.equal(INSPECTION_OVERLAY_OPTIONS.minWidth, 80);
    assert.equal(INSPECTION_OVERLAY_OPTIONS.maxHeight, "88%");
    assert.equal(INSPECTION_OVERLAY_OPTIONS.margin, 1);
  });

  it("only shows the inspection overlay on sufficiently wide terminals", () => {
    assert.equal(INSPECTION_OVERLAY_OPTIONS.visible(99), false);
    assert.equal(INSPECTION_OVERLAY_OPTIONS.visible(100), true);
    assert.equal(INSPECTION_OVERLAY_OPTIONS.visible(160), true);
  });
});
