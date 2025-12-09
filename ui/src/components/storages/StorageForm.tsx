import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Storage, useStorages } from '@/hooks/useStorages';
import { Loader2 } from 'lucide-react';

interface StorageFormProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  storage?: Storage;
  onSuccess: () => void;
}

const STORAGE_TYPES = ['filesystem', 's3'] as const;

export function StorageForm({ open, onOpenChange, storage, onSuccess }: StorageFormProps) {
  const { createStorage, updateStorage } = useStorages();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [formData, setFormData] = useState({
    storage_id: '',
    storage_type: 'filesystem' as 'filesystem' | 's3',
    storage_name: '',
    description: '',
    base_directory: '',
    shared_tables_template: '{namespace}/{tableName}/',
    user_tables_template: '{namespace}/{tableName}/{userId}/',
  });

  const isEditing = !!storage;

  // Reset form when dialog opens/closes or storage changes
  useEffect(() => {
    if (open) {
      if (storage) {
        setFormData({
          storage_id: storage.storage_id,
          storage_type: (storage.storage_type as 'filesystem' | 's3') || 'filesystem',
          storage_name: storage.storage_name,
          description: storage.description || '',
          base_directory: storage.base_directory,
          shared_tables_template: storage.shared_tables_template || '{namespace}/{tableName}/',
          user_tables_template: storage.user_tables_template || '{namespace}/{tableName}/{userId}/',
        });
      } else {
        setFormData({
          storage_id: '',
          storage_type: 'filesystem',
          storage_name: '',
          description: '',
          base_directory: '',
          shared_tables_template: '{namespace}/{tableName}/',
          user_tables_template: '{namespace}/{tableName}/{userId}/',
        });
      }
      setError(null);
    }
  }, [open, storage]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    setError(null);

    try {
      if (isEditing) {
        await updateStorage(storage.storage_id, {
          storage_name: formData.storage_name !== storage.storage_name ? formData.storage_name : undefined,
          description: formData.description !== (storage.description || '') ? formData.description : undefined,
          shared_tables_template: formData.shared_tables_template !== (storage.shared_tables_template || '') 
            ? formData.shared_tables_template : undefined,
          user_tables_template: formData.user_tables_template !== (storage.user_tables_template || '') 
            ? formData.user_tables_template : undefined,
        });
      } else {
        if (!formData.storage_id.trim()) {
          throw new Error('Storage ID is required');
        }
        if (!formData.storage_name.trim()) {
          throw new Error('Storage Name is required');
        }
        if (!formData.base_directory.trim()) {
          throw new Error('Base Directory/Bucket is required');
        }
        
        await createStorage({
          storage_id: formData.storage_id,
          storage_type: formData.storage_type,
          storage_name: formData.storage_name,
          description: formData.description || undefined,
          base_directory: formData.base_directory,
          shared_tables_template: formData.shared_tables_template || undefined,
          user_tables_template: formData.user_tables_template || undefined,
        });
      }
      onSuccess();
      onOpenChange(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save storage');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{isEditing ? 'Edit Storage' : 'Create Storage'}</DialogTitle>
          <DialogDescription>
            {isEditing
              ? 'Update storage configuration'
              : 'Create a new storage backend for tables'}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">Storage ID</label>
            <Input
              value={formData.storage_id}
              onChange={(e) => setFormData({ ...formData, storage_id: e.target.value })}
              disabled={isEditing}
              placeholder="e.g., local_storage, s3_prod"
              pattern="[a-zA-Z][a-zA-Z0-9_]*"
              title="Must start with a letter, followed by letters, numbers, or underscores"
            />
            <p className="text-xs text-muted-foreground">
              Unique identifier for the storage (cannot be changed after creation)
            </p>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Storage Type</label>
            <select
              value={formData.storage_type}
              onChange={(e) => setFormData({ 
                ...formData, 
                storage_type: e.target.value as 'filesystem' | 's3',
                base_directory: '' // Reset when type changes
              })}
              disabled={isEditing}
              className="w-full px-3 py-2 border rounded-md text-sm bg-background"
            >
              {STORAGE_TYPES.map((type) => (
                <option key={type} value={type}>
                  {type === 'filesystem' ? 'Filesystem (Local)' : 'S3 (Cloud)'}
                </option>
              ))}
            </select>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Storage Name</label>
            <Input
              value={formData.storage_name}
              onChange={(e) => setFormData({ ...formData, storage_name: e.target.value })}
              placeholder="e.g., Local Filesystem Storage"
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Description (optional)</label>
            <Textarea
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
              placeholder="Describe the purpose of this storage"
              rows={2}
            />
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">
              {formData.storage_type === 'filesystem' ? 'Base Directory' : 'S3 Bucket'}
            </label>
            <Input
              value={formData.base_directory}
              onChange={(e) => setFormData({ ...formData, base_directory: e.target.value })}
              disabled={isEditing}
              placeholder={formData.storage_type === 'filesystem' 
                ? '/var/data/kalamdb/' 
                : 's3://my-bucket/kalamdb/'}
            />
            <p className="text-xs text-muted-foreground">
              {formData.storage_type === 'filesystem' 
                ? 'Absolute path to the storage directory'
                : 'S3 bucket URI (e.g., s3://bucket-name/prefix/)'}
            </p>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Shared Tables Template</label>
            <Input
              value={formData.shared_tables_template}
              onChange={(e) => setFormData({ ...formData, shared_tables_template: e.target.value })}
              placeholder="{namespace}/{tableName}/"
            />
            <p className="text-xs text-muted-foreground">
              Path template for shared tables. Available: {'{namespace}'}, {'{tableName}'}
            </p>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">User Tables Template</label>
            <Input
              value={formData.user_tables_template}
              onChange={(e) => setFormData({ ...formData, user_tables_template: e.target.value })}
              placeholder="{namespace}/{tableName}/{userId}/"
            />
            <p className="text-xs text-muted-foreground">
              Path template for user tables. Available: {'{namespace}'}, {'{tableName}'}, {'{userId}'}
            </p>
          </div>

          {error && (
            <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded">
              {error}
            </div>
          )}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              {isEditing ? 'Save Changes' : 'Create Storage'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
