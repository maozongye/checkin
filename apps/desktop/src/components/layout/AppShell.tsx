import { useState } from "react";
import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { useMonitorHealthPoll } from "@/hooks/useMonitorHealthPoll";

export function AppShell() {
  const [collapsed, setCollapsed] = useState(false);
  // 壳层按 settings 探活间隔调 monitor_refresh（无常驻 Rust timer）
  useMonitorHealthPoll();

  return (
    <div className="flex h-full min-h-0 bg-bg text-fg">
      <Sidebar collapsed={collapsed} onToggle={() => setCollapsed((v) => !v)} />
      <main className="min-w-0 flex-1 overflow-auto">
        <div className="mx-auto max-w-6xl px-6 py-6">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
