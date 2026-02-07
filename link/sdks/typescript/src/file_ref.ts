/**
 * FileRef - Type-safe representation of FILE datatype in KalamDB
 *
 * Mirrors the Rust `FileRef` struct and provides utilities for working with
 * file references stored in FILE columns.
 *
 * Files are stored in table-specific folders with automatic subfolder rotation.
 * Download URLs can be generated to retrieve files via the KalamDB API.
 *
 * @example
 * ```typescript
 * // Parse FileRef from a FILE column
 * const fileRef = FileRef.fromJson(row.avatar);
 *
 * // Get download URL
 * const url = fileRef.getDownloadUrl('http://localhost:8080', 'default', 'users');
 *
 * // Fetch and display
 * const img = document.createElement('img');
 * img.src = url;
 * ```
 */

/**
 * File reference stored as JSON in FILE columns.
 *
 * Contains all metadata needed to locate and serve the file.
 */
export interface FileRefData {
  /** Unique file identifier (Snowflake ID) */
  id: string;

  /** Subfolder name (e.g., "f0001", "f0002") */
  sub: string;

  /** Original filename (preserved for display/download) */
  name: string;

  /** File size in bytes */
  size: number;

  /** MIME type (e.g., "image/png", "application/pdf") */
  mime: string;

  /** SHA-256 hash of file content (hex-encoded) */
  sha256: string;

  /** Optional shard ID for shared tables */
  shard?: number;
}

/**
 * Type-safe FileRef class for working with FILE datatype in KalamDB.
 *
 * Provides methods to parse JSON, generate download URLs, and access file metadata.
 */
export class FileRef implements FileRefData {
  id: string;
  sub: string;
  name: string;
  size: number;
  mime: string;
  sha256: string;
  shard?: number;

  constructor(data: FileRefData) {
    this.id = data.id;
    this.sub = data.sub;
    this.name = data.name;
    this.size = data.size;
    this.mime = data.mime;
    this.sha256 = data.sha256;
    this.shard = data.shard;
  }

  /**
   * Parse FileRef from JSON string (as stored in FILE columns)
   *
   * @param json - JSON string from FILE column
   * @returns FileRef instance
   * @throws Error if JSON is invalid
   *
   * @example
   * ```typescript
   * const fileRef = FileRef.fromJson(row.document);
   * console.log(fileRef.name, fileRef.size);
   * ```
   */
  static fromJson(json: string | null | undefined): FileRef | null {
    if (!json) return null;

    try {
      const data = typeof json === 'string' ? JSON.parse(json) : json;
      return new FileRef(data);
    } catch (err) {
      console.error('[FileRef] Failed to parse JSON:', err);
      return null;
    }
  }

  /**
   * Parse FileRef from unknown value (handles JSON string or object)
   *
   * @param value - Value from FILE column (string or object)
   * @returns FileRef instance or null
   *
   * @example
   * ```typescript
   * const fileRef = FileRef.from(row.avatar);
   * if (fileRef) {
   *   console.log('Avatar:', fileRef.name);
   * }
   * ```
   */
  static from(value: unknown): FileRef | null {
    if (!value) return null;

    if (typeof value === 'string') {
      return FileRef.fromJson(value);
    }

    if (typeof value === 'object' && value !== null) {
      return new FileRef(value as FileRefData);
    }

    return null;
  }

  /**
   * Generate download URL for this file
   *
   * @param baseUrl - KalamDB server URL (e.g., 'http://localhost:8080')
   * @param namespace - Table namespace
   * @param table - Table name
   * @returns Full download URL
   *
   * @example
   * ```typescript
   * const url = fileRef.getDownloadUrl('http://localhost:8080', 'default', 'users');
   * // Returns: http://localhost:8080/api/v1/files/default/users/f0001/12345
   * ```
   */
  getDownloadUrl(baseUrl: string, namespace: string, table: string): string {
    const cleanUrl = baseUrl.replace(/\/$/, '');
    return `${cleanUrl}/api/v1/files/${namespace}/${table}/${this.sub}/${this.id}`;
  }

  /**
   * Get the stored filename (with ID prefix)
   *
   * Format: `{id}-{sanitized_name}.{ext}` or `{id}.{ext}` if name is non-ASCII
   *
   * @returns Stored filename
   */
  storedName(): string {
    const sanitized = this.sanitizeFilename(this.name);
    const ext = this.extractExtension(this.name);

    if (sanitized.length === 0) {
      return `${this.id}.${ext}`;
    }

    return `${this.id}-${sanitized}.${ext}`;
  }

  /**
   * Get the relative path within the table folder
   *
   * For user tables: `{subfolder}/{stored_name}`
   * For shared tables with shard: `shard-{n}/{subfolder}/{stored_name}`
   *
   * @returns Relative path
   */
  relativePath(): string {
    const storedName = this.storedName();
    if (this.shard !== undefined) {
      return `shard-${this.shard}/${this.sub}/${storedName}`;
    }
    return `${this.sub}/${storedName}`;
  }

  /**
   * Format file size in human-readable format
   *
   * @returns Formatted size (e.g., "1.5 MB", "256 KB")
   *
   * @example
   * ```typescript
   * const fileRef = FileRef.fromJson(row.document);
   * console.log(fileRef.formatSize()); // "2.3 MB"
   * ```
   */
  formatSize(): string {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = this.size;
    let unitIndex = 0;

    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }

    return `${size.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
  }

  /**
   * Check if this is an image file (based on MIME type)
   *
   * @returns true if image MIME type
   */
  isImage(): boolean {
    return this.mime.startsWith('image/');
  }

  /**
   * Check if this is a video file (based on MIME type)
   *
   * @returns true if video MIME type
   */
  isVideo(): boolean {
    return this.mime.startsWith('video/');
  }

  /**
   * Check if this is an audio file (based on MIME type)
   *
   * @returns true if audio MIME type
   */
  isAudio(): boolean {
    return this.mime.startsWith('audio/');
  }

  /**
   * Check if this is a PDF file
   *
   * @returns true if PDF MIME type
   */
  isPdf(): boolean {
    return this.mime === 'application/pdf';
  }

  /**
   * Get a user-friendly file type description
   *
   * @returns File type description (e.g., "Image", "PDF Document", "Video")
   */
  getTypeDescription(): string {
    if (this.isImage()) return 'Image';
    if (this.isVideo()) return 'Video';
    if (this.isAudio()) return 'Audio';
    if (this.isPdf()) return 'PDF Document';

    // Extract from MIME type
    const parts = this.mime.split('/');
    if (parts.length === 2) {
      return parts[1].toUpperCase() + ' File';
    }

    return 'File';
  }

  /**
   * Serialize to JSON string
   *
   * @returns JSON string representation
   */
  toJson(): string {
    return JSON.stringify({
      id: this.id,
      sub: this.sub,
      name: this.name,
      size: this.size,
      mime: this.mime,
      sha256: this.sha256,
      ...(this.shard !== undefined && { shard: this.shard }),
    });
  }

  /**
   * Convert to plain object
   *
   * @returns Plain object representation
   */
  toObject(): FileRefData {
    return {
      id: this.id,
      sub: this.sub,
      name: this.name,
      size: this.size,
      mime: this.mime,
      sha256: this.sha256,
      ...(this.shard !== undefined && { shard: this.shard }),
    };
  }

  /**
   * Sanitize filename for storage (matches Rust implementation)
   *
   * @private
   */
  private sanitizeFilename(name: string): string {
    const nameWithoutExt = name.includes('.')
      ? name.substring(0, name.lastIndexOf('.'))
      : name;

    const sanitized = nameWithoutExt
      .split('')
      .map((c) => {
        if (/[a-zA-Z0-9]/.test(c)) {
          return c.toLowerCase();
        }
        if (c === ' ' || c === '_' || c === '-') {
          return '-';
        }
        return '';
      })
      .join('')
      .substring(0, 50);

    // Remove leading/trailing dashes and collapse multiple dashes
    return sanitized
      .replace(/^-+/, '')
      .replace(/-+$/, '')
      .replace(/-+/g, '-');
  }

  /**
   * Extract file extension (matches Rust implementation)
   *
   * @private
   */
  private extractExtension(name: string): string {
    if (!name.includes('.')) {
      return 'bin';
    }

    const ext = name.substring(name.lastIndexOf('.') + 1).toLowerCase();

    // Only keep extension if it's ASCII alphanumeric and reasonable length
    if (ext.length <= 10 && /^[a-z0-9]+$/.test(ext)) {
      return ext;
    }

    return 'bin';
  }
}

/**
 * Helper function to parse FileRef from query results
 *
 * @param value - Value from FILE column
 * @returns FileRef instance or null
 *
 * @example
 * ```typescript
 * const results = await client.query('SELECT * FROM users');
 * results.results[0].rows.forEach(row => {
 *   const avatar = parseFileRef(row.avatar);
 *   if (avatar) {
 *     console.log('Avatar URL:', avatar.getDownloadUrl(baseUrl, 'default', 'users'));
 *   }
 * });
 * ```
 */
export function parseFileRef(value: unknown): FileRef | null {
  return FileRef.from(value);
}

/**
 * Helper function to parse array of FileRefs from query results
 *
 * @param values - Array of values from FILE columns
 * @returns Array of FileRef instances (nulls filtered out)
 */
export function parseFileRefs(values: unknown[]): FileRef[] {
  return values
    .map((v) => FileRef.from(v))
    .filter((ref): ref is FileRef => ref !== null);
}
