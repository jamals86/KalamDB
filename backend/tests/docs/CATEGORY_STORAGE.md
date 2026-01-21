# Storage Tests (backend/tests/misc/storage)

## Standard Steps
1. Arrange storage backends and sample data.
2. Trigger storage/flush behavior described by the test name.
3. Assert manifest/parquet outputs and data integrity.

## Tests
- test_user_table_cold_storage_uses_manifest — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L32)
- test_shared_table_cold_storage_uses_manifest — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L148)
- test_manifest_tracks_multiple_flush_segments — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L238)
- test_cold_storage_version_resolution_after_update — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L341)
- test_cold_storage_delete_creates_tombstone — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L455)
- test_shared_table_manifest_isolation — [backend/tests/misc/storage/test_cold_storage_manifest.rs](backend/tests/misc/storage/test_cold_storage_manifest.rs#L556)
- test_get_or_load_cache_miss — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L45)
- test_get_or_load_cache_hit — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L57)
- test_validate_freshness_stale — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L94)
- test_update_after_flush_atomic_write — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L130)
- test_show_manifest_returns_all_entries — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L226)
- test_cache_eviction_and_repopulation — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L273)
- test_clear_all_entries — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L321)
- test_multiple_updates_same_key — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L345)
- test_invalidate_table_removes_all_user_entries — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L385)
- test_invalidate_table_preserves_other_tables — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L460)
- test_invalidate_table_shared — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L512)
- test_tiered_eviction_shared_tables_stay_longer — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L545)
- test_equal_weight_factor — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L611)
- test_cache_stats — [backend/tests/misc/storage/test_manifest_cache.rs](backend/tests/misc/storage/test_manifest_cache.rs#L648)
- test_create_manifest_generates_valid_json — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L155)
- test_update_manifest_increments_version — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L184)
- test_flush_five_batches_manifest_tracking — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L263)
- test_manifest_persistence_across_reads — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L334)
- test_batch_entry_metadata_preservation — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L377)
- test_manifest_validation_detects_corruption — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L419)
- test_segment_metadata_creation — [backend/tests/misc/storage/test_manifest_flush_integration.rs](backend/tests/misc/storage/test_manifest_flush_integration.rs#L450)
- test_storage_compact_table_and_all_commands — [backend/tests/misc/storage/test_storage_compact.rs](backend/tests/misc/storage/test_storage_compact.rs#L140)
- test_storage_compact_rejects_stream_and_empty_namespace — [backend/tests/misc/storage/test_storage_compact.rs](backend/tests/misc/storage/test_storage_compact.rs#L219)
