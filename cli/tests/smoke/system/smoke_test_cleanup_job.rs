use crate::common::*;
use std::time::Duration;

fn parse_cleanup_job_id(output: &str) -> Result<String, Box<dyn std::error::Error>> {
    let marker = "Cleanup job: ";
    if let Some(idx) = output.find(marker) {
        let after = &output[idx + marker.len()..];
        let job_id = after
            .split_whitespace()
            .next()
            .ok_or("Missing cleanup job id after marker")?
            .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';')
            .to_string();
        if job_id.is_empty() {
            return Err("Cleanup job id was empty".into());
        }
        return Ok(job_id);
    }

    Err(format!("Failed to parse cleanup job id from output: {}", output).into())
}

#[test]
fn smoke_cleanup_job_completes() {
    if !require_server_running() {
        return;
    }

    let namespace = generate_unique_namespace("cleanup_job_ns");
    let table = generate_unique_table("cleanup_job_table");
    let full_table = format!("{}.{}", namespace, table);

    let _ = execute_sql_as_root_via_cli(&format!("DROP TABLE IF EXISTS {}", full_table));
    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));

    execute_sql_as_root_via_cli(&format!("CREATE NAMESPACE {}", namespace))
        .expect("create namespace");

    execute_sql_as_root_via_cli(&format!(
        "CREATE TABLE {} (id INT PRIMARY KEY, value TEXT) WITH (TYPE = 'SHARED')",
        full_table
    ))
    .expect("create table");

    execute_sql_as_root_via_cli(&format!(
        "INSERT INTO {} (id, value) VALUES (1, 'cleanup')",
        full_table
    ))
    .expect("insert row");

    let drop_output = execute_sql_as_root_via_cli(&format!("DROP TABLE {}", full_table))
        .expect("drop table");

    let job_id = parse_cleanup_job_id(&drop_output).expect("parse cleanup job id");
    let status = wait_for_job_finished(&job_id, Duration::from_secs(30))
        .expect("wait for cleanup job to finish");

    assert_eq!(status, "completed", "cleanup job did not complete");

    let _ = execute_sql_as_root_via_cli(&format!("DROP NAMESPACE IF EXISTS {} CASCADE", namespace));
}
