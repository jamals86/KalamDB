import { useEffect, useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
  Briefcase,
  ChevronLeft,
  ChevronRight,
  FileText,
  LayoutDashboard,
  RadioTower,
  Settings,
  Terminal,
  Users,
  Wifi,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

const SIDEBAR_COLLAPSED_STORAGE_KEY = "kalamdb-admin-sidebar-collapsed";

const navigation = [
  { name: "Dashboard", href: "/dashboard", icon: LayoutDashboard },
  { name: "SQL Studio", href: "/sql", icon: Terminal },
  { name: "Streaming", href: "/streaming/topics", icon: RadioTower },
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
  const location = useLocation();

  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, collapsed ? "1" : "0");
  }, [collapsed]);

  return (
    <TooltipProvider delayDuration={0}>
      <aside
        className={cn(
          "flex h-full min-h-0 shrink-0 flex-col border-r bg-background transition-[width] duration-200",
          collapsed ? "w-14" : "w-64",
        )}
      >
        <nav className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
          {navigation.map((item) => {
            const isActive = location.pathname.startsWith(item.href);
            console.log("Sidebar item:", item.name, "href:", item.href, "pathname:", location.pathname, "isActive:", isActive);
            return (
              <Tooltip key={item.name} disableHoverableContent>
                <TooltipTrigger asChild>
                  <NavLink
                    to={item.href}
                    className={cn(
                      "flex items-center w-full rounded-md transition-colors",
                      collapsed ? "h-9 w-9 justify-center" : "h-9 px-3 justify-start",
                      isActive
                        ? "bg-primary/10 text-primary font-medium"
                        : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                    )}
                  >
                    <item.icon className={cn("h-4 w-4 shrink-0", !collapsed && "mr-2")} />
                    {!collapsed && <span className="truncate">{item.name}</span>}
                  </NavLink>
                </TooltipTrigger>
                {collapsed && (
                  <TooltipContent side="right" className="flex items-center gap-4">
                    {item.name}
                  </TooltipContent>
                )}
              </Tooltip>
            );
          })}
        </nav>

        <div className="shrink-0 border-t p-2 space-y-1">
          <Tooltip disableHoverableContent>
            <TooltipTrigger asChild>
              <NavLink
                to="/settings"
                className={cn(
                  "flex items-center w-full rounded-md transition-colors",
                  collapsed ? "h-9 w-9 justify-center" : "h-9 px-3 justify-start",
                  location.pathname.startsWith("/settings")
                    ? "bg-primary/10 text-primary font-medium"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                )}
              >
                <Settings className={cn("h-4 w-4 shrink-0", !collapsed && "mr-2")} />
                {!collapsed && <span className="truncate">Settings</span>}
              </NavLink>
            </TooltipTrigger>
            {collapsed && (
              <TooltipContent side="right" className="flex items-center gap-4">
                Settings
              </TooltipContent>
            )}
          </Tooltip>

          <Tooltip disableHoverableContent>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                className={cn(
                  "w-full justify-start text-muted-foreground",
                  collapsed ? "h-9 w-9 p-0 justify-center" : "px-3"
                )}
                onClick={() => setCollapsed((prev) => !prev)}
              >
                {collapsed ? (
                  <ChevronRight className="h-4 w-4" />
                ) : (
                  <ChevronLeft className="h-4 w-4 mr-2" />
                )}
                {!collapsed && <span>Collapse</span>}
              </Button>
            </TooltipTrigger>
            {collapsed && (
              <TooltipContent side="right" className="flex items-center gap-4">
                Expand
              </TooltipContent>
            )}
          </Tooltip>
        </div>
      </aside>
    </TooltipProvider>
  );
}
