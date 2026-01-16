//! Cluster view tests: CLUSTER LIST/STATUS output

use crate::cluster_common;

#[ntest::timeout(60_000)]
#[test]
fn cluster_list_output_contains_overview() {
    if !cluster_common::require_cluster_running() {
        return;
    }

    let urls = cluster_common::cluster_urls();
    for url in &urls {
        let output = cluster_common::execute_on_node(url, "CLUSTER LIST")
            .expect("CLUSTER LIST failed");
        assert!(
            output.contains("CLUSTER OVERVIEW"),
            "missing overview in CLUSTER LIST output for {}",
            url
        );
        assert!(
            output.contains("NODES"),
            "missing NODES section in CLUSTER LIST output for {}",
            url
        );
    }
}

#[ntest::timeout(60_000)]
#[test]
fn cluster_status_aliases_render() {
    if !cluster_common::require_cluster_running() {
        return;
    }

    let urls = cluster_common::cluster_urls();
    let Some(url) = urls.first() else {
        return;
    };

    for cmd in ["CLUSTER STATUS", "CLUSTER LS"] {
        let output = cluster_common::execute_on_node(url, cmd)
            .expect("cluster status alias failed");
        assert!(
            output.contains("CLUSTER OVERVIEW"),
            "{} output missing overview",
            cmd
        );
    }
}