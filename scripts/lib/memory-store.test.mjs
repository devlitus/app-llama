import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { findSessionByAgentId, listSessions, nextN, sessionsOfTask } from "./memory-store.mjs";
import { renderSession } from "./memory-entry.mjs";

const input = { summary: "a", decisions: [], errors: [] };

function meta(overrides) {
  return {
    fecha: "2026-11-25",
    plan: "001-memoria",
    tarea: "T2",
    agente: "developer",
    agentId: "a1",
    n: 1,
    rama: "chore/001-memoria",
    gate: "verde",
    files: [],
    ...overrides,
  };
}

function withRoot(body) {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  mkdirSync(join(root, "docs", "memory", "short"), { recursive: true });
  try {
    body(root, (name, content) => writeFileSync(join(root, "docs", "memory", "short", name), content));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("store_lista_vacia_sin_directorio", () => {
  const root = mkdtempSync(join(tmpdir(), "mem-"));
  try {
    assert.deepEqual(listSessions(root), []);
    assert.equal(nextN(root, "2026-11-25-001-memoria-T2-developer-"), 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("store_lista_sesiones", () =>
  withRoot((root, write) => {
    write("2026-11-25-001-memoria-T2-developer-1.md", renderSession(meta({}), input));
    const [session] = listSessions(root);
    assert.equal(session.rel, "docs/memory/short/2026-11-25-001-memoria-T2-developer-1.md");
    assert.deepEqual(session.meta, meta({}));
  }));

test("store_sesion_ilegible_lanza_error", () =>
  withRoot((root, write) => {
    write("x.md", "no es una sesión");
    assert.throws(() => listSessions(root), /x\.md/);
  }));

test("store_next_n_usa_el_maximo", () =>
  withRoot((root, write) => {
    const prefix = "2026-11-25-001-memoria-T2-developer-";
    write(`${prefix}1.md`, "x");
    write(`${prefix}3.md`, "x");
    write("2026-11-25-001-memoria-T2-reviewer-7.md", "x");
    assert.equal(nextN(root, prefix), 4);
  }));

test("store_busca_por_agent_id_y_tolera_corruptos", () =>
  withRoot((root, write) => {
    write("a.md", "basura \u0000 sin front matter");
    write("b.md", renderSession(meta({ agentId: "a1" }), input));
    assert.equal(findSessionByAgentId(root, "a1").rel, "docs/memory/short/b.md");
    assert.equal(findSessionByAgentId(root, "a"), null);
  }));

test("store_agent_id_exacto_en_su_linea", () =>
  withRoot((root, write) => {
    write("c.md", "---\nagent_id: a1-extra\n---\n");
    assert.equal(findSessionByAgentId(root, "a1"), null);
    assert.equal(findSessionByAgentId(root, "a1-extra").rel, "docs/memory/short/c.md");
  }));

test("store_sesiones_de_tarea", () =>
  withRoot((root, write) => {
    write("1.md", renderSession(meta({ tarea: "T2" }), input));
    write("2.md", renderSession(meta({ tarea: "T3", agentId: "a2" }), input));
    write("3.md", renderSession(meta({ plan: "002-x", agentId: "a3" }), input));
    const found = sessionsOfTask(root, "001-memoria", "T2");
    assert.deepEqual(found.map(({ rel }) => rel), ["docs/memory/short/1.md"]);
  }));
