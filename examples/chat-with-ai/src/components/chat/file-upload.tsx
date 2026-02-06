'use client';

import { useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Paperclip, X, FileIcon, ImageIcon } from 'lucide-react';
import type { UploadProgress } from 'kalam-link';
import { cn } from '@/lib/utils';

interface FileUploadProps {
  files: File[];
  onFilesChange: (files: File[]) => void;
  maxFiles?: number;
  maxSizeMB?: number;
  disabled?: boolean;
  uploadProgress?: UploadProgress | null;
}

export function FileUpload({ 
  files, 
  onFilesChange, 
  maxFiles = 10, 
  maxSizeMB = 10,
  disabled = false,
  uploadProgress = null,
}: FileUploadProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const selectedFiles = Array.from(e.target.files || []);
    setError(null);

    // Validate file count
    if (files.length + selectedFiles.length > maxFiles) {
      setError(`Maximum ${maxFiles} files allowed`);
      return;
    }

    // Validate file sizes
    const maxSizeBytes = maxSizeMB * 1024 * 1024;
    const oversized = selectedFiles.filter(f => f.size > maxSizeBytes);
    if (oversized.length > 0) {
      setError(`Files must be under ${maxSizeMB}MB`);
      return;
    }

    onFilesChange([...files, ...selectedFiles]);
    
    // Reset input
    if (inputRef.current) {
      inputRef.current.value = '';
    }
  };

  const handleRemove = (index: number) => {
    onFilesChange(files.filter((_, i) => i !== index));
    setError(null);
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  };

  const isImage = (file: File) => file.type.startsWith('image/');
  const getProgressForFile = (file: File) => {
    if (!uploadProgress) return null;
    if (uploadProgress.file_name !== file.name) return null;
    return uploadProgress;
  };

  return (
    <div className="space-y-2">
      {/* File Attachments Display */}
      {files.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {files.map((file, index) => {
            const progress = getProgressForFile(file);
            return (
              <div
                key={index}
                className={cn(
                  'flex flex-col gap-1 px-3 py-1.5 rounded-lg border bg-muted/50',
                  'text-xs',
                  isImage(file) && 'border-blue-200 bg-blue-50 dark:bg-blue-950/20'
                )}
              >
                <div className="flex items-center gap-2">
                  {isImage(file) ? (
                    <ImageIcon className="h-3.5 w-3.5 text-blue-500" />
                  ) : (
                    <FileIcon className="h-3.5 w-3.5 text-muted-foreground" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p className="truncate max-w-[150px] font-medium">{file.name}</p>
                    <p className="text-[10px] text-muted-foreground">{formatFileSize(file.size)}</p>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleRemove(index)}
                    className="p-0.5 hover:bg-background rounded"
                    disabled={disabled}
                  >
                    <X className="h-3 w-3" />
                  </button>
                </div>
                {progress && (
                  <div className="w-full">
                    <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                      <div
                        className="h-full bg-primary transition-all"
                        style={{ width: `${progress.percent}%` }}
                      />
                    </div>
                    <p className="text-[10px] text-muted-foreground mt-0.5">
                      Uploading {Math.round(progress.percent)}%
                    </p>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Error Message */}
      {error && (
        <p className="text-xs text-red-500">{error}</p>
      )}

      {/* Upload Button */}
      {files.length < maxFiles && (
        <>
          <input
            ref={inputRef}
            type="file"
            multiple
            onChange={handleFileSelect}
            disabled={disabled}
            className="hidden"
            accept="*/*"
          />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => inputRef.current?.click()}
            disabled={disabled}
            className="h-7 px-2 text-xs"
          >
            <Paperclip className="h-3.5 w-3.5 mr-1" />
            Attach Files ({files.length}/{maxFiles})
          </Button>
        </>
      )}
    </div>
  );
}
