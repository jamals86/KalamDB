# Message Duplication Architecture

**Feature**: KalamDB Chat and AI Message History Storage  
**Date**: 2025-10-13  
**Version**: 1.0

## Overview

KalamDB implements a **message duplication strategy** where each message sent to a conversation is stored in every participant's individual storage partition. This design prioritizes user data ownership, query performance, and privacy compliance over storage efficiency.

---

## Core Concept

```
┌─────────────────────────────────────────────────────────────┐
│  Message sent to Conversation A (3 participants)            │
│  Participants: userA, userB, userC                          │
└─────────────────────────────────────────────────────────────┘
                              ↓
        ┌─────────────────────┴─────────────────────┐
        ↓                     ↓                      ↓
┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│ userA/       │      │ userB/       │      │ userC/       │
│ batch-*.     │      │ batch-*.     │      │ batch-*.     │
│ parquet      │      │ parquet      │      │ parquet      │
│              │      │              │      │              │
│ [Message]    │      │ [Message]    │      │ [Message]    │
│ msgId: 123   │      │ msgId: 123   │      │ msgId: 123   │
│ convId: A    │      │ convId: A    │      │ convId: A    │
│ from: userA  │      │ from: userA  │      │ from: userA  │
│ content: ... │      │ content: ... │      │ content: ... │
└──────────────┘      └──────────────┘      └──────────────┘
```

**Key Insight**: Same `msgId` (123), but stored in 3 different user partitions.

---

## Benefits

### 1. User Data Ownership
- **Complete history**: Each user has a full copy of their conversation data
- **Independent backups**: Users can export/backup their data without dependencies
- **Data portability**: Easy to migrate user data between systems
- **Compliance**: GDPR "right to data portability" trivially satisfied

### 2. Query Performance
- **No joins**: Users query only their own partition (`userA/batch-*.parquet`)
- **Parallel queries**: Each user's query runs independently
- **Local filtering**: All filtering happens within a single partition
- **Fast aggregations**: `COUNT(*) FROM my_messages` doesn't need cross-user coordination

### 3. Privacy & Access Control
- **Physical isolation**: User A cannot access User B's storage
- **Simple permissions**: Storage-level access control (filesystem/S3 permissions)
- **Easy deletion**: Delete user partition = delete all user data
- **Audit trails**: Each user has their own independent audit log

### 4. Scalability
- **Horizontal scaling**: Shard users across multiple servers
- **Independent archival**: Archive old user data without affecting others
- **Hotspot avoidance**: Popular conversations don't create bottlenecks (load distributed)
- **Storage tiering**: Move cold users to cheaper storage independently

---

## Trade-offs

### Storage Cost
- **Multiplication factor**: N copies (N = participants per conversation)
- **Example**: 10-person group chat → 10x storage
- **Typical overhead**: 3-5x for most conversations
- **Mitigation**: Large message optimization (see below)

### Write Amplification
- **Multiple writes**: Each message written N times (parallel)
- **Performance target**: <100ms for N ≤ 10 participants
- **Mitigation**: Parallel writes, optimized RocksDB config

### Consistency Complexity
- **Partial failures**: What if write succeeds for userA but fails for userB?
- **Solution**: Retry mechanism + eventual consistency
- **Trade-off**: Accept slight delay for 100% delivery guarantee

---

## Large Message & Media File Optimization

### Problem
Duplicating large messages (e.g., 1MB+ content) and media files (images, documents, audio, video) across many participants wastes storage significantly.

### Solution: Shared Storage with References

**Configuration**:
```toml
[message]
large_message_threshold_bytes = 102400  # 100 KB (for text content)
max_size_bytes = 10485760  # 10 MB (total including media)

[media]
# Media files always use shared storage (regardless of size)
supported_types = ["image/*", "audio/*", "video/*", "application/pdf", "application/*"]
max_file_size_bytes = 52428800  # 50 MB
generate_thumbnails = true  # Auto-generate thumbnails for images/videos
thumbnail_max_dimension = 200  # Max width/height for thumbnails
```

**Flow for Large Text Messages**:
```
1. Message arrives with content.length = 500 KB (> 100 KB threshold)
2. Upload full content to shared storage:
   → s3://bucket/shared/conversations/convA/msg-123456789.bin
3. Truncate content to preview (first 1 KB)
4. Set contentRef field:
   → "s3://bucket/shared/conversations/convA/msg-123456789.bin"
5. Write to each participant's RocksDB:
   userA → {msgId: 123, content: "preview...", contentRef: "s3://..."}
   userB → {msgId: 123, content: "preview...", contentRef: "s3://..."}
   userC → {msgId: 123, content: "preview...", contentRef: "s3://..."}
```

**Flow for Media Files (Images, Documents, Audio, Video)**:
```
1. Client uploads media file: POST /api/v1/messages/upload
   - conversationId: convA
   - file: vacation-photo.jpg (2.4 MB)
2. Server processes upload:
   a. Store media in shared location:
      → s3://bucket/shared/conversations/convA/media-987654321.jpg
   b. Extract metadata (content type, dimensions, file size)
   c. Generate thumbnail for images/videos (200x200 max)
   d. Return contentRef and metadata
3. Client submits message with media reference:
   POST /api/v1/messages
   {
     "conversationId": "convA",
     "from": "userA",
     "content": "Check out this vacation photo!",
     "contentRef": "s3://bucket/shared/.../media-987654321.jpg",
     "metadata": {
       "contentType": "image/jpeg",
       "fileName": "vacation-photo.jpg",
       "fileSize": 2457600,
       "width": 1920,
       "height": 1080,
       "thumbnail": "data:image/jpeg;base64,/9j/4AAQ..."
     }
   }
4. Write to each participant's RocksDB:
   userA → {msgId: 123, content: "Check out...", contentRef: "s3://...", metadata: {...}}
   userB → {msgId: 123, content: "Check out...", contentRef: "s3://...", metadata: {...}}
   userC → {msgId: 123, content: "Check out...", contentRef: "s3://...", metadata: {...}}
```

**Benefits**:
- Only reference duplicated (negligible size)
- Full content stored once in shared location
- On-demand retrieval: `GET /api/v1/messages/{msgId}/content`

**Storage Savings**:

*Large Text Message Example (500 KB):*
```
Without optimization:
  500 KB × 10 participants = 5 MB

With optimization:
  (1 KB preview + ref + metadata) × 10 participants + 500 KB shared = ~510 KB
  
Savings: 90% reduction
```

*Media File Example (2.4 MB image):*
```
Without optimization:
  2.4 MB × 10 participants = 24 MB

With optimization:
  (thumbnail 10 KB + ref + metadata) × 10 participants + 2.4 MB shared = ~2.5 MB
  
Savings: 90% reduction
```

*Group Chat with Mixed Content (50 messages: 40 text, 8 images, 2 videos):*
```
Without optimization:
  Text: (40 × 5 KB) × 10 = 2 MB
  Images: (8 × 2 MB) × 10 = 160 MB
  Videos: (2 × 10 MB) × 10 = 200 MB
  Total: 362 MB

With optimization:
  Text: (40 × 5 KB) × 10 = 2 MB (below threshold, duplicated)
  Images: (8 × 15 KB ref) × 10 + (8 × 2 MB shared) = 17.2 MB
  Videos: (2 × 20 KB ref) × 10 + (2 × 10 MB shared) = 20.4 MB
  Total: 39.6 MB
  
Savings: 89% reduction
```

---

## Implementation Details

### Write Path

```rust
async fn submit_message(msg: MessageRequest) -> Result<MessageResponse> {
    // 1. Validate permissions
    check_conversation_access(&msg.from, &msg.conversationId)?;
    
    // 2. Generate unique msgId
    let msg_id = snowflake::generate();
    
    // 3. Query all participants
    let participants = get_conversation_users(&msg.conversationId).await?;
    
    // 4. Check if large message
    let (content, content_ref) = if msg.content.len() >= config.large_message_threshold {
        // Upload to shared storage
        let uri = upload_shared_content(
            &msg.conversationId, 
            msg_id, 
            &msg.content
        ).await?;
        
        // Truncate content, return reference
        (msg.content[..1024].to_string(), Some(uri))
    } else {
        (msg.content, None)
    };
    
    // 5. Write to RocksDB for each participant (parallel)
    let write_futures = participants.iter().map(|user_id| {
        write_to_rocksdb(user_id, Message {
            msg_id,
            conversation_id: msg.conversationId.clone(),
            from: msg.from.clone(),
            timestamp: msg.timestamp,
            content: content.clone(),
            content_ref: content_ref.clone(),
            metadata: msg.metadata.clone(),
        })
    });
    
    // Wait for all writes to complete
    futures::future::try_join_all(write_futures).await?;
    
    // 6. Acknowledge
    Ok(MessageResponse {
        msg_id,
        acknowledged: true,
    })
}
```

### Query Path

```rust
async fn query_messages(user_id: &str, conv_id: &str) -> Result<Vec<Message>> {
    // Query ONLY this user's partition
    let parquet_files = list_user_parquet_files(user_id)?;
    
    let query = format!(
        "SELECT * FROM parquet_scan('{}') WHERE conversationId = '{}' ORDER BY msgId DESC LIMIT 50",
        parquet_files.join(","),
        conv_id
    );
    
    // DataFusion executes query
    let messages = datafusion_ctx.sql(&query).await?.collect().await?;
    
    // If message has contentRef and user requests full content:
    // GET /api/v1/messages/{msgId}/content
    
    Ok(messages)
}
```

### Consolidation Path

```rust
async fn consolidate_user_messages(user_id: &str) {
    // Lock this user's consolidation (prevent concurrent runs)
    let _lock = acquire_user_lock(user_id).await;
    
    // Read all messages from RocksDB for this user
    let messages = rocksdb.scan_prefix(&format!("{}:", user_id))?;
    
    // Group by conversationId (for row group optimization)
    let grouped = group_by_conversation(messages);
    
    // Write to Parquet file: {userId}/batch-{timestamp}-{index}.parquet
    let parquet_path = format!("{}/batch-{}-001.parquet", user_id, now());
    write_parquet(&parquet_path, grouped).await?;
    
    // Delete from RocksDB to free memory
    for msg in messages {
        rocksdb.delete(&format!("{}:{}", user_id, msg.msg_id))?;
    }
}
```

---

## Storage Layout

```
storage-root/
├── userA/
│   ├── batch-2025-10-13-001.parquet    (userA's messages: text + media refs)
│   ├── batch-2025-10-13-002.parquet
│   └── batch-2025-10-14-001.parquet
├── userB/
│   ├── batch-2025-10-13-001.parquet    (userB's messages, incl. shared with userA)
│   └── batch-2025-10-13-002.parquet
├── userC/
│   └── batch-2025-10-13-001.parquet    (userC's messages)
└── shared/
    └── conversations/
        ├── convA/
        │   ├── msg-1234567890123456.bin        (large text message content)
        │   ├── media-1234567890123457.jpg      (image file)
        │   ├── media-1234567890123458.pdf      (document file)
        │   ├── media-1234567890123459.mp3      (audio file)
        │   ├── media-1234567890123460.mp4      (video file)
        │   └── msg-1234567899999999.bin        (another large message)
        └── convB/
            ├── msg-1234567800000000.bin
            ├── media-1234567800000001.png
            └── media-1234567800000002.zip       (archive file)
```

**File Naming Convention**:
- Large text messages: `msg-{msgId}.bin`
- Media files: `media-{msgId}.{ext}` (extension from original file)
- Organized by conversation for easy cleanup/archival

---

## Configuration Reference

### config.toml

```toml
[message]
# Maximum inline message size before using shared storage
large_message_threshold_bytes = 102400  # 100 KB

# Maximum total message size (including large messages)
max_size_bytes = 10485760  # 10 MB

[storage]
# Local or S3 backend
backend = "s3"

# Shared storage location for large message content
shared_storage_path = "shared/conversations"

[storage.s3]
bucket = "kalamdb-storage"
region = "us-east-1"

[duplication]
# Enable message duplication (future: allow single-storage mode)
enabled = true

# Warn when conversation has many participants (high storage cost)
max_participants_warning = 20
```

---

## Monitoring & Metrics

### Key Metrics to Track

1. **Duplication Factor**:
   ```
   avg_duplication_factor = total_physical_messages / total_logical_messages
   ```
   Expected: 3-5x for typical workloads

2. **Storage per User**:
   ```
   user_storage_bytes = sum(parquet_file_sizes) for user
   ```
   Monitor for users with excessive storage

3. **Large Message & Media Ratio**:
   ```
   content_with_ref_ratio = messages_with_contentRef / total_messages
   ```
   Expected: 5-15% for typical chat apps with media
   
4. **Media Type Breakdown**:
   ```
   media_distribution = {
     "text/plain": 85%,
     "image/*": 10%,
     "video/*": 3%,
     "audio/*": 1%,
     "application/*": 1%
   }
   ```
   Monitor for unusual patterns (e.g., excessive video uploads)

5. **Storage Efficiency**:
   ```
   storage_efficiency = total_shared_content_size / (total_shared_content_size + total_duplicated_references_size)
   ```
   Should be >90% for media-heavy conversations

6. **Write Latency by Participant Count**:
   ```
   p99_write_latency[participants=N]
   ```
   Alert if p99 > 100ms for N < 10
   
7. **Media Processing Time**:
   ```
   p95_thumbnail_generation_ms
   p95_metadata_extraction_ms
   ```
   Monitor for slow media processing

---

## Future Optimizations

### 1. Deduplication for Read-Only Users
Allow conversations with "read-only" participants who don't get full duplication:
- Read-only users query shared conversation storage
- Only "active" users get duplicated messages
- Reduces storage for large broadcast channels

### 2. Compression by Conversation Age
Older messages compressed more aggressively:
- Recent messages: Snappy (fast compression)
- 30-day old messages: Zstd (better compression)
- 90-day old messages: Zstd max (best compression)

### 3. Delta Storage
For large group chats, store deltas instead of full duplicates:
- First participant gets full message
- Others get delta (reference to first copy + user-specific metadata)
- Requires more complex query logic

### 4. Content-Addressed Storage (CAS)
For duplicate media files (e.g., forwarded images):
- Hash media content (SHA-256)
- Store once per unique hash: `shared/media/{hash}.{ext}`
- Multiple messages can reference same hash
- Reduces storage for viral/forwarded content

### 5. Adaptive Compression
Apply different compression strategies based on content type:
- **Images**: Convert large PNG to compressed JPEG/WebP
- **Videos**: Transcode to efficient codec (H.265, AV1)
- **Documents**: Apply PDF compression
- **Audio**: Convert to efficient codec (Opus, AAC)
- Balance quality vs storage cost

---

## Summary

**Message duplication is a deliberate architectural choice** that trades storage cost for:
- ✅ User data ownership
- ✅ Query performance (no joins)
- ✅ Privacy compliance (easy deletion)
- ✅ Horizontal scalability (shard by user)
- ✅ Operational simplicity (no distributed transactions)

**Storage cost is mitigated by**:
- Large message optimization (shared storage for >100KB text messages)
- Media file optimization (shared storage for ALL media: images, documents, audio, video)
- Thumbnail generation (small previews duplicated, full media stored once)
- Parquet compression (3-5x compression ratio for metadata)
- Per-user consolidation (removes RocksDB overhead)
- Content-addressed storage for duplicate media (future)

**Performance targets**:
- Write latency: <100ms for N ≤ 10 participants
- Query latency: <50ms for 1M messages per user
- Storage overhead: 3-5x typical (1x with large message optimization)

This architecture is **production-ready** for applications with:
- <100 participants per conversation (typical)
- <1GB average message size (with large message optimization)
- Media-rich conversations (images, documents, audio, video)
- Strong privacy/compliance requirements (GDPR, CCPA)

**Supported Media Types**:
- ✅ Images: JPEG, PNG, GIF, WebP, HEIC
- ✅ Documents: PDF, DOCX, XLSX, PPTX, TXT
- ✅ Audio: MP3, WAV, OGG, M4A, FLAC
- ✅ Video: MP4, MOV, AVI, WebM, MKV
- ✅ Archives: ZIP, RAR, 7z, TAR.GZ
