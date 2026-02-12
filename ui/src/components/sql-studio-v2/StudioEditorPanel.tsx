import { useEffect, useMemo, useState } from "react";
import Editor, { type Monaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { MoreHorizontal, PenLine, Play, Save, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface StudioEditorPanelProps {
  tabTitle: string;
  lastSavedAt: string | null;
  isLive: boolean;
  liveStatus: "idle" | "connecting" | "connected" | "error";
  sql: string;
  isRunning: boolean;
  onSqlChange: (value: string) => void;
  onRun: () => void;
  onToggleLive: (checked: boolean) => void;
  onRename: (title: string) => void;
  onSave: () => void;
  onSaveCopy: () => void;
  onDelete: () => void;
}

export function StudioEditorPanel({
  tabTitle,
  lastSavedAt,
  isLive,
  liveStatus,
  sql,
  isRunning,
  onSqlChange,
  onRun,
  onToggleLive,
  onRename,
  onSave,
  onSaveCopy,
  onDelete,
}: StudioEditorPanelProps) {
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [draftTitle, setDraftTitle] = useState(tabTitle);

  useEffect(() => {
    if (!isEditingTitle) {
      setDraftTitle(tabTitle);
    }
  }, [tabTitle, isEditingTitle]);

  const handleEditorMount = (instance: editor.IStandaloneCodeEditor, monaco: Monaco) => {
    instance.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => {
      onRun();
    });
  };

  const lastSavedLabel = useMemo(() => {
    if (!lastSavedAt) {
      return "Never saved";
    }

    const savedAt = new Date(lastSavedAt);
    const now = Date.now();
    const diffMs = now - savedAt.getTime();
    if (Number.isNaN(savedAt.getTime()) || diffMs < 0) {
      return "Last saved just now";
    }

    const diffMinutes = Math.floor(diffMs / (1000 * 60));
    if (diffMinutes < 1) {
      return "Last saved just now";
    }
    if (diffMinutes < 60) {
      return `Last saved ${diffMinutes}m ago`;
    }

    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) {
      return `Last saved ${diffHours}h ago`;
    }

    return `Last saved ${savedAt.toLocaleString()}`;
  }, [lastSavedAt]);

  const commitRename = () => {
    const normalizedTitle = draftTitle.trim();
    if (normalizedTitle.length > 0 && normalizedTitle !== tabTitle) {
      onRename(normalizedTitle);
    }
    setIsEditingTitle(false);
  };

  return (
    <div className="flex h-full flex-col bg-white dark:bg-[#101922]">
      <div className="flex items-center justify-between border-b border-slate-200 px-4 py-2 dark:border-[#1e293b]">
        <div className="min-w-0">
          {isEditingTitle ? (
            <input
              value={draftTitle}
              onChange={(event) => setDraftTitle(event.target.value)}
              onBlur={commitRename}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  commitRename();
                }
                if (event.key === "Escape") {
                  setIsEditingTitle(false);
                }
              }}
              autoFocus
              className="h-7 w-full max-w-[320px] rounded border border-slate-300 bg-white px-2 text-base font-semibold text-slate-900 outline-none ring-2 ring-sky-400 dark:border-[#334155] dark:bg-[#0f172a] dark:text-slate-100"
            />
          ) : (
            <div className="flex items-center gap-1.5">
              <p className="truncate text-base font-semibold text-slate-800 dark:text-slate-100">{tabTitle}</p>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-6 w-6 shrink-0 text-slate-500 hover:text-slate-200"
                onClick={() => setIsEditingTitle(true)}
                title="Rename query"
              >
                <PenLine className="h-3.5 w-3.5" />
              </Button>
            </div>
          )}
          <p className="text-xs text-slate-500 dark:text-slate-400">Draft</p>
        </div>
        <div className="flex items-center gap-2">
          <div className="hidden items-center gap-2 lg:flex">
            <div className="flex items-center gap-2 border-r border-slate-200 pr-3 dark:border-[#334155]">
              <Switch
                checked={isLive}
                onCheckedChange={onToggleLive}
                disabled={liveStatus === "connecting"}
              />
              <span className="text-xs text-slate-500 dark:text-slate-400">Live query</span>
            </div>
            <span className="text-xs text-slate-500 dark:text-slate-400">{lastSavedLabel}</span>
          </div>
          <Button
            variant="secondary"
            size="sm"
            className="shrink-0"
            onClick={onSave}
          >
            <Save className="mr-1.5 h-3.5 w-3.5" />
            Save
          </Button>
          <Button
            size="sm"
            className="shrink-0 bg-[#137fec] text-white hover:bg-[#0f6cbd]"
            onClick={onRun}
            disabled={isRunning || !sql.trim() || liveStatus === "connecting"}
          >
            {isLive && liveStatus === "connected" ? (
              <Square className="mr-1.5 h-3.5 w-3.5" />
            ) : (
              <Play className="mr-1.5 h-3.5 w-3.5" />
            )}
            {isLive && liveStatus === "connected"
              ? "Stop"
              : isRunning
                ? "Running..."
                : isLive
                  ? "Subscribe"
                  : "Run query"}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="secondary"
                size="icon"
                className="h-8 w-8"
              >
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setIsEditingTitle(true)}>Rename</DropdownMenuItem>
              <DropdownMenuItem onSelect={onSaveCopy}>Save a copy</DropdownMenuItem>
              <DropdownMenuItem onSelect={onDelete} className="text-destructive">Delete</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <div className="flex-1 overflow-hidden">
        <Editor
          height="100%"
          defaultLanguage="sql"
          theme="vs-dark"
          value={sql}
          onChange={(value) => onSqlChange(value ?? "")}
          onMount={handleEditorMount}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            lineNumbers: "on",
            lineNumbersMinChars: 3,
            automaticLayout: true,
            wordWrap: "on",
            scrollBeyondLastLine: false,
            padding: { top: 12 },
            fontFamily: "JetBrains Mono, monospace",
          }}
        />
      </div>
    </div>
  );
}
