import { test } from "node:test";
import assert from "node:assert/strict";
import { compareBySlug, comparePlans, compareTasks, planNumber, taskNumber } from "./memory-order.mjs";

test("order_numeros_de_tarea_y_de_plan", () => {
  assert.equal(taskNumber("T10"), 10);
  assert.equal(planNumber("007-x"), 7);
});

test("order_tareas_en_orden_numerico", () => {
  assert.deepEqual(["T10", "T2", "T1"].sort(compareTasks), ["T1", "T2", "T10"]);
});

test("order_planes_por_numero_y_despues_por_nombre", () => {
  assert.deepEqual(["010-a", "002-b", "002-a"].sort(comparePlans), ["002-a", "002-b", "010-a"]);
});

test("order_slugs_alfabeticos", () => {
  const entries = [{ slug: "b" }, { slug: "a" }, { slug: "a-b" }];
  assert.deepEqual(entries.sort(compareBySlug).map(({ slug }) => slug), ["a", "a-b", "b"]);
  assert.equal(compareBySlug({ slug: "a" }, { slug: "a" }), 0);
});
