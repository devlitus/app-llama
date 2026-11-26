import { test } from "node:test";
import assert from "node:assert/strict";
import { join } from "node:path";
import { developer, withRoot } from "./memory-policy-fixtures.mjs";
import { checkMemoryToolUse } from "./memory-policy.mjs";

const fileInput = (tool_name, file_path) => ({ ...developer, tool_name, tool_input: { file_path } });
const EXTENDED_PREFIX = "\\\\?\\";

test("politica_bloquea_edit_ruta_extendida", () =>
  withRoot((root) => {
    const input = fileInput("Edit", EXTENDED_PREFIX + join(root, "docs", "memory", "long", "x.md"));
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));

test("politica_bloquea_edit_raiz_mayusculas", () =>
  withRoot((root) => {
    const input = fileInput("Write", root.toUpperCase() + "/docs/memory/x.md");
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));

test("politica_bloquea_edit_punto_final", () =>
  withRoot((root) => {
    const input = fileInput("Write", root + "/docs/memory./long/x.md");
    assert.equal(checkMemoryToolUse(input, { root }).allow, false);
  }));
