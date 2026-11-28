import { describe, expect, it } from "vitest";
import { INITIAL_SECTION, SECTIONS, isSectionId, sectionLabel } from "./navigation";

describe("navigation", () => {
  it("secciones_en_orden", () => {
    expect(SECTIONS.map((s) => s.id)).toEqual([
      "explore",
      "downloads",
      "library",
      "run",
      "chat",
      "settings",
    ]);
  });

  it("secciones_con_etiqueta_en_espanol", () => {
    expect(SECTIONS.map((s) => s.label)).toEqual([
      "Explorar",
      "Descargas",
      "Biblioteca",
      "Ejecutar",
      "Chat",
      "Ajustes",
    ]);
  });

  it("seccion_inicial_es_explorar", () => {
    expect(INITIAL_SECTION).toBe("explore");
  });

  it("ids_de_seccion_validos", () => {
    for (const { id } of SECTIONS) {
      expect(isSectionId(id)).toBe(true);
    }
  });

  it("ids_de_seccion_invalidos", () => {
    for (const value of ["", "Explore", "ajustes", "constructor", "toString"]) {
      expect(isSectionId(value)).toBe(false);
    }
  });

  it("etiqueta_de_seccion", () => {
    expect(sectionLabel("chat")).toBe("Chat");
  });
});
