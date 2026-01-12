//! Flush concurrency + correctness tests over the real HTTP SQL API.
//!
//! These tests were previously in `tests/integration/flush/*` but were not
//! registered in `backend/Cargo.toml`, so they never ran. This suite migrates
//! them to the near-production HTTP harness.

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use test_support::http_server::{with_http_test_server_timeout, HttpTestServer};
use tokio::time::{sleep, Duration, Instant};

fn is_pending_job_status(status: &str) -> bool {
	matches!(status, "new" | "queued" | "running" | "retrying")
}

async fn wait_for_flush_jobs_settled(
	server: &HttpTestServer,
	ns: &str,
	table: &str,
) -> anyhow::Result<()> {
	let deadline = Instant::now() + Duration::from_secs(10);

	loop {
		let resp = server
			.execute_sql("SELECT job_type, status, parameters FROM system.jobs WHERE job_type = 'flush'")
			.await?;

		if resp.status != ResponseStatus::Success {
			if Instant::now() >= deadline {
				anyhow::bail!("Timed out waiting for system.jobs to be queryable");
			}
			sleep(Duration::from_millis(50)).await;
			continue;
		}

		let rows = resp.results[0].rows_as_maps();
		let matching: Vec<_> = rows
			.iter()
			.filter(|r| {
				r.get("parameters")
					.and_then(|v| v.as_str())
					.map(|s| s.contains(ns) && s.contains(table))
					.unwrap_or(false)
			})
			.collect();

		let has_completed = matching.iter().any(|r| {
			r.get("status")
				.and_then(|v| v.as_str())
				.map(|s| s == "completed")
				.unwrap_or(false)
		});

		let has_pending = matching.iter().any(|r| {
			r.get("status")
				.and_then(|v| v.as_str())
				.map(is_pending_job_status)
				.unwrap_or(false)
		});

		if has_completed && !has_pending {
			return Ok(());
		}

		if Instant::now() >= deadline {
			anyhow::bail!(
				"Timed out waiting for flush jobs to settle for {}.{} (matching={:?})",
				ns,
				table,
				matching
			);
		}

		sleep(Duration::from_millis(50)).await;
	}
}

async fn count_rows(server: &HttpTestServer, auth: &str, ns: &str, table: &str) -> anyhow::Result<i64> {
	let resp = server
		.execute_sql_with_auth(
			&format!("SELECT COUNT(*) AS cnt FROM {}.{}", ns, table),
			auth,
		)
		.await?;
	anyhow::ensure!(
		resp.status == ResponseStatus::Success,
		"COUNT failed: {:?}",
		resp.error
	);

	let row = resp
		.results
		.first()
		.and_then(|r| r.row_as_map(0))
		.ok_or_else(|| anyhow::anyhow!("Missing COUNT result row"))?;

	row.get("cnt")
		.and_then(|v| {
			v.as_i64()
				.or_else(|| v.as_u64().map(|u| u as i64))
				.or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
		})
		.ok_or_else(|| anyhow::anyhow!("COUNT value not an integer: {:?}", row.get("cnt")))
}

async fn create_user(server: &HttpTestServer, username: &str, password: &str) -> anyhow::Result<String> {
	let resp = server
		.execute_sql(&format!(
			"CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
			username, password
		))
		.await?;
	anyhow::ensure!(
		resp.status == ResponseStatus::Success,
		"CREATE USER failed: {:?}",
		resp.error
	);
	Ok(HttpTestServer::basic_auth_header(&UserName::new(username), password))
}

async fn create_user_table(
	server: &HttpTestServer,
	ns: &str,
	table: &str,
	flush_policy: &str,
) -> anyhow::Result<()> {
	let resp = server
		.execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
		.await?;
	anyhow::ensure!(
		resp.status == ResponseStatus::Success,
		"CREATE NAMESPACE failed: {:?}",
		resp.error
	);

	let create_sql = format!(
		r#"CREATE TABLE {ns}.{table} (
			id INT PRIMARY KEY,
			data TEXT
		) WITH (
			TYPE = 'USER',
			STORAGE_ID = 'local',
			FLUSH_POLICY = '{flush_policy}'
		)"#
	);

	let resp = server.execute_sql(&create_sql).await?;
	anyhow::ensure!(
		resp.status == ResponseStatus::Success,
		"CREATE TABLE failed: {:?}",
		resp.error
	);
	Ok(())
}

async fn flush_table_and_wait(server: &HttpTestServer, ns: &str, table: &str) -> anyhow::Result<()> {
	let sql = format!("FLUSH TABLE {}.{}", ns, table);
	let resp = server.execute_sql(&sql).await?;

	if resp.status == ResponseStatus::Success {
		return wait_for_flush_jobs_settled(server, ns, table).await;
	}

	let is_idempotent_conflict = resp
		.error
		.as_ref()
		.map(|e| e.message.contains("Idempotent conflict"))
		.unwrap_or(false);

	if is_idempotent_conflict {
		// A flush is already running/queued for this table. Treat this as success
		// and wait for it to settle, matching old synchronous flush semantics.
		return wait_for_flush_jobs_settled(server, ns, table).await;
	}

	anyhow::bail!("FLUSH TABLE failed: {:?}", resp.error);
}

#[tokio::test]
async fn test_flush_concurrency_and_correctness_over_http() {
	with_http_test_server_timeout(Duration::from_secs(180), |server| {
		Box::pin(async move {
			let suffix = std::process::id();

			let user_a = format!("user_a_{}", suffix);
			let user_b = format!("user_b_{}", suffix);
			let password = "UserPass123!";
			let auth_a = create_user(server, &user_a, password).await?;
			let auth_b = create_user(server, &user_b, password).await?;

			// -----------------------------------------------------------------
			// test_flush_concurrent_dml
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_concurrent_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:50").await?;

				let writer = async {
					for i in 1..=30 {
						let insert_sql = format!(
							"INSERT INTO {}.{} (id, data) VALUES ({}, 'value_{}')",
							ns, table, i, i
						);
						let resp = server.execute_sql_with_auth(&insert_sql, &auth_a).await?;
						anyhow::ensure!(
							resp.status == ResponseStatus::Success,
							"insert {} failed: {:?}",
							i,
							resp.error
						);

						if i % 5 == 0 {
							let update_sql = format!(
								"UPDATE {}.{} SET data = 'updated_{}' WHERE id = {}",
								ns, table, i, i
							);
							let resp = server.execute_sql_with_auth(&update_sql, &auth_a).await?;
							anyhow::ensure!(
								resp.status == ResponseStatus::Success,
								"update {} failed: {:?}",
								i,
								resp.error
							);
						}

						if i % 10 == 0 {
							let delete_id = i - 5;
							let delete_sql =
								format!("DELETE FROM {}.{} WHERE id = {}", ns, table, delete_id);
							let resp = server.execute_sql_with_auth(&delete_sql, &auth_a).await?;
							anyhow::ensure!(
								resp.status == ResponseStatus::Success,
								"delete {} failed: {:?}",
								delete_id,
								resp.error
							);
						}
					}
					anyhow::Ok(())
				};

				let flusher = async {
					for _ in 0..2 {
						flush_table_and_wait(server, &ns, table).await?;
						sleep(Duration::from_millis(10)).await;
					}
					anyhow::Ok(())
				};

				let (w, f) = tokio::join!(writer, flusher);
				w?;
				f?;

				flush_table_and_wait(server, &ns, table).await?;

				let cnt = count_rows(server, &auth_a, &ns, table).await?;
				anyhow::ensure!(cnt == 27, "expected 27 rows after deletes, got {}", cnt);

				for deleted_id in [5, 15, 25] {
					let resp = server
						.execute_sql_with_auth(
							&format!("SELECT id FROM {}.{} WHERE id = {}", ns, table, deleted_id),
							&auth_a,
						)
						.await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "SELECT failed: {:?}", resp.error);
					let rows = resp
						.results
						.first()
						.and_then(|r| r.rows.as_ref())
						.map(|r| r.len())
						.unwrap_or(0);
					anyhow::ensure!(rows == 0, "expected id {} to be deleted", deleted_id);
				}

				for updated_id in [10, 20, 30] {
					let resp = server
						.execute_sql_with_auth(
							&format!("SELECT data FROM {}.{} WHERE id = {}", ns, table, updated_id),
							&auth_a,
						)
						.await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "SELECT failed: {:?}", resp.error);
					let row = resp
						.results
						.first()
						.and_then(|r| r.row_as_map(0))
						.ok_or_else(|| anyhow::anyhow!("Missing row for id {}", updated_id))?;
					let val = row
						.get("data")
						.and_then(|v| v.as_str())
						.ok_or_else(|| anyhow::anyhow!("Missing data for id {}", updated_id))?;
					anyhow::ensure!(
						val == format!("updated_{}", updated_id),
						"unexpected value for id {}: {}",
						updated_id,
						val
					);
				}
			}

			// -----------------------------------------------------------------
			// test_queries_remain_consistent_during_flush
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_consistency_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:8").await?;

				for i in 1..=10 {
					let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'v{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "INSERT failed: {:?}", resp.error);
				}

				let flusher = async {
					for _ in 0..1 {
						flush_table_and_wait(server, &ns, table).await?;
						sleep(Duration::from_millis(10)).await;
					}
					anyhow::Ok(())
				};

				let reader = async {
					for _ in 0..2 {
						let cnt = count_rows(server, &auth_a, &ns, table).await?;
						anyhow::ensure!(cnt == 10, "expected count=10 during flush, got {}", cnt);
						sleep(Duration::from_millis(5)).await;
					}
					anyhow::Ok(())
				};

				let (f, r) = tokio::join!(flusher, reader);
				f?;
				r?;

				wait_for_flush_jobs_settled(server, &ns, table).await?;
			}

			// -----------------------------------------------------------------
			// test_flush_preserves_read_visibility
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_vis_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:20").await?;

				for i in 1..=10 {
					let insert_sql =
						format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'val_{i}')");
					let resp = server.execute_sql_with_auth(&insert_sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert {} failed: {:?}", i, resp.error);
				}

				flush_table_and_wait(server, &ns, table).await?;

				for i in 11..=13 {
					let insert_sql =
						format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'val_{i}')");
					let resp = server.execute_sql_with_auth(&insert_sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert {} failed: {:?}", i, resp.error);
				}

				flush_table_and_wait(server, &ns, table).await?;

				let cnt = count_rows(server, &auth_a, &ns, table).await?;
				anyhow::ensure!(cnt == 13, "expected all rows visible after multiple flushes, got {}", cnt);
			}

			// -----------------------------------------------------------------
			// test_updates_persist_across_flushes
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_update_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:10").await?;

				let insert = format!("INSERT INTO {ns}.{table} (id, data) VALUES (1, 'old')");
				let resp = server.execute_sql_with_auth(&insert, &auth_a).await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);

				flush_table_and_wait(server, &ns, table).await?;

				let update = format!("UPDATE {ns}.{table} SET data = 'new' WHERE id = 1");
				let resp = server.execute_sql_with_auth(&update, &auth_a).await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "update failed: {:?}", resp.error);

				flush_table_and_wait(server, &ns, table).await?;

				let resp = server
					.execute_sql_with_auth(&format!("SELECT data FROM {ns}.{table} WHERE id = 1"), &auth_a)
					.await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
				let row = resp
					.results
					.first()
					.and_then(|r| r.row_as_map(0))
					.ok_or_else(|| anyhow::anyhow!("Missing row"))?;
				let val = row.get("data").and_then(|v| v.as_str()).unwrap_or("");
				anyhow::ensure!(val == "new", "expected 'new', got '{}'", val);
			}

			// -----------------------------------------------------------------
			// test_deletes_persist_across_flushes
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_delete_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:15").await?;

				for i in 1..=5 {
					let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'v{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}

				flush_table_and_wait(server, &ns, table).await?;

				let delete_sql = format!("DELETE FROM {ns}.{table} WHERE id = 3");
				let resp = server.execute_sql_with_auth(&delete_sql, &auth_a).await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "delete failed: {:?}", resp.error);

				flush_table_and_wait(server, &ns, table).await?;

				let cnt = count_rows(server, &auth_a, &ns, table).await?;
				anyhow::ensure!(cnt == 4, "delete should persist after flush, got {}", cnt);

				let resp = server
					.execute_sql_with_auth(&format!("SELECT id FROM {ns}.{table} WHERE id = 3"), &auth_a)
					.await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
				let rows = resp
					.results
					.first()
					.and_then(|r| r.rows.as_ref())
					.map(|r| r.len())
					.unwrap_or(0);
				anyhow::ensure!(rows == 0, "expected id=3 to be deleted");
			}

			// -----------------------------------------------------------------
			// test_interleaved_batches_and_flushes
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_interleave_ns_{}", suffix);
				let table = "items";
				create_user_table(server, &ns, table, "rows:25").await?;

				for i in 1..=10 {
					let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'p{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}
				flush_table_and_wait(server, &ns, table).await?;

				for i in 11..=15 {
					let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'p{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}
				for id in [5] {
					let sql = format!("UPDATE {ns}.{table} SET data = 'upd_{id}' WHERE id = {id}");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "update failed: {:?}", resp.error);
				}
				flush_table_and_wait(server, &ns, table).await?;

				for id in [3] {
					let sql = format!("DELETE FROM {ns}.{table} WHERE id = {id}");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "delete failed: {:?}", resp.error);
				}
				flush_table_and_wait(server, &ns, table).await?;

				let cnt = count_rows(server, &auth_a, &ns, table).await?;
				anyhow::ensure!(cnt == 14, "should reflect inserts minus deletions, got {}", cnt);

				for id in [5] {
					let resp = server
						.execute_sql_with_auth(&format!("SELECT data FROM {ns}.{table} WHERE id = {id}"), &auth_a)
						.await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
					let row = resp
						.results
						.first()
						.and_then(|r| r.row_as_map(0))
						.ok_or_else(|| anyhow::anyhow!("Missing row for id {}", id))?;
					let val = row.get("data").and_then(|v| v.as_str()).unwrap_or("");
					anyhow::ensure!(val == format!("upd_{}", id), "unexpected value for id {}: {}", id, val);
				}

				for id in [3] {
					let resp = server
						.execute_sql_with_auth(&format!("SELECT id FROM {ns}.{table} WHERE id = {id}"), &auth_a)
						.await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
					let rows = resp
						.results
						.first()
						.and_then(|r| r.rows.as_ref())
						.map(|r| r.len())
						.unwrap_or(0);
					anyhow::ensure!(rows == 0, "expected id {} to be deleted", id);
				}
			}

			// -----------------------------------------------------------------
			// test_flush_isolation_between_tables
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_iso_ns_{}", suffix);
				let t1 = "alpha";
				let t2 = "beta";
				create_user_table(server, &ns, t1, "rows:5").await?;
				create_user_table(server, &ns, t2, "rows:5").await?;

				for i in 1..=4 {
					let sql = format!("INSERT INTO {ns}.{t1} (id, data) VALUES ({i}, 'a{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}
				for i in 1..=3 {
					let sql = format!("INSERT INTO {ns}.{t2} (id, data) VALUES ({i}, 'b{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}

				flush_table_and_wait(server, &ns, t1).await?;

				for i in 4..=5 {
					let sql = format!("INSERT INTO {ns}.{t2} (id, data) VALUES ({i}, 'b{i}')");
					let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
					anyhow::ensure!(resp.status == ResponseStatus::Success, "insert failed: {:?}", resp.error);
				}

				flush_table_and_wait(server, &ns, t2).await?;

				let cnt1 = count_rows(server, &auth_a, &ns, t1).await?;
				let cnt2 = count_rows(server, &auth_a, &ns, t2).await?;
				anyhow::ensure!(cnt1 == 4, "expected t1 count=4, got {}", cnt1);
				anyhow::ensure!(cnt2 == 5, "expected t2 count=5, got {}", cnt2);
			}

			// -----------------------------------------------------------------
			// test_concurrent_flushes_across_tables
			// -----------------------------------------------------------------
			{
				let ns = format!("flush_concurrent_multi_ns_{}", suffix);
				let t1 = "alpha";
				let t2 = "beta";
				create_user_table(server, &ns, t1, "rows:30").await?;
				create_user_table(server, &ns, t2, "rows:30").await?;

				let writer1 = async {
					for i in 1..=30 {
						let sql = format!("INSERT INTO {ns}.{t1} (id, data) VALUES ({i}, 'a{i}')");
						let resp = server.execute_sql_with_auth(&sql, &auth_a).await?;
						anyhow::ensure!(
							resp.status == ResponseStatus::Success,
							"t1 insert {} failed: {:?}",
							i,
							resp.error
						);
						if i % 10 == 0 {
							let upd = format!("UPDATE {ns}.{t1} SET data = 'upd_{i}' WHERE id = {i}");
							let resp = server.execute_sql_with_auth(&upd, &auth_a).await?;
							anyhow::ensure!(
								resp.status == ResponseStatus::Success,
								"t1 update {} failed: {:?}",
								i,
								resp.error
							);
						}
					}
					anyhow::Ok(())
				};

				let writer2 = async {
					for i in 1..=30 {
						let sql = format!("INSERT INTO {ns}.{t2} (id, data) VALUES ({i}, 'b{i}')");
						let resp = server.execute_sql_with_auth(&sql, &auth_b).await?;
						anyhow::ensure!(
							resp.status == ResponseStatus::Success,
							"t2 insert {} failed: {:?}",
							i,
							resp.error
						);
						if i % 10 == 0 {
							let del = format!("DELETE FROM {ns}.{t2} WHERE id = {}", i - 5);
							let resp = server.execute_sql_with_auth(&del, &auth_b).await?;
							anyhow::ensure!(
								resp.status == ResponseStatus::Success,
								"t2 delete failed: {:?}",
								resp.error
							);
						}
					}
					anyhow::Ok(())
				};

				let flusher = async {
					for _ in 0..2 {
						flush_table_and_wait(server, &ns, t1).await?;
						flush_table_and_wait(server, &ns, t2).await?;
						sleep(Duration::from_millis(10)).await;
					}
					anyhow::Ok(())
				};

				let (w1, w2, f) = tokio::join!(writer1, writer2, flusher);
				w1?;
				w2?;
				f?;

				flush_table_and_wait(server, &ns, t1).await?;
				flush_table_and_wait(server, &ns, t2).await?;

				let cnt1 = count_rows(server, &auth_a, &ns, t1).await?;
				anyhow::ensure!(cnt1 == 30, "t1 expected 30 rows, got {}", cnt1);

				let cnt2 = count_rows(server, &auth_b, &ns, t2).await?;
				anyhow::ensure!(cnt2 == 27, "t2 expected 27 rows after deletes, got {}", cnt2);

				let resp = server
					.execute_sql_with_auth(&format!("SELECT data FROM {ns}.{t1} WHERE id = 30"), &auth_a)
					.await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
				let row = resp
					.results
					.first()
					.and_then(|r| r.row_as_map(0))
					.ok_or_else(|| anyhow::anyhow!("Missing row"))?;
				let val = row.get("data").and_then(|v| v.as_str()).unwrap_or("");
				anyhow::ensure!(val == "upd_30", "expected upd_30, got {}", val);

				let resp = server
					.execute_sql_with_auth(&format!("SELECT id FROM {ns}.{t2} WHERE id = 15"), &auth_b)
					.await?;
				anyhow::ensure!(resp.status == ResponseStatus::Success, "select failed: {:?}", resp.error);
				let rows = resp
					.results
					.first()
					.and_then(|r| r.rows.as_ref())
					.map(|r| r.len())
					.unwrap_or(0);
				anyhow::ensure!(rows == 0, "expected id=30 to be deleted");
			}

			Ok(())
		})
	})
	.await
	.expect("with_http_test_server_timeout");
}