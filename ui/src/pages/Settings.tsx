import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useAuth } from "@/lib/auth";
import { SettingsView } from "@/components/settings/SettingsView";

export default function Settings() {
  const { user } = useAuth();

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Settings</h1>
        <p className="text-muted-foreground">
          View system settings and configuration
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Current User</CardTitle>
          <CardDescription>
            Your current session information
          </CardDescription>
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

      <SettingsView />
    </div>
  );
}
