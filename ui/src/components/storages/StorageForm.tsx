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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Storage, useStorages } from '@/hooks/useStorages';
import { Loader2, HardDrive, Cloud, Database } from 'lucide-react';

interface StorageFormProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  storage?: Storage;
  onSuccess: () => void;
}

type StorageType = 'filesystem' | 's3' | 'gcs' | 'azure';

interface StorageTypeConfig {
  value: StorageType;
  label: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  fields: {
    baseDirectoryLabel: string;
    baseDirectoryPlaceholder: string;
    baseDirectoryDescription: string;
    additionalFields?: Array<{
      key: string;
      label: string;
      placeholder: string;
      description: string;
      type?: 'text' | 'password';
    }>;
  };
}

const STORAGE_TYPES: StorageTypeConfig[] = [
  {
    value: 'filesystem',
    label: 'Local Filesystem',
    description: 'Store data on local or network-attached file systems',
    icon: HardDrive,
    fields: {
      baseDirectoryLabel: 'Base Directory',
      baseDirectoryPlaceholder: '/var/data/kalamdb/',
      baseDirectoryDescription: 'Absolute path to the storage directory on the server',
    },
  },
  {
    value: 's3',
    label: 'Amazon S3',
    description: 'Store data in AWS S3 buckets with high durability',
    icon: Cloud,
    fields: {
      baseDirectoryLabel: 'S3 Bucket URI',
      baseDirectoryPlaceholder: 's3://my-bucket/kalamdb/',
      baseDirectoryDescription: 'S3 bucket URI (e.g., s3://bucket-name/prefix/)',
      additionalFields: [
        {
          key: 'region',
          label: 'AWS Region',
          placeholder: 'us-east-1',
          description: 'AWS region where the bucket is located',
        },
        {
          key: 'access_key_id',
          label: 'Access Key ID',
          placeholder: 'AKIAIOSFODNN7EXAMPLE',
          description: 'AWS access key ID for authentication',
        },
        {
          key: 'secret_access_key',
          label: 'Secret Access Key',
          placeholder: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY',
          description: 'AWS secret access key for authentication',
          type: 'password',
        },
      ],
    },
  },
  {
    value: 'gcs',
    label: 'Google Cloud Storage',
    description: 'Store data in Google Cloud Storage buckets',
    icon: Cloud,
    fields: {
      baseDirectoryLabel: 'GCS Bucket URI',
      baseDirectoryPlaceholder: 'gs://my-bucket/kalamdb/',
      baseDirectoryDescription: 'GCS bucket URI (e.g., gs://bucket-name/prefix/)',
      additionalFields: [
        {
          key: 'service_account_key',
          label: 'Service Account Key (JSON)',
          placeholder: '{"type": "service_account", ...}',
          description: 'Google Cloud service account key in JSON format',
          type: 'password',
        },
      ],
    },
  },
  {
    value: 'azure',
    label: 'Azure Blob Storage',
    description: 'Store data in Microsoft Azure Blob Storage',
    icon: Database,
    fields: {
      baseDirectoryLabel: 'Azure Container URI',
      baseDirectoryPlaceholder: 'https://myaccount.blob.core.windows.net/container/',
      baseDirectoryDescription: 'Azure Blob Storage container URI',
      additionalFields: [
        {
          key: 'account_name',
          label: 'Storage Account Name',
          placeholder: 'mystorageaccount',
          description: 'Azure storage account name',
        },
        {
          key: 'account_key',
          label: 'Account Key',
          placeholder: 'your-account-key',
          description: 'Azure storage account key',
          type: 'password',
        },
      ],
    },
  },
];

export function StorageForm({ open, onOpenChange, storage, onSuccess }: StorageFormProps) {
  const { createStorage, updateStorage } = useStorages();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [formData, setFormData] = useState({
    storage_id: '',
    storage_type: 'filesystem' as StorageType,
    storage_name: '',
    description: '',
    base_directory: '',
    shared_tables_template: '{namespace}/{tableName}/',
    user_tables_template: '{namespace}/{tableName}/{userId}/',
    config: {} as Record<string, string>, // Additional config fields based on storage type
  });

  const isEditing = !!storage;

  // Reset form when dialog opens/closes or storage changes
  useEffect(() => {
    if (open) {
      if (storage) {
        setFormData({
          storage_id: storage.storage_id,
          storage_type: (storage.storage_type as StorageType) || 'filesystem',
          storage_name: storage.storage_name,
          description: storage.description || '',
          base_directory: storage.base_directory,
          shared_tables_template: storage.shared_tables_template || '{namespace}/{tableName}/',
          user_tables_template: storage.user_tables_template || '{namespace}/{tableName}/{userId}/',
          config: {},
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
          config: {},
        });
      }
      setError(null);
    }
  }, [open, storage]);

  // Get current storage type configuration
  const currentStorageConfig = STORAGE_TYPES.find(t => t.value === formData.storage_type)!;

  // Handle storage type change
  const handleStorageTypeChange = (value: StorageType) => {
    setFormData({
      ...formData,
      storage_type: value,
      base_directory: '', // Reset when type changes
      config: {}, // Reset additional config
    });
  };

  // Handle config field change
  const handleConfigChange = (key: string, value: string) => {
    setFormData({
      ...formData,
      config: {
        ...formData.config,
        [key]: value,
      },
    });
  };

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
            <Select
              value={formData.storage_type}
              onValueChange={handleStorageTypeChange}
              disabled={isEditing}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select storage type" />
              </SelectTrigger>
              <SelectContent>
                {STORAGE_TYPES.map((storageType) => {
                  const Icon = storageType.icon;
                  return (
                    <SelectItem key={storageType.value} value={storageType.value}>
                      <div className="flex items-start gap-3 py-1">
                        <Icon className="h-5 w-5 mt-0.5 text-muted-foreground" />
                        <div className="flex-1">
                          <div className="font-medium">{storageType.label}</div>
                          <div className="text-xs text-muted-foreground">
                            {storageType.description}
                          </div>
                        </div>
                      </div>
                    </SelectItem>
                  );
                })}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              Choose the backend storage system (cannot be changed after creation)
            </p>
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
              {currentStorageConfig.fields.baseDirectoryLabel}
            </label>
            <Input
              value={formData.base_directory}
              onChange={(e) => setFormData({ ...formData, base_directory: e.target.value })}
              disabled={isEditing}
              placeholder={currentStorageConfig.fields.baseDirectoryPlaceholder}
            />
            <p className="text-xs text-muted-foreground">
              {currentStorageConfig.fields.baseDirectoryDescription}
            </p>
          </div>

          {/* Dynamic additional fields based on storage type */}
          {currentStorageConfig.fields.additionalFields?.map((field) => (
            <div key={field.key} className="space-y-2">
              <label className="text-sm font-medium">{field.label}</label>
              {field.type === 'password' || field.key.includes('key') || field.key.includes('secret') ? (
                <Textarea
                  value={formData.config[field.key] || ''}
                  onChange={(e) => handleConfigChange(field.key, e.target.value)}
                  placeholder={field.placeholder}
                  rows={3}
                  className="font-mono text-xs"
                />
              ) : (
                <Input
                  value={formData.config[field.key] || ''}
                  onChange={(e) => handleConfigChange(field.key, e.target.value)}
                  placeholder={field.placeholder}
                  type={field.type || 'text'}
                />
              )}
              <p className="text-xs text-muted-foreground">
                {field.description}
              </p>
            </div>
          ))}

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
