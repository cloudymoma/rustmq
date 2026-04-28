mod common;

use rustmq::controller::*;
use std::collections::BTreeMap;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

async fn setup_raft_node(node_id: NodeId, temp_dir: &TempDir) -> RaftManager {
    let mut config = RaftManagerConfig::default();
    config.node_id = node_id;
    config.storage_config.data_dir = temp_dir.path().join(format!("node-{}", node_id));

    // Set fast election timeouts for testing
    config.raft_config.election_timeout_min = 200;
    config.raft_config.election_timeout_max = 400;
    config.raft_config.heartbeat_interval = 50;
    
    // Set unique RPC port for each node
    config.rpc_port = 9090 + node_id as u16;

    let mut manager = RaftManager::new(config).await.unwrap();
    manager.initialize().await.unwrap();
    manager.start().await.unwrap();
    manager
}

#[tokio::test]
async fn test_raft_cluster_formation_3_nodes() {
    let temp_dir = TempDir::new().unwrap();

    // 1. Start 3 nodes
    let mut node1 = setup_raft_node(1, &temp_dir).await;
    let mut node2 = setup_raft_node(2, &temp_dir).await;
    let mut node3 = setup_raft_node(3, &temp_dir).await;

    // 2. Initialize the cluster from node 1
    let mut nodes = BTreeMap::new();
    nodes.insert(
        1,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9091,
            data: "".to_string(),
        },
    );
    nodes.insert(
        2,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9092,
            data: "".to_string(),
        },
    );
    nodes.insert(
        3,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9093,
            data: "".to_string(),
        },
    );

    node1.initialize_cluster(nodes).await.unwrap();

    // 3. Wait for leader election
    let mut leader_found = false;
    for _ in 0..20 {
        // 10 seconds timeout
        if node1.is_leader().await || node2.is_leader().await || node3.is_leader().await {
            leader_found = true;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(
        leader_found,
        "Cluster failed to elect a leader within timeout"
    );

    // 4. Verify all nodes reach Running state
    assert_eq!(node1.get_state().await, ManagerState::Running);
    assert_eq!(node2.get_state().await, ManagerState::Running);
    assert_eq!(node3.get_state().await, ManagerState::Running);

    // 5. Cleanup
    node1.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
    node3.shutdown().await.unwrap();
}
