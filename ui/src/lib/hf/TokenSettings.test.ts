import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import TokenSettings from "./TokenSettings.svelte";
import type { TokenStatus } from "./types";

afterEach(clearMocks);

const TOKEN = `hf_${"a1B2".repeat(9).slice(0, 34)}`;
const NO_TOKEN: TokenStatus = { present: false, user: null, invalid: false };

interface Calls {
  [cmd: string]: unknown[];
}

function mockHf(status: TokenStatus, handlers: Record<string, (args: unknown) => unknown> = {}): Calls {
  const calls: Calls = {};
  mockIPC((cmd, args) => {
    (calls[cmd] ??= []).push(args);
    if (cmd === "hf_token_status") return status;
    const handler = handlers[cmd];
    if (handler) return handler(args);
    return undefined;
  });
  return calls;
}

function tokenField(): HTMLInputElement {
  return screen.getByLabelText("Token de acceso") as HTMLInputElement;
}

async function typeAndSave(value: string): Promise<void> {
  await fireEvent.input(tokenField(), { target: { value } });
  await fireEvent.click(screen.getByRole("button", { name: "Guardar" }));
}

describe("TokenSettings", () => {
  it("token_settings_save_valid", async () => {
    const calls = mockHf(NO_TOKEN, { hf_token_save: () => ({ name: "ana", role: "read" }) });
    render(TokenSettings);
    await screen.findByText("Sin token");

    await typeAndSave(TOKEN);

    expect(await screen.findByText("Conectado como ana")).toBeInTheDocument();
    expect(calls["hf_token_save"]).toEqual([{ token: TOKEN }]);
    expect(tokenField().value).toBe("");
    expect(tokenField().type).toBe("password");
  });

  it("token_settings_invalid_format_no_call", async () => {
    const calls = mockHf(NO_TOKEN);
    render(TokenSettings);
    await screen.findByText("Sin token");

    await typeAndSave("abc");

    expect(
      await screen.findByText("El token debe empezar por hf_ y tener al menos 30 caracteres alfanuméricos más"),
    ).toBeInTheDocument();
    expect(calls["hf_token_save"]).toBeUndefined();
  });

  it("token_settings_unauthorized", async () => {
    mockHf(NO_TOKEN, {
      hf_token_save: () => {
        throw { code: "Unauthorized", message: "Acceso denegado: revisa tu token" };
      },
    });
    render(TokenSettings);
    await screen.findByText("Sin token");

    await typeAndSave(TOKEN);

    expect(await screen.findByRole("alert")).toHaveTextContent("Token no válido");
    expect(screen.getByText("Sin token")).toBeInTheDocument();
  });

  it("token_settings_other_error", async () => {
    mockHf(NO_TOKEN, {
      hf_token_save: () => {
        throw { code: "Secret", message: "No se pudo acceder al llavero del sistema" };
      },
    });
    render(TokenSettings);
    await screen.findByText("Sin token");

    await typeAndSave(TOKEN);

    expect(await screen.findByRole("alert")).toHaveTextContent("No se pudo acceder al llavero del sistema");
  });

  it("token_settings_delete", async () => {
    const calls = mockHf({ present: true, user: "ana", invalid: false });
    render(TokenSettings);
    await screen.findByText("Conectado como ana");

    await fireEvent.click(screen.getByRole("button", { name: "Borrar" }));

    expect(await screen.findByText("Sin token")).toBeInTheDocument();
    expect(calls["hf_token_delete"]).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Borrar" })).not.toBeInTheDocument();
  });

  it("token_settings_revoked", async () => {
    mockHf({ present: true, user: null, invalid: true });
    render(TokenSettings);

    expect(await screen.findByText("Token no válido")).toBeInTheDocument();
  });

  it("token_settings_unverified", async () => {
    mockHf({ present: true, user: null, invalid: false });
    render(TokenSettings);

    expect(await screen.findByText("Token guardado (sin verificar)")).toBeInTheDocument();
  });

  it("token_settings_buttons_disabled_while_saving", async () => {
    let release: (value: unknown) => void = () => {};
    const pending = new Promise((resolve) => (release = resolve));
    mockHf({ present: true, user: "ana", invalid: false }, { hf_token_save: () => pending });
    render(TokenSettings);
    await screen.findByText("Conectado como ana");

    await typeAndSave(TOKEN);

    expect(screen.getByRole("button", { name: "Guardar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Borrar" })).toBeDisabled();
    release({ name: "ana", role: "read" });
    await waitFor(() => expect(screen.getByRole("button", { name: "Borrar" })).toBeEnabled());
  });

  it("token_not_persisted_in_local_storage", async () => {
    mockHf(NO_TOKEN, { hf_token_save: () => ({ name: "ana", role: "read" }) });
    render(TokenSettings);
    await screen.findByText("Sin token");
    const before = localStorage.length;

    await typeAndSave(TOKEN);
    await screen.findByText("Conectado como ana");

    expect(localStorage.length).toBe(before);
  });
});
