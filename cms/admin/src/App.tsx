import { useState } from "react";
import { BuilderWorkspace } from "./builder/BuilderWorkspace";
import { SystemStatus } from "./features/system/SystemStatus";
import { AdminShell, type AdminView } from "./shell/AdminShell";

export function App() {
  const [view, setView] = useState<AdminView>("overview");

  return (
    <AdminShell view={view} onNavigate={setView}>
      {view === "builder" ? <BuilderWorkspace /> : <SystemStatus />}
    </AdminShell>
  );
}
