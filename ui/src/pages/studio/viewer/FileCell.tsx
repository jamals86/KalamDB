interface FileRefData {
  id: string;
  sub: string;
  name: string;
  mime: string;
  size: number;
  sha256: string;
  shard?: number;
}

interface FileCellProps {
  fileRef: FileRefData;
  namespace?: string;
  tableName?: string;
}

function getFileIcon(mime: string): string {
  if (mime.startsWith('image/')) return '🖼️';
  if (mime.startsWith('video/')) return '🎥';
  if (mime.startsWith('audio/')) return '🎵';
  if (mime.includes('pdf')) return '📕';
  if (mime.includes('zip') || mime.includes('compressed')) return '📦';
  if (mime.includes('text')) return '📝';
  return '📄';
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function FileCell({ fileRef, namespace, tableName }: FileCellProps) {
  // Build download URL if we have table context
  // Format: /files/{namespace}/{table_name}/{subfolder}/{file_id}
  const downloadUrl = namespace && tableName 
    ? `/files/${namespace}/${tableName}/${fileRef.sub}/${fileRef.id}`
    : null;

  const fileInfo = `${fileRef.mime} • ${formatSize(fileRef.size)}`;

  if (downloadUrl) {
    return (
      <a
        href={downloadUrl}
        className="flex items-center gap-2 text-blue-600 hover:text-blue-800 hover:underline no-underline"
        title={`${fileInfo} • Click to download`}
        download={fileRef.name}
        onClick={(e) => e.stopPropagation()}
      >
        <span>{getFileIcon(fileRef.mime)}</span>
        <span className="font-medium">{fileRef.name}</span>
      </a>
    );
  }

  // Fallback: show info without download link
  return (
    <div className="flex items-center gap-2" title={fileInfo}>
      <span>{getFileIcon(fileRef.mime)}</span>
      <span className="font-medium text-blue-600">{fileRef.name}</span>
      <span className="text-xs text-muted-foreground">({formatSize(fileRef.size)})</span>
    </div>
  );
}
