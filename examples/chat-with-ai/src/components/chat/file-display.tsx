'use client';

import { useState } from 'react';
import { FileIcon, Download, ImageIcon, FileText, Film, Music, Archive } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { FileAttachment } from '@/types';

interface FileDisplayProps {
  files: FileAttachment[];
  className?: string;
}

export function FileDisplay({ files, className }: FileDisplayProps) {
  if (!files || files.length === 0) return null;

  return (
    <div className={cn('space-y-2 mt-2', className)}>
      {files.map((file) => (
        <FileItem key={file.id} file={file} />
      ))}
    </div>
  );
}

function FileItem({ file }: { file: FileAttachment }) {
  const [imageError, setImageError] = useState(false);
  const isImage = file.mime_type.startsWith('image/');
  const showImagePreview = isImage && !imageError;

  const handleDownload = async () => {
    try {
      const response = await fetch(file.url);
      const blob = await response.blob();
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = file.name;
      document.body.appendChild(a);
      a.click();
      window.URL.revokeObjectURL(url);
      document.body.removeChild(a);
    } catch (error) {
      console.error('Failed to download file:', error);
    }
  };

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  };

  const getFileIcon = () => {
    if (file.mime_type.startsWith('image/')) return ImageIcon;
    if (file.mime_type.startsWith('video/')) return Film;
    if (file.mime_type.startsWith('audio/')) return Music;
    if (file.mime_type.startsWith('text/')) return FileText;
    if (file.mime_type.includes('zip') || file.mime_type.includes('rar')) return Archive;
    return FileIcon;
  };

  const Icon = getFileIcon();

  return (
    <div className="border rounded-lg overflow-hidden bg-background/50">
      {/* Image Preview */}
      {showImagePreview && (
        <div className="relative max-w-[300px]">
          <img
            src={file.url}
            alt={file.name}
            className="w-full h-auto max-h-[300px] object-contain"
            onError={() => setImageError(true)}
          />
        </div>
      )}

      {/* File Info */}
      <div className="flex items-center gap-3 p-3">
        <div className="flex-shrink-0">
          <Icon className="h-5 w-5 text-muted-foreground" />
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium truncate">{file.name}</p>
          <p className="text-xs text-muted-foreground">
            {formatFileSize(file.size)} • {file.mime_type.split('/')[1]?.toUpperCase() || 'FILE'}
          </p>
        </div>
        <button
          onClick={handleDownload}
          className="flex-shrink-0 p-2 hover:bg-muted rounded-md transition-colors"
          title="Download file"
        >
          <Download className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
