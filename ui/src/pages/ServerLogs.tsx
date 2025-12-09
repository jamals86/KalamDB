import { ServerLogList } from "@/components/logs/ServerLogList";

export default function ServerLogs() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Server Logs</h1>
        <p className="text-muted-foreground">
          View real-time server logs and debug information
        </p>
      </div>

      <ServerLogList />
    </div>
  );
}
