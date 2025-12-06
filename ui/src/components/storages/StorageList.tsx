import { useEffect } from 'react';
import { useStorages, Storage } from '@/hooks/useStorages';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Loader2, RefreshCw, Database, HardDrive, Cloud } from 'lucide-react';

interface StorageListProps {
  onSelectStorage?: (storage: Storage) => void;
}

export function StorageList({ onSelectStorage }: StorageListProps) {
  const { storages, isLoading, error, fetchStorages } = useStorages();

  useEffect(() => {
    fetchStorages();
  }, [fetchStorages]);

  const getStorageIcon = (type: string) => {
    switch (type.toLowerCase()) {
      case 's3':
      case 'cloud':
        return <Cloud className="h-4 w-4" />;
      case 'local':
      case 'filesystem':
        return <HardDrive className="h-4 w-4" />;
      default:
        return <Database className="h-4 w-4" />;
    }
  };

  const getStorageTypeBadge = (type: string) => {
    switch (type.toLowerCase()) {
      case 's3':
        return 'bg-orange-100 text-orange-800';
      case 'local':
      case 'filesystem':
        return 'bg-blue-100 text-blue-800';
      default:
        return 'bg-gray-100 text-gray-800';
    }
  };

  if (error) {
    return (
      <Card className="border-red-200">
        <CardContent className="py-6">
          <p className="text-red-700">{error}</p>
          <Button variant="outline" onClick={fetchStorages} className="mt-2">
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center justify-end">
        <Button variant="outline" size="icon" onClick={fetchStorages} disabled={isLoading}>
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      {/* Table */}
      {isLoading && storages.length === 0 ? (
        <div className="flex items-center justify-center py-8">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </div>
      ) : storages.length === 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>No Storages Found</CardTitle>
            <CardDescription>
              No storage configurations are available.
            </CardDescription>
          </CardHeader>
        </Card>
      ) : (
        <div className="border rounded-lg">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Path</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {storages.map((storage, index) => (
                <TableRow
                  key={storage.storage_id || `storage-${index}`}
                  className={onSelectStorage ? 'cursor-pointer' : ''}
                  onClick={() => onSelectStorage?.(storage)}
                >
                  <TableCell className="font-medium">
                    <div className="flex items-center gap-2">
                      {getStorageIcon(storage.storage_type)}
                      {storage.storage_name}
                    </div>
                  </TableCell>
                  <TableCell>
                    <span className={`px-2 py-1 rounded-full text-xs font-medium ${getStorageTypeBadge(storage.storage_type)}`}>
                      {storage.storage_type}
                    </span>
                  </TableCell>
                  <TableCell className="text-muted-foreground font-mono text-sm">
                    {storage.base_directory}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(storage.created_at).toLocaleDateString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
