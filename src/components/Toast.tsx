import { Icon } from "./icons";

/** The transient, fixed-position confirmation toast. Renders nothing when null. */
export function Toast({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <div style={{ position: "fixed", bottom: 24, right: 24, zIndex: 300 }}>
      <div
        className="card"
        style={{
          padding: "10px 14px",
          display: "flex",
          alignItems: "center",
          gap: 8,
          boxShadow: "var(--shadow-3)",
        }}
      >
        <Icon.Check size={14} className="icon" />
        <span style={{ fontSize: "var(--fs-ui-sm)" }}>{message}</span>
      </div>
    </div>
  );
}
