import { useEffect, useState } from 'react';
import { useNamespaces, Namespace } from '@/hooks/useNamespaces';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Loader2, RefreshCw, Plus, FolderOpen, Search } from 'lucide-react';

interface NamespaceListProps {
  onSelectNamespace?: (namespace: Namespace) => void;
}

export function NamespaceList({ onSelectNamespace }: NamespaceListProps) {
  const { namespaces, isLoading, error, fetchNamespaces, createNamespace } = useNamespaces();
  const [searchQuery, setSearchQuery] = useState('');
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [newNamespaceName, setNewNamespaceName] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  useEffect(() => {
    fetchNamespaces();
  }, [fetchNamespaces]);

  const filteredNamespaces = namespaces.filter(ns => {
    if (!searchQuery.trim()) return true;
    return ns.name.toLowerCase().includes(searchQuery.toLowerCase());
  });

  const handleCreate = async () => {
    if (!newNamespaceName.trim()) {
      setCreateError('Namespace name is required');
      return;
    }
    
    setIsCreating(true);
    setCreateError(null);
    try {
      await createNamespace(newNamespaceName);
      setIsCreateOpen(false);
      setNewNamespaceName('');
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : 'Failed to create namespace');
    } finally {
      setIsCreating(false);
    }
  };

  if (error) {
    return (
      <Card className="border-red-200">
        <CardContent className="py-6">
          <p className="text-red-700">{error}</p>
          <Button variant="outline" onClick={fetchNamespaces} className="mt-2">
            Retry
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center justify-between gap-4">
        <div className="relative flex-1 max-w-sm">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search namespaces..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="icon" onClick={fetchNamespaces} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
          <Button onClick={() => setIsCreateOpen(true)}>
            <Plus className="h-4 w-4 mr-2" />
            Create Namespace
          </Button>
        </div>
      </div>

      {/* Table */}
      {isLoading && namespaces.length === 0 ? (
        <div className="flex items-center justify-center py-8">
          <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        </div>
      ) : filteredNamespaces.length === 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>No Namespaces Found</CardTitle>
            <CardDescription>
              {searchQuery ? 'No namespaces match your search' : 'Create your first namespace to get started'}
            </CardDescription>
          </CardHeader>
        </Card>
      ) : (
        <div className="border rounded-lg">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Namespace</TableHead>
                <TableHead>Tables</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredNamespaces.map((namespace) => (
                <TableRow
                  key={namespace.name}
                  className={onSelectNamespace ? 'cursor-pointer' : ''}
                  onClick={() => onSelectNamespace?.(namespace)}
                >
                  <TableCell className="font-medium">
                    <div className="flex items-center gap-2">
                      <FolderOpen className="h-4 w-4 text-muted-foreground" />
                      {namespace.name}
                    </div>
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {namespace.table_count} tables
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {new Date(namespace.created_at).toLocaleDateString()}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {/* Create Namespace Dialog */}
      <Dialog open={isCreateOpen} onOpenChange={setIsCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create Namespace</DialogTitle>
            <DialogDescription>
              Create a new namespace to organize your tables
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <label className="text-sm font-medium">Namespace Name</label>
              <Input
                value={newNamespaceName}
                onChange={(e) => setNewNamespaceName(e.target.value)}
                placeholder="my_namespace"
              />
            </div>

            {createError && (
              <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded">
                {createError}
              </div>
            )}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setIsCreateOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleCreate} disabled={isCreating}>
              {isCreating && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
