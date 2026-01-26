import { Download, File, FileText, Image, Video, Music, Archive } from 'lucide-react';

interface FileRef {
  id: string;
  sub: string;
  name: string;
  mime: string;
  size: number;
  sha256?: string;
}

interface FileDisplayProps {
  value: FileRef;
  namespace?: string;
  tableName?: string;
}

function getFileIcon(mime: string) {
  if (mime.startsWith('image/')) return <Image className="h-4 w-4 text-blue-500" />;
  if (mime.startsWith('video/')) return <Video className="h-4 w-4 text-purple-500" />;
  if (mime.startsWith('audio/')) return <Music className="h-4 w-4 text-green-500" />;
  if (mime.startsWith('text/')) return <FileText className="h-4 w-4 text-gray-500" />;
  if (mime.includes('zip') || mime.includes('tar') || mime.includes('compressed')) {
    return <Archive className="h-4 w-4 text-orange-500" />;
  }
  return <File className="h-4 w-4 text-gray-400" />;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function FileDisplay({ value, namespace, tableName }: FileDisplayProps) {
  if (!value || typeof value !== 'object') {
    return <span className="text-muted-foreground italic">null</span>;
  }

  const { id, sub, name, mime, size } = value;

  // Build download URL if we have table context
  const downloadUrl = namespace && tableName 
    ? `/files/${namespace}/${tableName}/${sub}/${id}`
    : null;

  return (
    <div className="flex items-center gap-2 max-w-[280px]">
      {getFileIcon(mime)}
      {downloadUrl ? (
        <a
          href={downloadUrl}
          download={name}
          className="text-blue-600 dark:text-blue-400 hover:underline truncate flex-1"
          onClick={(e) => e.stopPropagation()}
          title={`${name} (${formatSize(size)})`}
        >
          {name}
        </a>
      ) : (
        <span className="truncate flex-1" title={`${name} (${formatSize(size)})`}>
          {name}
        </span>
      )}
      <span className="text-xs text-muted-foreground shrink-0">
        {formatSize(size)}
      </span>
      {downloadUrl && (
        <a
          href={downloadUrl}
          download={name}
          className="text-muted-foreground hover:text-foreground shrink-0"
          onClick={(e) => e.stopPropagation()}
          title="Download"
        >
          <Download className="h-3.5 w-3.5" />
        </a>
      )}
    </div>
  );
}
