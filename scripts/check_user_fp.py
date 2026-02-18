#!/usr/bin/env python3
"""Check which files have false positive CREATE USER matches."""
import re

files_to_check = [
    "cli/tests/smoke/flushing/smoke_test_flush_operations.rs",
    "cli/tests/smoke/flushing/smoke_test_flush_pk_integrity.rs",
    "cli/tests/smoke/flushing/smoke_test_flush_manifest.rs",
    "cli/tests/smoke/usecases/smoke_test_all_datatypes.rs",
    "cli/tests/smoke/usecases/smoke_test_int64_precision.rs",
    "cli/tests/smoke/system/smoke_test_system_tables_extended.rs",
    "cli/tests/storage/test_storage_lifecycle.rs",
    "cli/tests/cluster/cluster_test_node_rejoin.rs",
]
for f in files_to_check:
    with open(f) as fp:
        content = fp.read()
    matches = list(re.finditer(r"CREATE USER\s+(?!TABLE)", content, re.IGNORECASE))
    if matches:
        for m in matches:
            start = max(0, m.start() - 40)
            end = min(len(content), m.end() + 60)
            ctx = content[start:end].replace("\n", " | ")
            print("%s: MATCH at pos %d: ...%s..." % (f, m.start(), ctx))
    else:
        print("%s: NO CREATE USER match (false positive)" % f)
