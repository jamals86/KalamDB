import Editor, { type Monaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { Button } from "@/components/ui/button";
import { Play, Square } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

interface EditorPanelProps {
  value: string;
  onChange: (value: string | undefined) => void;
  onMount: (editor: editor.IStandaloneCodeEditor, monaco: Monaco) => void;
  isLoading: boolean;
  isLive: boolean;
  onToggleLive: (checked: boolean) => void;
  onExecute: () => void;
  onStop: () => void;
  subscriptionStatus: 'idle' | 'connecting' | 'connected' | 'error';
  canExecute: boolean;
}

export function EditorPanel({
  value,
  onChange,
  onMount,
  isLoading,
  isLive,
  onToggleLive,
  onExecute,
  onStop,
  subscriptionStatus,
  canExecute,
}: EditorPanelProps) {
  return (
    <div className="h-full flex flex-col">
      {/* Editor Toolbar */}
      <div className="border-b flex items-center justify-between h-10 px-3 shrink-0 bg-muted/30">
        <div className="flex items-center gap-3">
          {/* Run/Stop Button */}
          <Button
            size="sm"
            onClick={isLive && subscriptionStatus === "connected" ? onStop : onExecute}
            disabled={isLoading || !canExecute || subscriptionStatus === "connecting"}
            className={cn(
              "gap-1.5 h-7",
              isLive && subscriptionStatus === "connected"
                ? "bg-red-500 hover:bg-red-600"
                : ""
            )}
          >
            {isLive && subscriptionStatus === "connected" ? (
              <>
                <Square className="h-3.5 w-3.5" />
                Stop
              </>
            ) : (
              <>
                <Play className="h-3.5 w-3.5" />
                {isLive ? "Subscribe" : "Run"}
              </>
            )}
          </Button>

          <div className="w-px h-5 bg-border" />

          {/* Live Query Toggle */}
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <div className="flex items-center gap-2">
                  <Switch
                    checked={isLive}
                    onCheckedChange={onToggleLive}
                    disabled={subscriptionStatus === "connected"}
                    className="scale-90"
                  />
                  <span className="text-xs font-medium">Live Query</span>
                </div>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="max-w-xs">
                {subscriptionStatus === "connected" ? (
                  <p>Stop the live query to toggle this option</p>
                ) : (
                  <p>Enable to receive real-time updates as data changes</p>
                )}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        
        <div className="text-xs text-muted-foreground">
          SQL Editor
        </div>
      </div>

      {/* Monaco Editor */}
      <div className="flex-1 overflow-hidden">
        <Editor
          height="100%"
          defaultLanguage="sql"
          theme="vs-dark"
          value={value}
          onChange={onChange}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            lineNumbers: "on",
            scrollBeyondLastLine: false,
            automaticLayout: true,
            tabSize: 2,
            wordWrap: "on",
            padding: { top: 8, bottom: 8 },
            lineHeight: 20,
          }}
          onMount={onMount}
        />
      </div>
    </div>
  );
}
