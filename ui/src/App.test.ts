import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import App from "./App.svelte";
import { SECTIONS } from "./lib/navigation";

function navigationButtons(): HTMLElement[] {
  return within(screen.getByRole("navigation", { name: "Secciones" })).getAllByRole("button");
}

describe("App", () => {
  it("muestra_cinco_botones_de_navegacion", () => {
    render(App);
    expect(navigationButtons().map((b) => b.textContent?.trim())).toEqual(
      SECTIONS.map((s) => s.label),
    );
  });

  it("seccion_inicial_explorar", () => {
    render(App);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Explorar");
    expect(screen.getByRole("button", { name: "Explorar" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("navega_a_cada_seccion", async () => {
    render(App);
    for (const { label } of SECTIONS) {
      await fireEvent.click(screen.getByRole("button", { name: label }));
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(label);
      const current = navigationButtons().filter((b) => b.getAttribute("aria-current") === "page");
      expect(current).toHaveLength(1);
      expect(current[0]).toHaveTextContent(label);
    }
  });

  it("click_en_la_activa_no_cambia", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: "Explorar" }));
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Explorar");
  });
});
