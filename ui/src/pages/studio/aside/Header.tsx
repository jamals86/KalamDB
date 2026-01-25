import { Plus, RefreshCw, Database } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface AsideHeaderProps {
  onCreateTable: () => void;
  onRefresh: () => void;
  isLoading: boolean;
}

export function AsideHeader({ onCreateTable, onRefresh, isLoading }: AsideHeaderProps) {
  return (
    <div className="p-2 border-b font-medium text-sm flex items-center justify-between">
      <div className="flex items-center gap-2">
        <Database className="h-4 w-4" />
        Schema Browser
      </div>
      <div className="flex items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          onClick={onCreateTable}
          className="h-6 w-6 p-0"
          title="Create new table"
        >
          <Plus className="h-3.5 w-3.5" />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={onRefresh}
          disabled={isLoading}
          className="h-6 w-6 p-0"
          title="Refresh schema"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", isLoading && "animate-spin")} />
        </Button>
      </div>
    </div>
  );
}
