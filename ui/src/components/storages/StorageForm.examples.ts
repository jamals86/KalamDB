// Example of how the new storage form works in practice

// When user opens the form, they see a dropdown:
// 
// ┌────────────────────────────────────────┐
// │ Select storage type                 ▼  │
// └────────────────────────────────────────┘
//
// When clicked, dropdown expands to show:
//
// ┌────────────────────────────────────────────────────────┐
// │ 🖴  Local Filesystem                                   │
// │     Store data on local or network-attached file       │
// │     systems                                            │
// ├────────────────────────────────────────────────────────┤
// │ ☁️  Amazon S3                                          │
// │     Store data in AWS S3 buckets with high            │
// │     durability                                         │
// ├────────────────────────────────────────────────────────┤
// │ ☁️  Google Cloud Storage                               │
// │     Store data in Google Cloud Storage buckets        │
// ├────────────────────────────────────────────────────────┤
// │ 💾  Azure Blob Storage                                 │
// │     Store data in Microsoft Azure Blob Storage        │
// └────────────────────────────────────────────────────────┘

// ========================================
// SCENARIO 1: User selects "Local Filesystem"
// ========================================

const filesystemFields = [
  { name: 'Storage ID', value: 'local_prod' },
  { name: 'Storage Type', value: 'Local Filesystem', disabled: true },
  { name: 'Storage Name', value: 'Production Local Storage' },
  { name: 'Description', value: 'Local NAS storage for production data' },
  { name: 'Base Directory', value: '/mnt/nas/kalamdb/' }, // Only this field for filesystem
  { name: 'Shared Tables Template', value: '{namespace}/{tableName}/' },
  { name: 'User Tables Template', value: '{namespace}/{tableName}/{userId}/' }
];

// ========================================
// SCENARIO 2: User selects "Amazon S3"
// ========================================

const s3Fields = [
  { name: 'Storage ID', value: 's3_prod' },
  { name: 'Storage Type', value: 'Amazon S3', disabled: true },
  { name: 'Storage Name', value: 'AWS S3 Production' },
  { name: 'Description', value: 'Primary S3 bucket for production' },
  { name: 'S3 Bucket URI', value: 's3://kalamdb-prod/data/' }, // Changed label
  // Additional S3-specific fields appear:
  { name: 'AWS Region', value: 'us-east-1' },
  { name: 'Access Key ID', value: 'AKIAIOSFODNN7EXAMPLE' },
  { name: 'Secret Access Key', value: '***hidden***', type: 'textarea' },
  { name: 'Shared Tables Template', value: '{namespace}/{tableName}/' },
  { name: 'User Tables Template', value: '{namespace}/{tableName}/{userId}/' }
];

// ========================================
// SCENARIO 3: User selects "Google Cloud Storage"
// ========================================

const gcsFields = [
  { name: 'Storage ID', value: 'gcs_prod' },
  { name: 'Storage Type', value: 'Google Cloud Storage', disabled: true },
  { name: 'Storage Name', value: 'GCS Production Bucket' },
  { name: 'Description', value: 'Google Cloud Storage for production' },
  { name: 'GCS Bucket URI', value: 'gs://kalamdb-prod/data/' }, // Changed label
  // Additional GCS-specific field appears:
  { 
    name: 'Service Account Key (JSON)', 
    value: '{\n  "type": "service_account",\n  "project_id": "my-project",\n  ...\n}',
    type: 'textarea'
  },
  { name: 'Shared Tables Template', value: '{namespace}/{tableName}/' },
  { name: 'User Tables Template', value: '{namespace}/{tableName}/{userId}/' }
];

// ========================================
// SCENARIO 4: User selects "Azure Blob Storage"
// ========================================

const azureFields = [
  { name: 'Storage ID', value: 'azure_prod' },
  { name: 'Storage Type', value: 'Azure Blob Storage', disabled: true },
  { name: 'Storage Name', value: 'Azure Production Storage' },
  { name: 'Description', value: 'Azure Blob Storage for production' },
  { name: 'Azure Container URI', value: 'https://kalamdb.blob.core.windows.net/prod/' }, // Changed label
  // Additional Azure-specific fields appear:
  { name: 'Storage Account Name', value: 'kalamdbstorage' },
  { name: 'Account Key', value: '***hidden***', type: 'password' },
  { name: 'Shared Tables Template', value: '{namespace}/{tableName}/' },
  { name: 'User Tables Template', value: '{namespace}/{tableName}/{userId}/' }
];

// ========================================
// Key Differences from Old Form:
// ========================================

// BEFORE:
// - Simple HTML <select> with just type names
// - No descriptions or icons
// - All fields always visible (confusing for users)
// - Generic "Base Directory/Bucket" label
// - No guidance on cloud provider credentials

// AFTER:
// - Rich shadcn-ui Select with descriptions and icons
// - Clear explanation of each storage type
// - Dynamic fields - only shows what you need
// - Type-specific labels (S3 Bucket URI, GCS Bucket URI, etc.)
// - Proper input types for credentials (textarea for JSON, password for keys)
// - Better user experience overall

export const storageFormExamples = {
  filesystemFields,
  s3Fields,
  gcsFields,
  azureFields
};
