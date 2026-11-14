/**
 * The displayable text and the log code of a refusal of a delivery operation
 * (`DSR-delivery-source.md`). The backend refuses with a `DeliveryError`
 * (`{ code, message }`); an older or a mocked backend can refuse with the bare
 * code as a string.
 */

/** Text for the codes that a bare string refusal can carry. */
const CODE_TEXT: Record<string, string> = {
  no_project_open: "No project is open.",
  not_a_flow: "The chosen file is not a Flow file of this project.",
  invalid_delivery_settings:
    "Each Type name must not be empty, and one Type can map to one kind only.",
};

/** The code of a refusal. A code holds no name and no description, so a log can carry it. */
export function deliveryErrorCode(reason: unknown): string {
  if (reason && typeof reason === "object" && "code" in reason) {
    const code = (reason as { code: unknown }).code;
    if (typeof code === "string" && code !== "") return code;
  }
  if (typeof reason === "string") return reason;
  // The message of a thrown Error is free text, so the log does not carry it.
  return "unknown";
}

/** The displayable text of a refusal. */
export function deliveryErrorMessage(reason: unknown): string {
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = (reason as { message: unknown }).message;
    if (typeof message === "string" && message !== "") {
      return CODE_TEXT[message] ?? message;
    }
  }
  const code = deliveryErrorCode(reason);
  return CODE_TEXT[code] ?? `The delivery operation failed (${code}).`;
}
