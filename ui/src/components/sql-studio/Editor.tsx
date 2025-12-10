import { useRef, useCallback } from 'react';
import MonacoEditor, { OnMount, OnChange } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { Button } from '../ui/button';

interface EditorProps {
  value: string;
  onChange: (value: string) => void;
  onExecute: () => void;
  isExecuting: boolean;
  onCancel?: () => void;
  executionTime?: number | null;
}

export function Editor({
  value,
  onChange,
  onExecute,
  isExecuting,
  onCancel,
  executionTime,
}: EditorProps) {
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);

  const handleEditorMount: OnMount = useCallback((editor) => {
    editorRef.current = editor;
    
    // Add keyboard shortcut for execution (Ctrl/Cmd + Enter)
    editor.addCommand(
      // Monaco.KeyMod.CtrlCmd | Monaco.KeyCode.Enter
      2048 | 3, // CtrlCmd + Enter
      () => {
        onExecute();
      }
    );
  }, [onExecute]);

  const handleEditorChange: OnChange = useCallback((newValue) => {
    onChange(newValue || '');
  }, [onChange]);

  const formatExecutionTime = (ms: number) => {
    if (ms < 1000) {
      return `${ms.toFixed(0)}ms`;
    }
    return `${(ms / 1000).toFixed(2)}s`;
  };

  return (
    <div className="flex flex-col h-full border rounded-lg overflow-hidden">
      <div className="flex items-center justify-between p-2 bg-gray-50 border-b">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-gray-600">SQL Editor</span>
          <span className="text-xs text-gray-400">Ctrl/Cmd + Enter to execute</span>
        </div>
        <div className="flex items-center gap-2">
          {executionTime !== null && executionTime !== undefined && (
            <span className="text-xs text-gray-500">
              Executed in {formatExecutionTime(executionTime)}
            </span>
          )}
          {isExecuting ? (
            <Button
              size="sm"
              variant="destructive"
              onClick={onCancel}
            >
              Cancel
            </Button>
          ) : (
            <Button
              size="sm"
              onClick={onExecute}
              disabled={!value.trim()}
            >
              Execute
            </Button>
          )}
        </div>
      </div>
      <div className="flex-1 min-h-[200px]">
        <MonacoEditor
          height="100%"
          language="sql"
          theme="vs-light"
          value={value}
          onChange={handleEditorChange}
          onMount={handleEditorMount}
          options={{
            minimap: { enabled: false },
            fontSize: 14,
            lineNumbers: 'on',
            scrollBeyondLastLine: false,
            automaticLayout: true,
            tabSize: 2,
            wordWrap: 'on',
            padding: { top: 8, bottom: 8 },
            suggestOnTriggerCharacters: true,
            quickSuggestions: true,
          }}
        />
      </div>
    </div>
  );
}
