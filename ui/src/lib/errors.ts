export interface ErrorPayload {
  code: string;
  message: string;
}

const UNKNOWN_ERROR = "Ha ocurrido un error inesperado.";

function isErrorPayload(error: unknown): error is ErrorPayload {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.message === "string"
  );
}

export function errorMessage(error: unknown): string {
  return isErrorPayload(error) ? error.message : UNKNOWN_ERROR;
}
