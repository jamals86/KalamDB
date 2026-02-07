import { Download, File, FileText, Image, Video, Music, Archive } from 'lucide-react';
import { FileRef } from 'kalam-link';

interface FileDisplayProps {
  value: unknown;
  namespace?: string;
  tableName?: string;
  baseUrl?: string; // KalamDB server URL (defaults to window.location.origin)
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

export function FileDisplay({ value, namespace, tableName, baseUrl }: FileDisplayProps) {
  // Parse FileRef from value (handles JSON string or object)
  const fileRef = FileRef.from(value);
  
  if (!fileRef) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  const serverUrl = baseUrl || window.location.origin;

  // Build download URL if we have table context
  const downloadUrl = namespace && tableName 
    ? fileRef.getDownloadUrl(serverUrl, namespace, tableName)
    : null;

  return (
    <div className="flex items-center gap-2 max-w-[280px]">
      {getFileIcon(fileRef.mime)}
      {downloadUrl ? (
        <a
          href={downloadUrl}
          download={fileRef.name}
          className="text-blue-600 dark:text-blue-400 hover:underline truncate flex-1"
          onClick={(e) => e.stopPropagation()}
          title={`${fileRef.name} (${fileRef.formatSize()})`}
        >
          {fileRef.name}
        </a>
      ) : (
        <span className="truncate flex-1" title={`${fileRef.name} (${fileRef.formatSize()})`}>
          {fileRef.name}
        </span>
      )}
      <span className="text-xs text-muted-foreground shrink-0">
        {fileRef.formatSize()}
      </span>
      {downloadUrl && (
        <a
          href={downloadUrl}
          download={fileRef.name}
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
