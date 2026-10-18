import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Users,
  CalendarCheck,
  Clock,
  ArrowLeftRight,
  Activity,
  Settings,
  PanelLeftClose,
  PanelLeft,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

const NAV = [
  { to: "/", label: "概览", icon: LayoutDashboard, end: true },
  { to: "/accounts", label: "账号", icon: Users },
  { to: "/checkin", label: "签到", icon: CalendarCheck },
  { to: "/scheduler", label: "定时", icon: Clock },
  { to: "/switcher", label: "换号", icon: ArrowLeftRight },
  { to: "/monitor", label: "监控", icon: Activity },
  { to: "/settings", label: "设置", icon: Settings },
] as const;

export interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  return (
    <aside
      className={cn(
        "flex h-full flex-col border-r border-border bg-bg-elevated transition-[width] duration-200",
        collapsed ? "w-14" : "w-56",
      )}
    >
      <div
        className={cn(
          "flex h-14 items-center border-b border-border px-3",
          collapsed ? "justify-center" : "justify-between",
        )}
      >
        {!collapsed ? (
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-fg">Buddy 加油站</div>
            <div className="truncate text-[11px] text-fg-subtle">签到 · 换号 · 监控</div>
          </div>
        ) : (
          <div
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border text-xs font-semibold text-fg"
            title="Buddy 加油站"
          >
            B
          </div>
        )}
        <Button
          variant="ghost"
          size="icon"
          onClick={onToggle}
          aria-label={collapsed ? "展开侧栏" : "折叠侧栏"}
        >
          {collapsed ? <PanelLeft className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
        </Button>
      </div>

      <nav className="flex-1 space-y-0.5 p-2">
        {NAV.map(({ to, label, icon: Icon, ...rest }) => (
          <NavLink
            key={to}
            to={to}
            end={"end" in rest ? rest.end : false}
            title={collapsed ? label : undefined}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 rounded-md px-2.5 py-2 text-sm transition-colors",
                collapsed && "justify-center px-0",
                isActive
                  ? "bg-bg-hover text-fg"
                  : "text-fg-muted hover:bg-bg-hover hover:text-fg",
              )
            }
          >
            <Icon className="h-4 w-4 shrink-0" />
            {!collapsed ? <span>{label}</span> : null}
          </NavLink>
        ))}
      </nav>

      <div className="border-t border-border p-3 text-[11px] text-fg-subtle">
        {!collapsed ? "v0.1 · 本地优先" : null}
      </div>
    </aside>
  );
}
