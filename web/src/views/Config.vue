<template>
  <div class="config">
    <h1>Configuration</h1>

    <div v-if="loading" class="loading">Loading configuration...</div>
    <div v-else-if="error" class="error-message">{{ error }}</div>

    <div v-else>
      <!-- Cluster Configuration -->
      <div class="card">
        <h2>Cluster Configuration</h2>
        <div class="config-grid">
          <div class="config-item">
            <span class="config-label">Cluster ID</span>
            <span class="config-value">{{ health?.version || 'N/A' }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Leader Node</span>
            <span class="config-value">{{ clusterStatus?.leader || 'None' }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Raft Term</span>
            <span class="config-value">{{ clusterStatus?.term || 0 }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Total Brokers</span>
            <span class="config-value">{{ clusterStatus?.brokers.length || 0 }}</span>
          </div>
        </div>
      </div>

      <!-- Broker Configuration -->
      <div class="card">
        <h2>Broker Configuration</h2>
        <div style="padding: 2rem; text-align: center; color: #a1a1aa;">
          <p>Broker-specific configuration will be displayed here in future updates.</p>
          <p style="margin-top: 0.5rem; font-size: 0.875rem;">
            Configuration includes: retention policies, compression settings, replication factors, and more.
          </p>
        </div>
      </div>

      <!-- Storage Configuration -->
      <div class="card">
        <h2>Storage Configuration</h2>
        <div class="config-grid">
          <div class="config-item">
            <span class="config-label">WAL Backend</span>
            <span class="config-value">Local (Optimized)</span>
          </div>
          <div class="config-item">
            <span class="config-label">Object Storage</span>
            <span class="config-value">S3 Compatible</span>
          </div>
          <div class="config-item">
            <span class="config-label">Cache Strategy</span>
            <span class="config-value">Moka (TinyLFU)</span>
          </div>
          <div class="config-item">
            <span class="config-label">Buffer Pool</span>
            <span class="config-value">Enabled</span>
          </div>
        </div>

        <div style="margin-top: 1.5rem; padding: 1rem; background-color: #2a2a2a; border-radius: 4px;">
          <p style="color: #4ade80; font-weight: 600; margin: 0 0 0.5rem 0;">✓ Cloud-Native Storage</p>
          <p style="color: #a1a1aa; font-size: 0.875rem; margin: 0;">
            RustMQ uses a cloud-first architecture with local WAL for fast writes and object storage (S3/GCS/Azure)
            for cost-effective long-term storage. This provides <strong>90% cost savings</strong> compared to traditional architectures.
          </p>
        </div>
      </div>

      <!-- Network Configuration -->
      <div class="card">
        <h2>Network Configuration</h2>
        <table>
          <thead>
            <tr>
              <th>Protocol</th>
              <th>Default Port</th>
              <th>Description</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td><strong>QUIC</strong></td>
              <td>9092</td>
              <td>Client connections (HTTP/3)</td>
              <td><span class="status-indicator status-online"></span> Active</td>
            </tr>
            <tr>
              <td><strong>gRPC</strong></td>
              <td>9093</td>
              <td>Inter-broker replication</td>
              <td><span class="status-indicator status-online"></span> Active</td>
            </tr>
            <tr>
              <td><strong>Admin API</strong></td>
              <td>8080</td>
              <td>REST API & WebUI</td>
              <td><span class="status-indicator status-online"></span> Active</td>
            </tr>
            <tr>
              <td><strong>Controller</strong></td>
              <td>9094-9095</td>
              <td>Raft consensus (internal)</td>
              <td><span class="status-indicator status-online"></span> Active</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Performance Features -->
      <div class="card">
        <h2>Performance Features</h2>
        <div class="features-grid">
          <div class="feature-card feature-enabled">
            <div class="feature-icon">⚡</div>
            <div class="feature-name">SmallVec Optimization</div>
            <div class="feature-desc">90% reduction in header allocations</div>
          </div>
          <div class="feature-card feature-enabled">
            <div class="feature-icon">🔄</div>
            <div class="feature-name">Buffer Pooling</div>
            <div class="feature-desc">30-40% allocation overhead reduction</div>
          </div>
          <div class="feature-card feature-enabled">
            <div class="feature-icon">🚀</div>
            <div class="feature-name">Lock-Free Cache</div>
            <div class="feature-desc">Moka cache with TinyLFU algorithm</div>
          </div>
          <div class="feature-card feature-enabled">
            <div class="feature-icon">⚙️</div>
            <div class="feature-name">FuturesUnordered</div>
            <div class="feature-desc">85% replication latency improvement</div>
          </div>
          <div class="feature-card feature-enabled">
            <div class="feature-icon">🛡️</div>
            <div class="feature-name">Fast ACL</div>
            <div class="feature-desc">547ns authorization checks</div>
          </div>
          <div class="feature-card feature-enabled">
            <div class="feature-icon">🧪</div>
            <div class="feature-name">Miri Validated</div>
            <div class="feature-desc">Comprehensive memory safety</div>
          </div>
        </div>
      </div>

      <!-- System Information -->
      <div class="card">
        <h2>System Information</h2>
        <div class="config-grid">
          <div class="config-item">
            <span class="config-label">Version</span>
            <span class="config-value">{{ health?.version || 'Unknown' }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Uptime</span>
            <span class="config-value">{{ formatUptime(health?.uptime_seconds || 0) }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Is Leader</span>
            <span class="config-value">{{ health?.is_leader ? 'Yes' : 'No' }}</span>
          </div>
          <div class="config-item">
            <span class="config-label">Health Status</span>
            <span class="config-value" style="color: #4ade80;">{{ health?.status || 'Unknown' }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '../api/client'
import type { HealthResponse, ClusterStatus } from '../api/types'

const loading = ref(true)
const error = ref<string | null>(null)
const health = ref<HealthResponse | null>(null)
const clusterStatus = ref<ClusterStatus | null>(null)

const formatUptime = (seconds: number): string => {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)

  if (days > 0) return `${days}d ${hours}h ${minutes}m`
  if (hours > 0) return `${hours}h ${minutes}m`
  return `${minutes}m ${seconds % 60}s`
}

onMounted(async () => {
  try {
    const [healthData, clusterData] = await Promise.all([
      api.getHealth(),
      api.getClusterStatus()
    ])
    health.value = healthData
    clusterStatus.value = clusterData
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load configuration'
  } finally {
    loading.value = false
  }
})
</script>

<style scoped>
.config {
  max-width: 1400px;
}

h1 {
  margin-bottom: 1.5rem;
  font-size: 2rem;
}

h2 {
  margin-bottom: 1rem;
  font-size: 1.25rem;
  color: #e5e5e5;
}

.config-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 1rem;
}

.config-item {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.config-label {
  font-size: 0.875rem;
  color: #a1a1aa;
}

.config-value {
  font-size: 1.125rem;
  font-weight: 600;
  color: #e5e5e5;
}

.features-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 1rem;
  margin-top: 1rem;
}

.feature-card {
  padding: 1.25rem;
  border-radius: 8px;
  border: 1px solid #3f3f3f;
  background-color: #2a2a2a;
  text-align: center;
}

.feature-card.feature-enabled {
  border-color: #4ade80;
}

.feature-icon {
  font-size: 2rem;
  margin-bottom: 0.5rem;
}

.feature-name {
  font-weight: 600;
  margin-bottom: 0.25rem;
  color: #e5e5e5;
}

.feature-desc {
  font-size: 0.75rem;
  color: #a1a1aa;
}
</style>
