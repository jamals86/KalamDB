import { useParams, useNavigate } from "react-router-dom";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAuth } from "@/lib/auth";
import { SettingsView } from "@/components/settings/SettingsView";
import Cluster from "./Cluster";
import Storages from "./Storages";
import { cn } from "@/lib/utils";
import { PageLayout } from "@/components/layout/PageLayout";
import {
  Settings as SettingsIcon,
  Sliders,
  Network,
  HardDrive,
  Archive,
  Shield,
} from "lucide-react";

type SettingsSection = "all" | "general" | "cluster" | "storages" | "backup" | "security";

const sections = [
  { id: "all" as const, name: "All Settings", icon: SettingsIcon },
  { id: "general" as const, name: "General", icon: Sliders },
  { id: "cluster" as const, name: "Cluster", icon: Network },
  { id: "storages" as const, name: "Storages", icon: HardDrive },
  { id: "backup" as const, name: "Backup", icon: Archive },
  { id: "security" as const, name: "Security", icon: Shield },
];

export default function Settings() {
  const { user } = useAuth();
  const { category } = useParams<{ category?: string }>();
  const navigate = useNavigate();
  
  // Determine active section from URL parameter
  const activeSection: SettingsSection = (category as SettingsSection) || "all";
  
  // Update URL when section changes
  const setActiveSection = (section: SettingsSection) => {
    if (section === "all") {
      navigate("/settings");
    } else {
      navigate(`/settings/${section}`);
    }
  };

  const renderContent = () => {
    switch (activeSection) {
      case "all":
        return <SettingsView />;
      case "general":
        return (
          <Card>
            <CardHeader>
              <CardTitle>General Settings</CardTitle>
              <CardDescription>General configuration options</CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground text-sm">No settings available yet.</p>
            </CardContent>
          </Card>
        );
      case "cluster":
        return <Cluster />;
      case "storages":
        return <Storages />;
      case "backup":
        return (
          <Card>
            <CardHeader>
              <CardTitle>Backup Settings</CardTitle>
              <CardDescription>Configure backup and restore options</CardDescription>
            </CardHeader>
            <CardContent>
              <SettingsView filterCategory="backup" />
            </CardContent>
          </Card>
        );
      case "security":
        return (
          <Card>
            <CardHeader>
              <CardTitle>Security Settings</CardTitle>
              <CardDescription>Configure authentication and authorization</CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground text-sm">No settings available yet.</p>
            </CardContent>
          </Card>
        );
      default:
        return null;
    }
  };

  return (
    <PageLayout
      title="Settings"
      description="View and configure system settings"
      className="flex h-full min-h-0 flex-col"
      contentClassName="min-h-0 flex-1"
    >
      <div className="flex h-full min-h-0 overflow-hidden rounded-lg border bg-card">
        {/* Settings sidebar */}
        <div className="w-56 shrink-0 border-r bg-muted/20 p-4 space-y-1">
          {sections.map((section) => (
            <button
              key={section.id}
              onClick={() => setActiveSection(section.id)}
              className={cn(
                "w-full flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors text-left",
                activeSection === section.id
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              )}
            >
              <section.icon className="h-4 w-4" />
              {section.name}
            </button>
          ))}
        </div>

        {/* Settings content */}
        <div className="flex-1 overflow-auto p-6">
          {activeSection === "all" ? (
            <div className="space-y-6 max-w-4xl">
              <Card>
                <CardHeader>
                  <CardTitle>Current User</CardTitle>
                  <CardDescription>Your current session information</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                  <div className="grid grid-cols-2 gap-2">
                    <span className="text-muted-foreground">Username:</span>
                    <span>{user?.username}</span>
                    <span className="text-muted-foreground">Role:</span>
                    <span>{user?.role}</span>
                    <span className="text-muted-foreground">Email:</span>
                    <span>{user?.email || "-"}</span>
                    <span className="text-muted-foreground">User ID:</span>
                    <span className="font-mono text-xs">{user?.id}</span>
                  </div>
                </CardContent>
              </Card>

              {renderContent()}
            </div>
          ) : (
            renderContent()
          )}
        </div>
      </div>
    </PageLayout>
  );
}
