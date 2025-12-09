import { useEffect, useState } from 'react';
import { useStorages, Storage } from '@/hooks/useStorages';
import { StorageForm } from './StorageForm';
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
import { Loader2, RefreshCw, Database, HardDrive, Cloud, Plus, Pencil } from 'lucide-react';

interface StorageListProps {
  onSelectStorage?: (storage: Storage) => void;
}

export function StorageList({ onSelectStorage }: StorageListProps) {
  const { storages, isLoading, error, fetchStorages } = useStorages();
  const [showForm, setShowForm] = useState(false);
  const [editingStorage, setEditingStorage] = useState<Storage | undefined>(undefined);

  useEffect(() => {
    fetchStorages();
  }, [fetchStorages]);

  const handleCreateNew = () => {
    setEditingStorage(undefined);
    setShowForm(true);
  };

  const handleEdit = (storage: Storage, e: React.MouseEvent) => {
    e.stopPropagation();
    setEditingStorage(storage);
    setShowForm(true);
  };

  const handleFormSuccess = () => {
    setShowForm(false);
    setEditingStorage(undefined);
    fetchStorages();
  };

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
      <div className="flex items-center justify-between">
        <Button onClick={handleCreateNew}>
          <Plus className="h-4 w-4 mr-2" />
          Create Storage
        </Button>
        <Button variant="outline" size="icon" onClick={fetchStorages} disabled={isLoading}>
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      {/* Storage Form Dialog */}
      <StorageForm
        open={showForm}
        onOpenChange={setShowForm}
        storage={editingStorage}
        onSuccess={handleFormSuccess}
      />

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
                <TableHead className="w-[80px]">Actions</TableHead>
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
                      <div>
                        <div>{storage.storage_name}</div>
                        <div className="text-xs text-muted-foreground font-mono">
                          {storage.storage_id}
                        </div>
                      </div>
                    </div>
                  </TableCell>
                  <TableCell>
                    <span className={`px-2 py-1 rounded-full text-xs font-medium ${getStorageTypeBadge(storage.storage_type)}`}>
                      {storage.storage_type}
                    </span>
                  </TableCell>
                  <TableCell className="text-muted-foreground font-mono text-sm max-w-[300px] truncate">
                    {storage.base_directory}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(storage.created_at).toLocaleDateString()}
                  </TableCell>
                  <TableCell>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={(e) => handleEdit(storage, e)}
                      title="Edit storage"
                    >
                      <Pencil className="h-4 w-4" />
                    </Button>
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
