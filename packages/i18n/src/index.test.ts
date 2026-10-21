import assert from "node:assert/strict";
import { test } from "node:test";
import { createTranslator } from "./index.ts";

const t = createTranslator({ hi: "Hi {name}", twice: "{a}-{a}" });

test("replaces_named_params", () => {
  assert.equal(t("hi", { name: "An" }), "Hi An");
});

test("missing_param_left_visible", () => {
  assert.equal(t("hi", {}), "Hi {name}");
});

test("repeated_param", () => {
  assert.equal(t("twice", { a: "x" }), "x-x");
});
