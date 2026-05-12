mod common;

use rustmq::controller::service::TopicConfig;
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
    config.rpc_port = 9100 + node_id as u16; // Use different ports from other tests

    let mut manager = RaftManager::new(config).await.unwrap();
    manager.initialize().await.unwrap();
    manager.start().await.unwrap();
    manager
}

#[tokio::test]
async fn test_raft_network_partition() {
    let temp_dir = TempDir::new().unwrap();

    // 1. Start 3 nodes
    let mut node1 = setup_raft_node(1, &temp_dir).await;
    let mut node2 = setup_raft_node(2, &temp_dir).await;
    let mut node3 = setup_raft_node(3, &temp_dir).await;

    // 2. Initialize the cluster
    let mut nodes = BTreeMap::new();
    nodes.insert(
        1,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9101,
            data: "".to_string(),
        },
    );
    nodes.insert(
        2,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9102,
            data: "".to_string(),
        },
    );
    nodes.insert(
        3,
        RustMqNode {
            addr: "127.0.0.1".to_string(),
            rpc_port: 9103,
            data: "".to_string(),
        },
    );

    node1.initialize_cluster(nodes).await.unwrap();

    // 3. Wait for leader election
    let mut leader_id = 0;
    for _ in 0..20 {
        if node1.is_leader().await {
            leader_id = 1;
            break;
        }
        if node2.is_leader().await {
            leader_id = 2;
            break;
        }
        if node3.is_leader().await {
            leader_id = 3;
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }
    assert!(leader_id > 0, "Cluster failed to elect a leader");
    println!("Leader elected: Node {}", leader_id);

    // Get the leader manager
    let leader = match leader_id {
        1 => &node1,
        2 => &node2,
        3 => &node3,
        _ => unreachable!(),
    };

    // 4. Block communication to one follower (e.g., Node 3 if Node 1 is leader)
    let follower_to_block = if leader_id == 1 {
        3
    } else if leader_id == 2 {
        3
    } else {
        1
    };

    println!(
        "Simulating partition: Blocking communication from Node {} to Node {}",
        leader_id, follower_to_block
    );

    leader
        .network()
        .unwrap()
        .block_node(follower_to_block)
        .await;

    // 5. Propose a command to leader. It should still succeed because it can reach the other follower (majority).
    let app_data = RustMqAppData::CreateTopic {
        name: "test-partition-topic".to_string(),
        partitions: 1,
        replication_factor: 3,
        config: TopicConfig::default(),
    };

    let response = leader.apply_command(app_data.clone()).await;
    assert!(
        response.is_ok(),
        "Leader should still be able to commit with majority"
    );
    println!("Command committed successfully during partial partition");

    // 6. Now isolate the leader completely (block the other follower too)
    let other_follower = if leader_id == 1 {
        2
    } else if leader_id == 2 {
        1
    } else {
        2
    };
    println!(
        "Simulating partition: Blocking communication from Node {} to Node {}",
        leader_id, other_follower
    );
    leader.network().unwrap().block_node(other_follower).await;

    // 7. Propose another command. It should FAIL or TIMEOUT because leader cannot reach majority.
    // Use a timeout to prevent hanging forever.
    let response =
        tokio::time::timeout(Duration::from_secs(5), leader.apply_command(app_data)).await;
    assert!(
        response.is_err() || response.unwrap().is_err(),
        "Leader should fail to commit when isolated"
    );
    println!("Command failed or timed out as expected when leader is isolated");

    // 8. Cleanup
    node1.shutdown().await.unwrap();
    node2.shutdown().await.unwrap();
    node3.shutdown().await.unwrap();
}
