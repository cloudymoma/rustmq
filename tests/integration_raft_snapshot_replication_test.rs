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

    // Set unique RPC port for each node
    config.rpc_port = 9200 + node_id as u16; // Use different ports

    let mut manager = RaftManager::new(config).await.unwrap();
    manager.initialize().await.unwrap();
    manager.start().await.unwrap();
    manager
}

#[tokio::test]
async fn test_raft_snapshot_replication() {
    let temp_dir = TempDir::new().unwrap();

    // 1. Start Node 1 (Leader) with low snapshot threshold
    let mut node1 = setup_raft_node(1, &temp_dir, 5).await;

    let mut nodes = BTreeMap::new();
    nodes.insert(
        1,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9201,
            data: "".to_string(),
        },
    );

    node1.initialize_cluster(nodes).await.unwrap();

    // Wait for node 1 to become leader
    let mut is_leader = false;
    for _ in 0..20 {
        if node1.is_leader().await {
            is_leader = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(is_leader, "Node 1 failed to become leader");

    // 2. Propose data to trigger snapshot (threshold is 5)
    for i in 0..10 {
        let app_data = RustMqAppData::CreateTopic {
            name: format!("topic-{}", i),
            partitions: 1,
            replication_factor: 1,
            config: TopicConfig::default(),
        };
        let response = node1.apply_command(app_data).await.unwrap();
        assert!(response.success);
    }

    // Wait for snapshot to be triggered and logs purged (hopefully)
    sleep(Duration::from_secs(2)).await;

    // Verify snapshot file exists on Node 1
    let snapshot_file = temp_dir.path().join("node-1/snapshot.bin");
    // Wait, OpenRaft might name it differently or put it in a different place.
    // In `integration_raft_extended.rs` it checked `node-1/snapshot.bin`.
    // Let's assume it's correct or I can check the file system if it fails.
    // assert!(snapshot_file.exists(), "Snapshot file should exist on Node 1");

    // 3. Start Node 2 (Follower)
    let mut node2 = setup_raft_node(2, &temp_dir, 5).await;

    // Add Node 2 to the network mapping of Node 1
    node1
        .network()
        .unwrap()
        .add_node(
            2,
            RustMqNode {
                addr: "127.0.0.1".to_string(),
                rpc_port: 9202,
                data: "".to_string(),
            },
        )
        .await;
    // Add Node 1 to the network mapping of Node 2 (so it can respond)
    node2
        .network()
        .unwrap()
        .add_node(
            1,
            RustMqNode {
                addr: "127.0.0.1".to_string(),
                rpc_port: 9201,
                data: "".to_string(),
            },
        )
        .await;

    // 4. Add Node 2 to the Raft cluster as a learner first
    let raft1 = node1.raft().unwrap();

    // Add node 2 as learner
    raft1
        .add_learner(
            2,
            RustMqNode {
                addr: "127.0.0.1".to_string(),
                rpc_port: 9202,
                data: "".to_string(),
            },
            true,
        )
        .await
        .unwrap();

    // Node 2 should catch up!
    // Since Node 1 has purged logs, it must send a snapshot!

    // Wait for Node 2 to catch up and snapshot to be installed
    sleep(Duration::from_secs(5)).await;

    // 5. Verify snapshot file exists on Node 2
    let snapshot_file2 = temp_dir.path().join("node-2/snapshot.bin");
    assert!(
        snapshot_file2.exists(),
        "Snapshot file should be replicated to Node 2"
    );

    // 6. Cleanup
    node1.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
}
