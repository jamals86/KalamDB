import { AuditLogList } from "@/components/audit/AuditLogList";

export default function AuditLogs() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Audit Logs</h1>
        <p className="text-muted-foreground">
          View system audit trail and activity history
        </p>
      </div>

      <AuditLogList />
    </div>
  );
}
