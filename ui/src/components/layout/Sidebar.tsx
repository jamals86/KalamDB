import { NavLink } from "react-router-dom";
import {
  LayoutDashboard,
  Terminal,
  Users,
  FolderTree,
  FileText,
  Settings,
  Database,
  Wifi,
} from "lucide-react";
import { cn } from "@/lib/utils";

const navigation = [
  { name: "Dashboard", href: "/dashboard", icon: LayoutDashboard },
  { name: "SQL Studio", href: "/sql", icon: Terminal },
  { name: "Users", href: "/users", icon: Users },
  { name: "Namespaces", href: "/namespaces", icon: FolderTree },
  { name: "Live Queries", href: "/live-queries", icon: Wifi },
  { name: "Logging", href: "/logging", icon: FileText },
];

export default function Sidebar() {
  return (
    <aside className="flex w-64 flex-col border-r bg-card">
      <div className="flex h-16 items-center gap-2 border-b px-6">
        <Database className="h-6 w-6 text-primary" />
        <span className="text-lg font-semibold">KalamDB</span>
      </div>
      <nav className="flex-1 space-y-1 p-4">
        {navigation.map((item) => (
          <NavLink
            key={item.name}
            to={item.href}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                isActive
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              )
            }
          >
            <item.icon className="h-5 w-5" />
            {item.name}
          </NavLink>
        ))}
      </nav>
      <div className="border-t p-4 space-y-2">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            cn(
              "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              isActive
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            )
          }
        >
          <Settings className="h-5 w-5" />
          Settings
        </NavLink>
        <div className="text-xs text-muted-foreground px-3">
          KalamDB Admin v0.1.0
        </div>
      </div>
    </aside>
  );
}
