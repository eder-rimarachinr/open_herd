import { Database as DbIcon } from "lucide-react";

export default function Database() {
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", height: "60vh", gap: 12, color: "var(--text-3)" }}>
      <DbIcon size={40} strokeWidth={1.2} />
      <h2 style={{ fontSize: 18, fontWeight: 600, color: "var(--text-2)" }}>Database</h2>
      <p style={{ fontSize: 13, maxWidth: 360, textAlign: "center", lineHeight: 1.6 }}>
        Database management is coming soon. You'll be able to create, browse, and manage
        local databases per project from here.
      </p>
    </div>
  );
}
