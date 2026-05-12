mod common;

use rustmq::controller::service::TopicConfig;
use rustmq::controller::*;
use std::collections::BTreeMap;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

async fn setup_raft_node(
    node_id: NodeId,
    temp_dir: &TempDir,
    snapshot_threshold: u64,
) -> RaftManager {
    let mut config = RaftManagerConfig::default();
    config.node_id = node_id;
    config.storage_config.data_dir = temp_dir.path().join(format!("node-{}", node_id));
    config.storage_config.snapshot_threshold = snapshot_threshold;

    // Set fast election timeouts for testing
    config.raft_config.election_timeout_min = 200;
    config.raft_config.election_timeout_max = 400;
    config.raft_config.heartbeat_interval = 50;

    let mut manager = RaftManager::new(config).await.unwrap();
    manager.initialize().await.unwrap();
    manager.start().await.unwrap();
    manager
}

#[tokio::test]
async fn test_raft_snapshot_trigger() {
    let temp_dir = TempDir::new().unwrap();
    let node_id = 1;

    // Start node with threshold 5
    let mut node = setup_raft_node(node_id, &temp_dir, 5).await;

    let mut nodes = BTreeMap::new();
    nodes.insert(
        node_id,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9091,
            data: "".to_string(),
        },
    );

    node.initialize_cluster(nodes).await.unwrap();

    // Wait for node to become leader
    let mut is_leader = false;
    for _ in 0..20 {
        if node.is_leader().await {
            is_leader = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(is_leader, "Node failed to become leader");

    // Propose data to trigger snapshot (threshold is 5)
    for i in 0..10 {
        let app_data = RustMqAppData::CreateTopic {
            name: format!("topic-{}", i),
            partitions: 1,
            replication_factor: 1,
            config: TopicConfig::default(),
        };

        let response = node.apply_command(app_data).await.unwrap();
        assert!(response.success);
    }

    // Wait for snapshot to be triggered
    sleep(Duration::from_secs(2)).await;

    // Verify snapshot file exists
    let snapshot_file = temp_dir
        .path()
        .join(format!("node-{}/snapshot.bin", node_id));
    assert!(
        snapshot_file.exists(),
        "Snapshot file should exist at {:?}",
        snapshot_file
    );

    node.shutdown().await.unwrap();
}
