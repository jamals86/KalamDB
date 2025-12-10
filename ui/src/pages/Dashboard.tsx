import { useEffect } from "react";
import { useAuth } from "@/lib/auth";
import { useStats } from "@/hooks/useStats";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Database, Users, HardDrive, FolderTree, Table2, Briefcase, Clock, RefreshCw, Loader2 } from "lucide-react";

export default function Dashboard() {
  const { user } = useAuth();
  const { stats, isLoading, error, fetchStats } = useStats();

  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  // Helper to format uptime
  const formatUptime = (seconds: string): string => {
    const secs = parseInt(seconds, 10);
    if (isNaN(secs)) return seconds;
    
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    const remainingSecs = secs % 60;
    
    const parts = [];
    if (days > 0) parts.push(`${days}d`);
    if (hours > 0) parts.push(`${hours}h`);
    if (mins > 0) parts.push(`${mins}m`);
    if (remainingSecs > 0 || parts.length === 0) parts.push(`${remainingSecs}s`);
    
    return parts.join(' ');
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Dashboard</h1>
          <p className="text-muted-foreground">
            Welcome back, {user?.username}
          </p>
        </div>
        <Button variant="outline" size="icon" onClick={fetchStats} disabled={isLoading}>
          {isLoading ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <RefreshCw className="h-4 w-4" />
          )}
        </Button>
      </div>

      {error && (
        <Card className="border-red-200">
          <CardContent className="py-4">
            <p className="text-red-700 text-sm">{error}</p>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Server</CardTitle>
            <Database className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">KalamDB</div>
            <p className="text-xs text-muted-foreground">
              {stats.server_version || 'v0.1.1'} • Running
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Uptime</CardTitle>
            <Clock className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {stats.server_uptime_human || (stats.server_uptime_seconds ? formatUptime(stats.server_uptime_seconds) : '-')}
            </div>
            <p className="text-xs text-muted-foreground">
              Server uptime
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Users</CardTitle>
            <Users className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total_users || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Total users
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Namespaces</CardTitle>
            <FolderTree className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total_namespaces || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Total namespaces
            </p>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Tables</CardTitle>
            <Table2 className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total_tables || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Total tables
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Jobs</CardTitle>
            <Briefcase className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total_jobs || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Total jobs
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Storages</CardTitle>
            <HardDrive className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.total_storages || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Storage backends
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Connections</CardTitle>
            <Users className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{stats.active_connections || '-'}</div>
            <p className="text-xs text-muted-foreground">
              Active connections
            </p>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Quick Start</CardTitle>
          <CardDescription>
            Common actions to get started with KalamDB administration
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2">
          <p className="text-sm">
            • Use <strong>SQL Studio</strong> to execute queries and explore data
          </p>
          <p className="text-sm">
            • Manage <strong>Users</strong> to create and configure database users
          </p>
          <p className="text-sm">
            • View <strong>Storages</strong> to see storage configurations
          </p>
          <p className="text-sm">
            • Browse <strong>Namespaces</strong> to see tables and their schemas
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
