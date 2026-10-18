import { Navigate, type RouteObject } from "react-router-dom";
import { AppShell } from "@/components/layout/AppShell";
import { OverviewPage } from "@/pages/OverviewPage";
import { AccountsPage } from "@/pages/AccountsPage";
import { CheckinPage } from "@/pages/CheckinPage";
import { SchedulerPage } from "@/pages/SchedulerPage";
import { SwitcherPage } from "@/pages/SwitcherPage";
import { MonitorPage } from "@/pages/MonitorPage";
import { SettingsPage } from "@/pages/SettingsPage";

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: "accounts", element: <AccountsPage /> },
      { path: "checkin", element: <CheckinPage /> },
      { path: "scheduler", element: <SchedulerPage /> },
      { path: "switcher", element: <SwitcherPage /> },
      { path: "monitor", element: <MonitorPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
];
