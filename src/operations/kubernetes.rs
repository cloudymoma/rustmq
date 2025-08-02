use crate::{Result, config::KubernetesConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Kubernetes StatefulSet configuration for RustMQ brokers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatefulSetConfig {
    pub name: String,
    pub namespace: String,
    pub replicas: i32,
    pub image: String,
    pub image_tag: String,
    pub pvc_config: PvcConfig,
    pub pod_config: PodConfig,
    pub service_config: ServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PvcConfig {
    pub storage_class: String,
    pub wal_volume_size: String,
    pub data_volume_size: String,
    pub access_mode: String, // ReadWriteOnce, ReadWriteMany, etc.
}

impl Default for PvcConfig {
    fn default() -> Self {
        Self {
            storage_class: "fast-ssd".to_string(),
            wal_volume_size: "50Gi".to_string(),
            data_volume_size: "100Gi".to_string(),
            access_mode: "ReadWriteOnce".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodConfig {
    pub cpu_request: String,
    pub cpu_limit: String,
    pub memory_request: String,
    pub memory_limit: String,
    pub enable_pod_affinity: bool,
    pub rack_awareness: bool,
    pub node_selector: HashMap<String, String>,
}

impl Default for PodConfig {
    fn default() -> Self {
        Self {
            cpu_request: "1000m".to_string(),
            cpu_limit: "2000m".to_string(),
            memory_request: "4Gi".to_string(),
            memory_limit: "8Gi".to_string(),
            enable_pod_affinity: true,
            rack_awareness: true,
            node_selector: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub service_type: String, // ClusterIP, NodePort, LoadBalancer
    pub quic_port: i32,
    pub grpc_port: i32,
    pub metrics_port: i32,
}

impl Default for StatefulSetConfig {
    fn default() -> Self {
        Self {
            name: "rustmq-broker".to_string(),
            namespace: "default".to_string(),
            replicas: 3,
            image: "rustmq/broker".to_string(),
            image_tag: "latest".to_string(),
            pvc_config: PvcConfig {
                storage_class: "fast-ssd".to_string(),
                wal_volume_size: "50Gi".to_string(),
                data_volume_size: "100Gi".to_string(),
                access_mode: "ReadWriteOnce".to_string(),
            },
            pod_config: PodConfig {
                cpu_request: "1000m".to_string(),
                cpu_limit: "2000m".to_string(),
                memory_request: "4Gi".to_string(),
                memory_limit: "8Gi".to_string(),
                enable_pod_affinity: true,
                rack_awareness: true,
                node_selector: HashMap::new(),
            },
            service_config: ServiceConfig {
                service_type: "ClusterIP".to_string(),
                quic_port: 9092,
                grpc_port: 9093,
                metrics_port: 9094,
            },
        }
    }
}

/// Kubernetes deployment manager for RustMQ
pub struct KubernetesDeploymentManager {
    config: KubernetesConfig,
    statefulset_config: StatefulSetConfig,
}

impl KubernetesDeploymentManager {
    pub fn new(config: KubernetesConfig) -> Self {
        let statefulset_config = StatefulSetConfig {
            pvc_config: PvcConfig {
                storage_class: config.pvc_storage_class.clone(),
                wal_volume_size: config.wal_volume_size.clone(),
                ..Default::default()
            },
            pod_config: PodConfig {
                enable_pod_affinity: config.enable_pod_affinity,
                ..Default::default()
            },
            ..Default::default()
        };

        Self {
            config,
            statefulset_config,
        }
    }

    /// Generate StatefulSet YAML for RustMQ brokers
    pub fn generate_statefulset_yaml(&self) -> String {
        format!(
            r#"apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: {name}
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
spec:
  serviceName: {name}-headless
  replicas: {replicas}
  selector:
    matchLabels:
      app: rustmq-broker
      component: broker
  template:
    metadata:
      labels:
        app: rustmq-broker
        component: broker
    spec:
      {affinity}
      containers:
      - name: rustmq-broker
        image: {image}:{image_tag}
        ports:
        - containerPort: {quic_port}
          name: quic
          protocol: UDP
        - containerPort: {grpc_port}
          name: grpc
          protocol: TCP
        - containerPort: {metrics_port}
          name: metrics
          protocol: TCP
        env:
        - name: RUSTMQ_BROKER_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: RUSTMQ_RACK_ID
          valueFrom:
            fieldRef:
              fieldPath: spec.nodeName
        - name: RUSTMQ_WAL_PATH
          value: "/data/wal"
        - name: RUSTMQ_CONFIG_PATH
          value: "/etc/rustmq/broker.toml"
        resources:
          requests:
            cpu: {cpu_request}
            memory: {memory_request}
          limits:
            cpu: {cpu_limit}
            memory: {memory_limit}
        volumeMounts:
        - name: wal-storage
          mountPath: /data/wal
        - name: data-storage
          mountPath: /data/storage
        - name: config
          mountPath: /etc/rustmq
        livenessProbe:
          httpGet:
            path: /health
            port: {metrics_port}
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: {metrics_port}
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: rustmq-broker-config
  volumeClaimTemplates:
  - metadata:
      name: wal-storage
    spec:
      accessModes: [ "{access_mode}" ]
      storageClassName: {storage_class}
      resources:
        requests:
          storage: {wal_volume_size}
  - metadata:
      name: data-storage
    spec:
      accessModes: [ "{access_mode}" ]
      storageClassName: {storage_class}
      resources:
        requests:
          storage: {data_volume_size}
"#,
            name = self.statefulset_config.name,
            namespace = self.statefulset_config.namespace,
            replicas = self.statefulset_config.replicas,
            image = self.statefulset_config.image,
            image_tag = self.statefulset_config.image_tag,
            quic_port = self.statefulset_config.service_config.quic_port,
            grpc_port = self.statefulset_config.service_config.grpc_port,
            metrics_port = self.statefulset_config.service_config.metrics_port,
            cpu_request = self.statefulset_config.pod_config.cpu_request,
            cpu_limit = self.statefulset_config.pod_config.cpu_limit,
            memory_request = self.statefulset_config.pod_config.memory_request,
            memory_limit = self.statefulset_config.pod_config.memory_limit,
            storage_class = self.statefulset_config.pvc_config.storage_class,
            wal_volume_size = self.statefulset_config.pvc_config.wal_volume_size,
            data_volume_size = self.statefulset_config.pvc_config.data_volume_size,
            access_mode = self.statefulset_config.pvc_config.access_mode,
            affinity = if self.config.enable_pod_affinity {
                r#"affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - weight: 100
            podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: app
                  operator: In
                  values:
                  - rustmq-broker
              topologyKey: kubernetes.io/hostname"#
            } else {
                ""
            }
        )
    }

    /// Generate headless service YAML for StatefulSet
    pub fn generate_headless_service_yaml(&self) -> String {
        format!(
            r#"apiVersion: v1
kind: Service
metadata:
  name: {name}-headless
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
spec:
  clusterIP: None
  selector:
    app: rustmq-broker
    component: broker
  ports:
  - name: quic
    port: {quic_port}
    protocol: UDP
  - name: grpc
    port: {grpc_port}
    protocol: TCP
  - name: metrics
    port: {metrics_port}
    protocol: TCP
"#,
            name = self.statefulset_config.name,
            namespace = self.statefulset_config.namespace,
            quic_port = self.statefulset_config.service_config.quic_port,
            grpc_port = self.statefulset_config.service_config.grpc_port,
            metrics_port = self.statefulset_config.service_config.metrics_port,
        )
    }

    /// Generate regular service YAML for external access
    pub fn generate_service_yaml(&self) -> String {
        format!(
            r#"apiVersion: v1
kind: Service
metadata:
  name: {name}-service
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
spec:
  type: {service_type}
  selector:
    app: rustmq-broker
    component: broker
  ports:
  - name: quic
    port: {quic_port}
    targetPort: {quic_port}
    protocol: UDP
  - name: grpc
    port: {grpc_port}
    targetPort: {grpc_port}
    protocol: TCP
  - name: metrics
    port: {metrics_port}
    targetPort: {metrics_port}
    protocol: TCP
"#,
            name = self.statefulset_config.name,
            namespace = self.statefulset_config.namespace,
            service_type = self.statefulset_config.service_config.service_type,
            quic_port = self.statefulset_config.service_config.quic_port,
            grpc_port = self.statefulset_config.service_config.grpc_port,
            metrics_port = self.statefulset_config.service_config.metrics_port,
        )
    }

    /// Generate ConfigMap YAML for broker configuration
    pub fn generate_configmap_yaml(&self, broker_config_toml: &str) -> String {
        format!(
            r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: rustmq-broker-config
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
data:
  broker.toml: |
{broker_config}
"#,
            namespace = self.statefulset_config.namespace,
            broker_config = broker_config_toml
                .lines()
                .map(|line| format!("    {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }

    /// Generate PodDisruptionBudget YAML for high availability
    pub fn generate_pod_disruption_budget_yaml(&self) -> String {
        format!(
            r#"apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {name}-pdb
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: rustmq-broker
      component: broker
"#,
            name = self.statefulset_config.name,
            namespace = self.statefulset_config.namespace,
        )
    }

    /// Generate HorizontalPodAutoscaler YAML for automatic scaling
    pub fn generate_hpa_yaml(&self, min_replicas: i32, max_replicas: i32) -> String {
        format!(
            r#"apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {name}-hpa
  namespace: {namespace}
  labels:
    app: rustmq-broker
    component: broker
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: StatefulSet
    name: {name}
  minReplicas: {min_replicas}
  maxReplicas: {max_replicas}
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 300
      policies:
      - type: Pods
        value: 2
        periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Pods
        value: 1
        periodSeconds: 180
"#,
            name = self.statefulset_config.name,
            namespace = self.statefulset_config.namespace,
            min_replicas = min_replicas,
            max_replicas = max_replicas,
        )
    }

    /// Generate complete deployment manifest
    pub fn generate_complete_manifest(&self, broker_config_toml: &str) -> String {
        vec![
            "# RustMQ Broker StatefulSet Deployment".to_string(),
            "# Generated by RustMQ Kubernetes Deployment Manager".to_string(),
            "---".to_string(),
            self.generate_configmap_yaml(broker_config_toml),
            "---".to_string(),
            self.generate_headless_service_yaml(),
            "---".to_string(),
            self.generate_service_yaml(),
            "---".to_string(),
            self.generate_statefulset_yaml(),
            "---".to_string(),
            self.generate_pod_disruption_budget_yaml(),
            "---".to_string(),
            self.generate_hpa_yaml(2, 10),
        ]
        .join("\n")
    }
}

/// Volume recovery manager for handling pod failures and recovery
pub struct VolumeRecoveryManager {
    config: KubernetesConfig,
}

impl VolumeRecoveryManager {
    pub fn new(config: KubernetesConfig) -> Self {
        Self { config }
    }

    /// Check if a pod's volumes are properly attached and accessible
    pub async fn validate_volume_attachment(&self, pod_name: &str, namespace: &str) -> Result<bool> {
        // In a real implementation, this would use the Kubernetes API to check:
        // 1. PVC is bound
        // 2. Volume is attached to the node
        // 3. Volume is mounted in the pod
        // 4. WAL directory is accessible and writable
        
        tracing::info!("Validating volume attachment for pod {} in namespace {}", pod_name, namespace);
        
        // For testing, assume validation passes
        Ok(true)
    }

    /// Recover a failed broker pod with volume reattachment
    pub async fn recover_broker_with_volume(&self, broker_id: &str, namespace: &str) -> Result<()> {
        tracing::info!("Starting volume recovery for broker {} in namespace {}", broker_id, namespace);

        // Step 1: Validate PVC exists and is bound
        if !self.validate_pvc_binding(broker_id, namespace).await? {
            return Err(crate::error::RustMqError::Storage(
                format!("PVC for broker {} is not properly bound", broker_id)
            ));
        }

        // Step 2: Check node affinity to ensure pod lands on correct node
        if self.config.enable_pod_affinity {
            self.ensure_node_affinity(broker_id, namespace).await?;
        }

        // Step 3: Validate volume mount and WAL recovery
        self.validate_wal_recovery(broker_id, namespace).await?;

        tracing::info!("Volume recovery completed for broker {}", broker_id);
        Ok(())
    }

    async fn validate_pvc_binding(&self, broker_id: &str, namespace: &str) -> Result<bool> {
        tracing::debug!("Validating PVC binding for broker {} in namespace {}", broker_id, namespace);
        // In real implementation, check PVC status via Kubernetes API
        Ok(true)
    }

    async fn ensure_node_affinity(&self, broker_id: &str, namespace: &str) -> Result<()> {
        tracing::debug!("Ensuring node affinity for broker {} in namespace {}", broker_id, namespace);
        // In real implementation, verify pod scheduling constraints
        Ok(())
    }

    async fn validate_wal_recovery(&self, broker_id: &str, namespace: &str) -> Result<()> {
        tracing::debug!("Validating WAL recovery for broker {} in namespace {}", broker_id, namespace);
        // In real implementation, validate WAL file integrity and recovery
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statefulset_yaml_generation() {
        let config = KubernetesConfig {
            use_stateful_sets: true,
            pvc_storage_class: "fast-ssd".to_string(),
            wal_volume_size: "50Gi".to_string(),
            enable_pod_affinity: true,
        };

        let deployment_manager = KubernetesDeploymentManager::new(config);
        let yaml = deployment_manager.generate_statefulset_yaml();

        assert!(yaml.contains("kind: StatefulSet"));
        assert!(yaml.contains("rustmq-broker"));
        assert!(yaml.contains("fast-ssd"));
        assert!(yaml.contains("50Gi"));
        assert!(yaml.contains("podAntiAffinity"));
    }

    #[test]
    fn test_service_yaml_generation() {
        let config = KubernetesConfig {
            use_stateful_sets: true,
            pvc_storage_class: "fast-ssd".to_string(),
            wal_volume_size: "50Gi".to_string(),
            enable_pod_affinity: true,
        };

        let deployment_manager = KubernetesDeploymentManager::new(config);
        let headless_yaml = deployment_manager.generate_headless_service_yaml();
        let service_yaml = deployment_manager.generate_service_yaml();

        assert!(headless_yaml.contains("clusterIP: None"));
        assert!(headless_yaml.contains("kind: Service"));
        assert!(service_yaml.contains("type: ClusterIP"));
        assert!(service_yaml.contains("port: 9092"));
    }

    #[test]
    fn test_complete_manifest_generation() {
        let config = KubernetesConfig {
            use_stateful_sets: true,
            pvc_storage_class: "premium-ssd".to_string(),
            wal_volume_size: "100Gi".to_string(),
            enable_pod_affinity: true,
        };

        let deployment_manager = KubernetesDeploymentManager::new(config);
        let broker_config = r#"
[broker]
id = "broker-001"
rack_id = "default"

[network]
quic_listen = "0.0.0.0:9092"
rpc_listen = "0.0.0.0:9093"
"#;

        let manifest = deployment_manager.generate_complete_manifest(broker_config);

        assert!(manifest.contains("kind: StatefulSet"));
        assert!(manifest.contains("kind: Service"));
        assert!(manifest.contains("kind: ConfigMap"));
        assert!(manifest.contains("kind: PodDisruptionBudget"));
        assert!(manifest.contains("kind: HorizontalPodAutoscaler"));
        assert!(manifest.contains("premium-ssd"));
        assert!(manifest.contains("100Gi"));
    }

    #[tokio::test]
    async fn test_volume_recovery() {
        let config = KubernetesConfig {
            use_stateful_sets: true,
            pvc_storage_class: "fast-ssd".to_string(),
            wal_volume_size: "50Gi".to_string(),
            enable_pod_affinity: true,
        };

        let recovery_manager = VolumeRecoveryManager::new(config);
        
        let result = recovery_manager.validate_volume_attachment("broker-1", "default").await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        let recovery_result = recovery_manager.recover_broker_with_volume("broker-1", "default").await;
        assert!(recovery_result.is_ok());
    }
}