/**
 * Typed wrappers over every backend `#[tauri::command]`.
 *
 * This is the single place command-name strings live on the frontend: every
 * component calls a function here rather than `invoke("...")` directly, so a
 * rename or a wrong argument shape is a compile error at one site instead of a
 * silent runtime failure scattered across the UI. Each wrapper mirrors a command
 * registered in `src-tauri/src/lib.rs`'s `generate_handler!`.
 *
 * Event subscriptions (the backend-emitted channels) live in `../events`.
 *
 * The wrappers stand in one file per backend area rather than in one file, and
 * this re-exports all of them, so a caller writes `from "../api"` whichever
 * area it reaches and no call site names an area at all. A wrapper belongs to
 * the area whose Rust module registers its command.
 */
export * from "./content";
export * from "./docker";
export * from "./documents";
export * from "./remoteConnectivity";
export * from "./drafts";
export * from "./git";
export * from "./githubPolling";
export * from "./integrations";
export * from "./library";
export * from "./panels";
export * from "./project";
export * from "./streams";
