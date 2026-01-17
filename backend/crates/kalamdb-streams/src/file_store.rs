use crate::config::StreamLogConfig;
use crate::error::{Result, StreamLogError};
use crate::record::StreamLogRecord;
use crate::store_trait::StreamLogStore;
use crate::time_bucket::StreamTimeBucket;
use crate::utils::{cleanup_empty_dir, parse_log_window, read_dirs, read_files};
use chrono::{Datelike, TimeZone, Timelike, Utc};
use kalamdb_commons::ids::StreamTableRowId;
use kalamdb_commons::models::{StreamTableRow, TableId, UserId};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// File-based stream log store.
#[derive(Debug, Clone)]
pub struct FileStreamLogStore {
    config: StreamLogConfig,
}

impl FileStreamLogStore {
    pub fn new(config: StreamLogConfig) -> Self {
        Self { config }
    }

    pub fn table_id(&self) -> &TableId {
        &self.config.table_id
    }

    pub fn append_delete(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        row_id: &StreamTableRowId,
    ) -> Result<()> {
        self.ensure_table(table_id)?;
        let ts = row_id.seq().timestamp_millis();
        let window_start = self.window_start_ms(ts);
        let path = self.log_path(user_id, window_start);
        self.append_record(
            &path,
            StreamLogRecord::Delete {
                row_id: row_id.clone(),
            },
        )
    }

    pub fn delete_old_logs_with_count(&self, before_time: u64) -> Result<usize> {
        let mut deleted = 0usize;
        let base_dir = &self.config.base_dir;
        if !base_dir.exists() {
            return Ok(0);
        }

        let bucket_dirs = read_dirs(base_dir)?;
        for bucket_dir in bucket_dirs {
            let shard_dirs = read_dirs(&bucket_dir)?;
            for shard_dir in shard_dirs {
                let user_dirs = read_dirs(&shard_dir)?;
                for user_dir in user_dirs {
                    let log_files = read_files(&user_dir)?;
                    for log_file in log_files {
                        if let Some(window_start) = parse_log_window(&log_file) {
                            let window_end =
                                window_start.saturating_add(self.config.bucket.duration_ms());
                            if window_end < before_time {
                                if fs::remove_file(&log_file).is_ok() {
                                    deleted += 1;
                                }
                            }
                        }
                    }
                    cleanup_empty_dir(&user_dir);
                }
                cleanup_empty_dir(&shard_dir);
            }
            cleanup_empty_dir(&bucket_dir);
        }

        cleanup_empty_dir(base_dir);

        Ok(deleted)
    }

    pub fn has_logs_before(&self, before_time: u64) -> Result<bool> {
        let base_dir = &self.config.base_dir;
        if !base_dir.exists() {
            return Ok(false);
        }

        let bucket_dirs = read_dirs(base_dir)?;
        for bucket_dir in bucket_dirs {
            let shard_dirs = read_dirs(&bucket_dir)?;
            for shard_dir in shard_dirs {
                let user_dirs = read_dirs(&shard_dir)?;
                for user_dir in user_dirs {
                    let log_files = read_files(&user_dir)?;
                    for log_file in log_files {
                        if let Some(window_start) = parse_log_window(&log_file) {
                            let window_end =
                                window_start.saturating_add(self.config.bucket.duration_ms());
                            if window_end < before_time {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    pub fn list_user_ids(&self) -> Result<Vec<UserId>> {
        let mut users = HashSet::new();
        let base_dir = &self.config.base_dir;
        if !base_dir.exists() {
            return Ok(Vec::new());
        }

        let bucket_dirs = read_dirs(base_dir)?;
        for bucket_dir in bucket_dirs {
            let shard_dirs = read_dirs(&bucket_dir)?;
            for shard_dir in shard_dirs {
                let user_dirs = read_dirs(&shard_dir)?;
                for user_dir in user_dirs {
                    if let Some(name) = user_dir.file_name().and_then(|n| n.to_str()) {
                        users.insert(UserId::new(name));
                    }
                }
            }
        }

        Ok(users.into_iter().collect())
    }

    fn ensure_table(&self, table_id: &TableId) -> Result<()> {
        if table_id != &self.config.table_id {
            return Err(StreamLogError::InvalidInput(format!(
                "Stream log store configured for {} but got {}",
                self.config.table_id, table_id
            )));
        }
        Ok(())
    }

    fn window_start_ms(&self, ts_ms: u64) -> u64 {
        let dt = Utc.timestamp_millis_opt(ts_ms as i64).single();
        let dt = match dt {
            Some(val) => val,
            None => return ts_ms,
        };

        match self.config.bucket {
            StreamTimeBucket::Hour => {
                let truncated = dt
                    .with_minute(0)
                    .and_then(|v| v.with_second(0))
                    .and_then(|v| v.with_nanosecond(0));
                truncated.map(|v| v.timestamp_millis() as u64).unwrap_or(ts_ms)
            },
            StreamTimeBucket::Day => {
                let truncated =
                    dt.date_naive().and_hms_opt(0, 0, 0).map(|naive| Utc.from_utc_datetime(&naive));
                truncated.map(|v| v.timestamp_millis() as u64).unwrap_or(ts_ms)
            },
            StreamTimeBucket::Week => {
                let weekday = dt.weekday().num_days_from_monday() as i64;
                let date = dt.date_naive() - chrono::Duration::days(weekday);
                let naive = date.and_hms_opt(0, 0, 0);
                naive
                    .map(|v| Utc.from_utc_datetime(&v).timestamp_millis() as u64)
                    .unwrap_or(ts_ms)
            },
            StreamTimeBucket::Month => {
                let naive = chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1)
                    .and_then(|d| d.and_hms_opt(0, 0, 0));
                naive
                    .map(|v| Utc.from_utc_datetime(&v).timestamp_millis() as u64)
                    .unwrap_or(ts_ms)
            },
        }
    }

    fn bucket_folder(&self, window_start_ms: u64) -> String {
        let dt = Utc
            .timestamp_millis_opt(window_start_ms as i64)
            .single()
            .unwrap_or_else(|| Utc.timestamp_millis_opt(0).single().unwrap());
        match self.config.bucket {
            StreamTimeBucket::Hour => dt.format("%Y%m%d%H").to_string(),
            StreamTimeBucket::Day => dt.format("%Y%m%d").to_string(),
            StreamTimeBucket::Week => dt.format("%G%V").to_string(),
            StreamTimeBucket::Month => dt.format("%Y%m").to_string(),
        }
    }

    fn log_path(&self, user_id: &UserId, window_start_ms: u64) -> PathBuf {
        let bucket_folder = self.bucket_folder(window_start_ms);
        let shard = self.config.shard_router.route_stream_user(user_id).folder_name();
        let user_dir = self.config.base_dir.join(bucket_folder).join(shard).join(user_id.as_str());
        user_dir.join(format!("{}.log", window_start_ms))
    }

    fn append_record(&self, path: &Path, record: StreamLogRecord) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StreamLogError::Io(e.to_string()))?;
        }

        let mut file = BufWriter::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| StreamLogError::Io(e.to_string()))?,
        );

        let payload = bincode::serde::encode_to_vec(&record, bincode::config::standard())
            .map_err(|e| StreamLogError::Serialization(e.to_string()))?;
        let len = payload.len() as u32;
        file.write_all(&len.to_le_bytes())
            .map_err(|e| StreamLogError::Io(e.to_string()))?;
        file.write_all(&payload).map_err(|e| StreamLogError::Io(e.to_string()))?;
        file.flush().map_err(|e| StreamLogError::Io(e.to_string()))?;
        Ok(())
    }

    fn read_records(path: &Path) -> Result<Vec<StreamLogRecord>> {
        let file = File::open(path).map_err(|e| StreamLogError::Io(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();
        loop {
            let mut len_buf = [0u8; 4];
            match reader.read_exact(&mut len_buf) {
                Ok(_) => {},
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(StreamLogError::Io(err.to_string()));
                },
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            reader.read_exact(&mut payload).map_err(|e| StreamLogError::Io(e.to_string()))?;
            let (record, _) = bincode::serde::decode_from_slice::<StreamLogRecord, _>(
                &payload,
                bincode::config::standard(),
            )
            .map_err(|e| StreamLogError::Serialization(e.to_string()))?;
            records.push(record);
        }
        Ok(records)
    }

    fn list_log_files_for_user(&self, user_id: &UserId) -> Result<Vec<(u64, PathBuf)>> {
        let mut entries = Vec::new();
        let base_dir = &self.config.base_dir;
        if !base_dir.exists() {
            return Ok(entries);
        }

        let bucket_dirs = read_dirs(base_dir)?;
        for bucket_dir in bucket_dirs {
            let shard_dirs = read_dirs(&bucket_dir)?;
            for shard_dir in shard_dirs {
                let user_dir = shard_dir.join(user_id.as_str());
                if !user_dir.exists() {
                    continue;
                }
                let log_files = read_files(&user_dir)?;
                for log_file in log_files {
                    if let Some(window_start) = parse_log_window(&log_file) {
                        entries.push((window_start, log_file));
                    }
                }
            }
        }

        Ok(entries)
    }

    fn read_range_internal(
        &self,
        user_id: &UserId,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<(StreamTableRowId, StreamTableRow)>> {
        let mut entries = self.list_log_files_for_user(user_id)?;
        if entries.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let bucket_ms = self.config.bucket.duration_ms();
        entries.retain(|(window_start, _)| {
            let window_end = window_start.saturating_add(bucket_ms);
            *window_start <= end_time && window_end >= start_time
        });
        entries.sort_by_key(|(window_start, _)| *window_start);

        let mut results: Vec<(StreamTableRowId, StreamTableRow)> = Vec::new();
        let mut deleted: HashSet<StreamTableRowId> = HashSet::new();

        for (_window_start, path) in entries {
            let records = Self::read_records(&path)?;
            for record in records {
                match record {
                    StreamLogRecord::Put { row_id, row } => {
                        if deleted.contains(&row_id) {
                            continue;
                        }
                        results.push((row_id, row));
                        if results.len() >= limit {
                            return Ok(results);
                        }
                    },
                    StreamLogRecord::Delete { row_id } => {
                        deleted.insert(row_id.clone());
                        results.retain(|(existing_id, _)| existing_id != &row_id);
                    },
                }
            }
        }

        Ok(results)
    }

    fn read_latest_internal(
        &self,
        user_id: &UserId,
        limit: usize,
    ) -> Result<Vec<(StreamTableRowId, StreamTableRow)>> {
        let mut entries = self.list_log_files_for_user(user_id)?;
        if entries.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        entries.sort_by(|a, b| b.0.cmp(&a.0));

        let mut results: Vec<(StreamTableRowId, StreamTableRow)> = Vec::new();
        let mut deleted: HashSet<StreamTableRowId> = HashSet::new();

        for (_window_start, path) in entries {
            let records = Self::read_records(&path)?;
            for record in records.into_iter().rev() {
                match record {
                    StreamLogRecord::Put { row_id, row } => {
                        if deleted.contains(&row_id) {
                            continue;
                        }
                        results.push((row_id, row));
                        if results.len() >= limit {
                            return Ok(results);
                        }
                    },
                    StreamLogRecord::Delete { row_id } => {
                        deleted.insert(row_id);
                    },
                }
            }
        }

        Ok(results)
    }
}

impl StreamLogStore for FileStreamLogStore {
    fn append_rows(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        rows: HashMap<StreamTableRowId, StreamTableRow>,
    ) -> Result<()> {
        self.ensure_table(table_id)?;
        for (row_id, row) in rows {
            let ts = row_id.seq().timestamp_millis();
            let window_start = self.window_start_ms(ts);
            let path = self.log_path(user_id, window_start);
            self.append_record(&path, StreamLogRecord::Put { row_id, row })?;
        }
        Ok(())
    }

    fn read_with_limit(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        limit: usize,
    ) -> Result<HashMap<StreamTableRowId, StreamTableRow>> {
        self.ensure_table(table_id)?;
        let rows = self.read_latest_internal(user_id, limit)?;
        Ok(rows.into_iter().collect())
    }

    fn read_in_time_range(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<HashMap<StreamTableRowId, StreamTableRow>> {
        self.ensure_table(table_id)?;
        let rows = self.read_range_internal(user_id, start_time, end_time, limit)?;
        Ok(rows.into_iter().collect())
    }

    fn delete_old_logs(&self, before_time: u64) -> Result<()> {
        let _ = self.delete_old_logs_with_count(before_time)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::FileStreamLogStore;
    use crate::config::StreamLogConfig;
    use crate::store_trait::StreamLogStore;
    use crate::time_bucket::StreamTimeBucket;
    use chrono::{Datelike, TimeZone, Timelike};
    use datafusion::scalar::ScalarValue;
    use kalamdb_commons::ids::{SeqId, SnowflakeGenerator, StreamTableRowId};
    use kalamdb_commons::models::rows::Row;
    use kalamdb_commons::models::{NamespaceId, StreamTableRow, TableId, TableName, UserId};
    use kalamdb_sharding::ShardRouter;
    use std::collections::{BTreeMap, HashMap};
    use std::fs;
    use std::path::PathBuf;

    fn temp_base_dir(prefix: &str) -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("{}_{}_{}", prefix, std::process::id(), now));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seq_from_timestamp(ts_ms: u64) -> SeqId {
        let id =
            SnowflakeGenerator::max_id_for_timestamp(ts_ms).expect("max_id_for_timestamp failed");
        SeqId::new(id)
    }

    fn window_start_ms(bucket: StreamTimeBucket, ts_ms: u64) -> u64 {
        let dt = chrono::Utc
            .timestamp_millis_opt(ts_ms as i64)
            .single()
            .unwrap_or_else(|| chrono::Utc.timestamp_millis_opt(0).single().unwrap());

        match bucket {
            StreamTimeBucket::Hour => dt
                .with_minute(0)
                .and_then(|v| v.with_second(0))
                .and_then(|v| v.with_nanosecond(0))
                .map(|v| v.timestamp_millis() as u64)
                .unwrap_or(ts_ms),
            StreamTimeBucket::Day => dt
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|naive| chrono::Utc.from_utc_datetime(&naive))
                .map(|v| v.timestamp_millis() as u64)
                .unwrap_or(ts_ms),
            StreamTimeBucket::Week => {
                let weekday = dt.weekday().num_days_from_monday() as i64;
                let date = dt.date_naive() - chrono::Duration::days(weekday);
                let naive = date.and_hms_opt(0, 0, 0);
                naive
                    .map(|v| chrono::Utc.from_utc_datetime(&v).timestamp_millis() as u64)
                    .unwrap_or(ts_ms)
            },
            StreamTimeBucket::Month => chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|v| chrono::Utc.from_utc_datetime(&v).timestamp_millis() as u64)
                .unwrap_or(ts_ms),
        }
    }

    fn bucket_folder(bucket: StreamTimeBucket, window_start_ms: u64) -> String {
        let dt = chrono::Utc
            .timestamp_millis_opt(window_start_ms as i64)
            .single()
            .unwrap_or_else(|| chrono::Utc.timestamp_millis_opt(0).single().unwrap());
        match bucket {
            StreamTimeBucket::Hour => dt.format("%Y%m%d%H").to_string(),
            StreamTimeBucket::Day => dt.format("%Y%m%d").to_string(),
            StreamTimeBucket::Week => dt.format("%G%V").to_string(),
            StreamTimeBucket::Month => dt.format("%Y%m").to_string(),
        }
    }

    fn build_row(user_id: &UserId, seq: SeqId) -> StreamTableRow {
        let values: BTreeMap<String, ScalarValue> = BTreeMap::new();
        StreamTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            fields: Row::new(values),
        }
    }

    #[test]
    fn test_delete_old_logs_cleans_files_and_folders() {
        let base_dir = temp_base_dir("kalamdb_streams_delete_old");
        let table_id = TableId::new(NamespaceId::new("test_ns"), TableName::new("events"));
        let shard_router = ShardRouter::new(4, 1);
        let bucket = StreamTimeBucket::Hour;

        let store = FileStreamLogStore::new(StreamLogConfig {
            base_dir: base_dir.clone(),
            shard_router: shard_router.clone(),
            bucket,
            table_id: table_id.clone(),
        });

        let user_id = UserId::new("user-1");
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let old_ts = now_ms.saturating_sub(3 * 60 * 60 * 1000);
        let new_ts = now_ms.saturating_sub(10 * 60 * 1000);

        let old_seq = seq_from_timestamp(old_ts);
        let new_seq = seq_from_timestamp(new_ts);
        let old_id = StreamTableRowId::new(user_id.clone(), old_seq);
        let new_id = StreamTableRowId::new(user_id.clone(), new_seq);

        let mut rows = HashMap::new();
        rows.insert(old_id.clone(), build_row(&user_id, old_seq));
        rows.insert(new_id.clone(), build_row(&user_id, new_seq));

        store.append_rows(&table_id, &user_id, rows).expect("append_rows failed");

        let shard_folder = shard_router.route_stream_user(&user_id).folder_name();
        let old_window = window_start_ms(bucket, old_ts);
        let new_window = window_start_ms(bucket, new_ts);
        let old_bucket = bucket_folder(bucket, old_window);
        let new_bucket = bucket_folder(bucket, new_window);

        let old_path = base_dir
            .join(old_bucket.clone())
            .join(&shard_folder)
            .join(user_id.as_str())
            .join(format!("{}.log", old_window));
        let new_path = base_dir
            .join(new_bucket.clone())
            .join(&shard_folder)
            .join(user_id.as_str())
            .join(format!("{}.log", new_window));

        assert!(old_path.exists(), "expected old log file to exist");
        assert!(new_path.exists(), "expected new log file to exist");
        assert!(
            store
                .has_logs_before(now_ms.saturating_sub(60 * 60 * 1000))
                .expect("has_logs_before failed"),
            "expected logs before cutoff"
        );

        let deleted = store
            .delete_old_logs_with_count(now_ms.saturating_sub(60 * 60 * 1000))
            .expect("delete_old_logs_with_count failed");
        assert!(deleted >= 1, "expected at least one log file deleted");

        assert!(!old_path.exists(), "expected old log file to be deleted");
        assert!(new_path.exists(), "expected new log file to remain");

        let old_bucket_dir = base_dir.join(old_bucket);
        assert!(!old_bucket_dir.exists(), "expected old bucket directory to be removed");
        assert!(base_dir.exists(), "expected base dir to remain");

        let _ = fs::remove_dir_all(&base_dir);
    }
}
