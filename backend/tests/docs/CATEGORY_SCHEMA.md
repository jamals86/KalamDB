# Schema Tests (backend/tests/misc/schema)

## Standard Steps
1. Arrange table schemas and any required seed data.
2. Apply the schema change described by the test name.
3. Assert schema metadata and read/write behavior.

## Tests
- test_alter_table_add_column — [backend/tests/misc/schema/test_alter_table.rs](backend/tests/misc/schema/test_alter_table.rs#L16)
- test_alter_table_drop_column — [backend/tests/misc/schema/test_alter_table.rs](backend/tests/misc/schema/test_alter_table.rs#L97)
- test_alter_table_rename_column — [backend/tests/misc/schema/test_alter_table.rs](backend/tests/misc/schema/test_alter_table.rs#L188)
- test_alter_table_modify_column — [backend/tests/misc/schema/test_alter_table.rs](backend/tests/misc/schema/test_alter_table.rs#L278)
- test_alter_table_schema_versioning — [backend/tests/misc/schema/test_alter_table.rs](backend/tests/misc/schema/test_alter_table.rs#L344)
- test_alter_table_add_column_after_flush — [backend/tests/misc/schema/test_alter_table_after_flush.rs](backend/tests/misc/schema/test_alter_table_after_flush.rs#L26)
- test_multiple_alter_operations_with_flushes — [backend/tests/misc/schema/test_alter_table_after_flush.rs](backend/tests/misc/schema/test_alter_table_after_flush.rs#L192)
- test_column_id_stability_across_schema_changes — [backend/tests/misc/schema/test_column_id_stability.rs](backend/tests/misc/schema/test_column_id_stability.rs#L16)
- test_column_stats_use_column_id — [backend/tests/misc/schema/test_column_id_stability.rs](backend/tests/misc/schema/test_column_id_stability.rs#L152)
- test_full_lifecycle_with_alter_and_flush — [backend/tests/misc/schema/test_column_id_stability.rs](backend/tests/misc/schema/test_column_id_stability.rs#L269)
- test_select_star_returns_columns_in_ordinal_order — [backend/tests/misc/schema/test_column_ordering.rs](backend/tests/misc/schema/test_column_ordering.rs#L20)
- test_alter_table_add_column_assigns_next_ordinal — [backend/tests/misc/schema/test_column_ordering.rs](backend/tests/misc/schema/test_column_ordering.rs#L89)
- test_alter_table_drop_column_preserves_ordinals — [backend/tests/misc/schema/test_column_ordering.rs](backend/tests/misc/schema/test_column_ordering.rs#L151)
- test_system_tables_have_correct_column_ordering — [backend/tests/misc/schema/test_column_ordering.rs](backend/tests/misc/schema/test_column_ordering.rs#L219)
- test_cache_invalidation_removes_entry — [backend/tests/misc/schema/test_schema_cache_invalidation.rs](backend/tests/misc/schema/test_schema_cache_invalidation.rs#L35)
- test_cache_invalidation_forces_cache_miss — [backend/tests/misc/schema/test_schema_cache_invalidation.rs](backend/tests/misc/schema/test_schema_cache_invalidation.rs#L111)
- test_selective_invalidation_preserves_other_entries — [backend/tests/misc/schema/test_schema_cache_invalidation.rs](backend/tests/misc/schema/test_schema_cache_invalidation.rs#L167)
- test_invalidation_idempotent — [backend/tests/misc/schema/test_schema_cache_invalidation.rs](backend/tests/misc/schema/test_schema_cache_invalidation.rs#L231)
- test_cache_stats_track_invalidation_behavior — [backend/tests/misc/schema/test_schema_cache_invalidation.rs](backend/tests/misc/schema/test_schema_cache_invalidation.rs#L280)
- test_schema_store_persistence — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L19)
- test_schema_cache_basic_operations — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L68)
- test_schema_versioning — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L114)
- test_all_system_tables_have_schemas — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L156)
- test_internal_api_schema_matches_describe_table — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L215)
- test_cache_invalidation_on_alter_table — [backend/tests/misc/schema/test_schema_consolidation.rs](backend/tests/misc/schema/test_schema_consolidation.rs#L269)
- test_all_kalambdata_types_convert_to_arrow_losslessly — [backend/tests/misc/schema/test_unified_types.rs](backend/tests/misc/schema/test_unified_types.rs#L10)
- test_embedding_dimensions_work_correctly — [backend/tests/misc/schema/test_unified_types.rs](backend/tests/misc/schema/test_unified_types.rs#L72)
- test_type_conversion_performance — [backend/tests/misc/schema/test_unified_types.rs](backend/tests/misc/schema/test_unified_types.rs#L104)
