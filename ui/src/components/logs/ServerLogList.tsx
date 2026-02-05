import { useEffect, useState } from 'react';
import { useServerLogs, ServerLog, ServerLogFilters } from '@/hooks/useServerLogs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardContent } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Loader2, RefreshCw, Filter, X, Eye, AlertCircle, AlertTriangle, Info, Bug } from 'lucide-react';
import { formatTimestamp } from '@/lib/formatters';

const LEVEL_CONFIG: Record<string, { color: string; icon: typeof AlertCircle }> = {
  'ERROR': { color: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400', icon: AlertCircle },
  'WARN': { color: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400', icon: AlertTriangle },
  'INFO': { color: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400', icon: Info },
  'DEBUG': { color: 'bg-gray-100 text-gray-800 dark:bg-gray-800 dark:text-gray-400', icon: Bug },
  'TRACE': { color: 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400', icon: Bug },
};

function getLevelConfig(level: string) {
  return LEVEL_CONFIG[level.toUpperCase()] || { color: 'bg-gray-100 text-gray-800', icon: Info };
}

export function ServerLogList() {
  const { logs, isLoading, error, fetchLogs } = useServerLogs();
  const [showFilters, setShowFilters] = useState(false);
  const [selectedLog, setSelectedLog] = useState<ServerLog | null>(null);
  const [filters, setFilters] = useState<ServerLogFilters>({
    limit: 500,
  });
  const [autoRefresh, setAutoRefresh] = useState(false);

  useEffect(() => {
    fetchLogs(filters);
  }, []);

  // Auto-refresh every 5 seconds if enabled
  useEffect(() => {
    if (!autoRefresh) return;
    
    const interval = setInterval(() => {
      fetchLogs(filters);
    }, 5000);
    
    return () => clearInterval(interval);
  }, [autoRefresh, filters, fetchLogs]);

  const handleApplyFilters = () => {
    fetchLogs(filters);
    setShowFilters(false);
  };

  const handleClearFilters = () => {
    const clearedFilters = { limit: 500 };
    setFilters(clearedFilters);
    fetchLogs(clearedFilters);
    setShowFilters(false);
  };

  const hasActiveFilters = filters.level || filters.target || filters.message;

  if (error) {
    return (
      <Card className="border-red-200">
        <CardContent className="py-6">
          <div className="flex items-center gap-2 text-red-700">
            <AlertCircle className="h-5 w-5" />
            <p>{error}</p>
          </div>
          <p className="text-sm text-muted-foreground mt-2">
            Make sure logging format is set to "json" in server configuration.
          </p>
          <Button variant="outline" onClick={() => fetchLogs(filters)} className="mt-4">
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center gap-2">
          <Button
            variant={showFilters ? 'secondary' : 'outline'}
            onClick={() => setShowFilters(!showFilters)}
          >
            <Filter className="h-4 w-4 mr-2" />
            Filters
            {hasActiveFilters && (
              <span className="ml-2 px-1.5 py-0.5 bg-primary text-primary-foreground rounded-full text-xs">
                Active
              </span>
            )}
          </Button>
          {hasActiveFilters && (
            <Button variant="ghost" size="sm" onClick={handleClearFilters}>
              <X className="h-4 w-4 mr-1" />
              Clear
            </Button>
          )}
        </div>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded"
            />
            Auto-refresh
          </label>
          <span className="text-sm text-muted-foreground">
            {logs.length} log{logs.length !== 1 ? 's' : ''}
          </span>
          <Button variant="outline" size="icon" onClick={() => fetchLogs(filters)} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <Card>
          <CardContent className="pt-4">
            <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
              <div className="space-y-1">
                <label className="text-sm font-medium">Level</label>
                <Select
                  value={filters.level || 'all'}
                  onValueChange={(value: string) => setFilters({ ...filters, level: value === 'all' ? undefined : value })}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="All levels" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Levels</SelectItem>
                    <SelectItem value="ERROR">ERROR</SelectItem>
                    <SelectItem value="WARN">WARN</SelectItem>
                    <SelectItem value="INFO">INFO</SelectItem>
                    <SelectItem value="DEBUG">DEBUG</SelectItem>
                    <SelectItem value="TRACE">TRACE</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-1">
                <label className="text-sm font-medium">Target/Module</label>
                <Input
                  placeholder="e.g., kalamdb_core"
                  value={filters.target || ''}
                  onChange={(e) => setFilters({ ...filters, target: e.target.value || undefined })}
                />
              </div>
              <div className="space-y-1">
                <label className="text-sm font-medium">Message Contains</label>
                <Input
                  placeholder="Search in message"
                  value={filters.message || ''}
                  onChange={(e) => setFilters({ ...filters, message: e.target.value || undefined })}
                />
              </div>
              <div className="space-y-1">
                <label className="text-sm font-medium">Limit</label>
                <Select
                  value={String(filters.limit || 500)}
                  onValueChange={(value: string) => setFilters({ ...filters, limit: parseInt(value) })}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="100">100</SelectItem>
                    <SelectItem value="500">500</SelectItem>
                    <SelectItem value="1000">1000</SelectItem>
                    <SelectItem value="5000">5000</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="outline" onClick={handleClearFilters}>
                Clear
              </Button>
              <Button onClick={handleApplyFilters}>
                Apply Filters
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Logs Table */}
      <Card>
        <CardContent className="p-0">
          {isLoading && logs.length === 0 ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
            </div>
          ) : logs.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              <p>No logs found</p>
              <p className="text-sm mt-1">
                Logs require format = "json" in server configuration
              </p>
            </div>
          ) : (
            <div className="font-mono text-xs overflow-x-auto">
              {logs.map((log, index) => {
                const levelConfig = getLevelConfig(log.level);
                const LevelIcon = levelConfig.icon;
                return (
                  <div 
                    key={`${log.timestamp}-${index}`} 
                    className="flex items-start gap-3 px-4 py-1.5 hover:bg-muted/50 border-b border-border/30 last:border-b-0 group"
                  >
                    <span className="text-muted-foreground shrink-0 w-[180px] whitespace-nowrap">
                      {formatTimestamp(log.timestamp, 'Timestamp(Microsecond, None)')}
                    </span>
                    <span className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium shrink-0 ${levelConfig.color}`}>
                      <LevelIcon className="h-2.5 w-2.5" />
                      {log.level}
                    </span>
                    <span className="text-muted-foreground shrink-0 w-[120px] truncate" title={log.target || '-'}>
                      {log.target || '-'}
                    </span>
                    <span className="flex-1 min-w-0 break-words">
                      {log.message}
                    </span>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity"
                      onClick={() => setSelectedLog(log)}
                      title="View details"
                    >
                      <Eye className="h-3 w-3" />
                    </Button>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Log Detail Dialog */}
      <Dialog open={!!selectedLog} onOpenChange={() => setSelectedLog(null)}>
        <DialogContent className="max-w-3xl max-h-[80vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              Log Details
              {selectedLog && (
                <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${getLevelConfig(selectedLog.level).color}`}>
                  {selectedLog.level}
                </span>
              )}
            </DialogTitle>
            <DialogDescription>
              {selectedLog && formatTimestamp(selectedLog.timestamp)}
            </DialogDescription>
          </DialogHeader>
          {selectedLog && (
            <div className="space-y-4 mt-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="text-sm font-medium text-muted-foreground">Thread</label>
                  <p className="font-mono text-sm">{selectedLog.thread || '-'}</p>
                </div>
                <div>
                  <label className="text-sm font-medium text-muted-foreground">Target</label>
                  <p className="font-mono text-sm">{selectedLog.target || '-'}</p>
                </div>
                <div>
                  <label className="text-sm font-medium text-muted-foreground">Line</label>
                  <p className="font-mono text-sm">{selectedLog.line ?? '-'}</p>
                </div>
                <div>
                  <label className="text-sm font-medium text-muted-foreground">Level</label>
                  <p className="font-mono text-sm">{selectedLog.level}</p>
                </div>
              </div>
              <div>
                <label className="text-sm font-medium text-muted-foreground">Message</label>
                <pre className="mt-1 p-3 bg-muted rounded-md font-mono text-sm whitespace-pre-wrap break-words overflow-x-auto">
                  {selectedLog.message}
                </pre>
              </div>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
