import { useEffect, useState } from "react";
import { NavLink } from "react-router-dom";
import {
  Briefcase,
  ChevronLeft,
  ChevronRight,
  FileText,
  LayoutDashboard,
  Settings,
  Terminal,
  Users,
  Wifi,
} from "lucide-react";
import { cn } from "@/lib/utils";

const SIDEBAR_COLLAPSED_STORAGE_KEY = "kalamdb-admin-sidebar-collapsed";

const navigation = [
  { name: "Dashboard", href: "/dashboard", icon: LayoutDashboard },
  { name: "SQL Studio", href: "/sql", icon: Terminal },
  { name: "Users", href: "/users", icon: Users },
  { name: "Jobs", href: "/jobs", icon: Briefcase },
  { name: "Live Queries", href: "/live-queries", icon: Wifi },
  { name: "Logging", href: "/logging", icon: FileText },
];

export default function Sidebar() {
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    const stored = localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY);
    return stored === "1";
  });

  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, collapsed ? "1" : "0");
  }, [collapsed]);

  return (
    <aside
      className={cn(
        "flex h-full min-h-0 shrink-0 flex-col border-r bg-[radial-gradient(circle_at_20%_10%,rgba(56,189,248,0.18),transparent_35%),linear-gradient(180deg,#0b1427_0%,#0f1f3f_55%,#111827_100%)] text-slate-100 transition-[width] duration-200",
        collapsed ? "w-14" : "w-64",
      )}
    >
      <nav className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
        {navigation.map((item) => (
          <NavLink
            key={item.name}
            to={item.href}
            title={collapsed ? item.name : undefined}
            className={({ isActive }) =>
              cn(
                "flex h-9 items-center rounded-md text-sm transition",
                collapsed ? "justify-center" : "gap-2.5 px-3",
                isActive
                  ? "bg-sky-500/20 text-sky-100"
                  : "text-slate-300 hover:bg-slate-800/60 hover:text-white",
              )
            }
          >
            <item.icon className="h-4 w-4 shrink-0" />
            {!collapsed && <span className="truncate">{item.name}</span>}
          </NavLink>
        ))}
      </nav>

      <div className="shrink-0 border-t border-slate-700/50 p-2">
        <NavLink
          to="/settings"
          title="Settings"
          className={({ isActive }) =>
            cn(
              "flex h-9 items-center rounded-md text-sm transition",
              collapsed ? "justify-center" : "gap-2.5 px-3",
              isActive
                ? "bg-sky-500/20 text-sky-100"
                : "text-slate-300 hover:bg-slate-800/60 hover:text-white",
            )
          }
        >
          <Settings className="h-4 w-4 shrink-0" />
          {!collapsed && <span className="truncate">Settings</span>}
        </NavLink>

        <button
          type="button"
          onClick={() => setCollapsed((prev) => !prev)}
          className={cn(
            "mt-1 flex h-9 w-full items-center rounded-md text-sm text-slate-400 transition hover:bg-slate-800/60 hover:text-slate-200",
            collapsed ? "justify-center" : "gap-2.5 px-3",
          )}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? (
            <ChevronRight className="h-4 w-4" />
          ) : (
            <ChevronLeft className="h-4 w-4" />
          )}
          {!collapsed && <span>{collapsed ? "Expand" : "Collapse"}</span>}
        </button>
      </div>
    </aside>
  );
}
