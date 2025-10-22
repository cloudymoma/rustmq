<template>
  <div class="dashboard">
    <h1>Cluster Dashboard</h1>

    <div v-if="loading" class="loading">Loading cluster status...</div>
    <div v-else-if="error" class="error-message">{{ error }}</div>

    <div v-else>
      <!-- Cluster Health Overview -->
      <div class="card">
        <h2>Cluster Health</h2>
        <div class="grid grid-cols-4">
          <div class="metric">
            <span class="metric-label">Status</span>
            <div class="metric-value">
              <span :class="['status-indicator', clusterStatus?.healthy ? 'status-online' : 'status-offline']"></span>
              {{ clusterStatus?.healthy ? 'Healthy' : 'Unhealthy' }}
            </div>
          </div>
          <div class="metric">
            <span class="metric-label">Leader</span>
            <div class="metric-value">{{ clusterStatus?.leader || 'None' }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Raft Term</span>
            <div class="metric-value">{{ clusterStatus?.term || 0 }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Uptime</span>
            <div class="metric-value">{{ formatUptime(health?.uptime_seconds || 0) }}</div>
          </div>
        </div>
      </div>

      <!-- Broker Statistics -->
      <div class="card">
        <h2>Broker Statistics</h2>
        <div class="grid grid-cols-3">
          <div class="metric">
            <span class="metric-label">Total Brokers</span>
            <div class="metric-value">{{ clusterStatus?.brokers.length || 0 }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Online Brokers</span>
            <div class="metric-value" style="color: #4ade80">
              {{ onlineBrokers }}
            </div>
          </div>
          <div class="metric">
            <span class="metric-label">Offline Brokers</span>
            <div class="metric-value" style="color: #ef4444">
              {{ offlineBrokers }}
            </div>
          </div>
        </div>
      </div>

      <!-- Topic Statistics -->
      <div class="card">
        <h2>Topic Statistics</h2>
        <div class="grid grid-cols-3">
          <div class="metric">
            <span class="metric-label">Total Topics</span>
            <div class="metric-value">{{ clusterStatus?.topics.length || 0 }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Total Partitions</span>
            <div class="metric-value">{{ totalPartitions }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Avg Replication Factor</span>
            <div class="metric-value">{{ averageReplication.toFixed(1) }}</div>
          </div>
        </div>
      </div>

      <!-- Recent Topics -->
      <div class="card">
        <h2>Recent Topics</h2>
        <div v-if="clusterStatus?.topics.length === 0" style="color: #a1a1aa">
          No topics created yet
        </div>
        <table v-else>
          <thead>
            <tr>
              <th>Topic Name</th>
              <th>Partitions</th>
              <th>Replication Factor</th>
              <th>Created At</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="topic in recentTopics" :key="topic.name">
              <td>{{ topic.name }}</td>
              <td>{{ topic.partitions }}</td>
              <td>{{ topic.replication_factor }}</td>
              <td>{{ formatDate(topic.created_at) }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Broker List -->
      <div class="card">
        <h2>Brokers</h2>
        <table>
          <thead>
            <tr>
              <th>Status</th>
              <th>Broker ID</th>
              <th>Host</th>
              <th>QUIC Port</th>
              <th>RPC Port</th>
              <th>Rack ID</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="broker in clusterStatus?.brokers" :key="broker.id">
              <td>
                <span :class="['status-indicator', broker.online ? 'status-online' : 'status-offline']"></span>
                {{ broker.online ? 'Online' : 'Offline' }}
              </td>
              <td>{{ broker.id }}</td>
              <td>{{ broker.host }}</td>
              <td>{{ broker.port_quic }}</td>
              <td>{{ broker.port_rpc }}</td>
              <td>{{ broker.rack_id }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import api from '../api/client'
import type { ClusterStatus, HealthResponse } from '../api/types'

const loading = ref(true)
const error = ref<string | null>(null)
const clusterStatus = ref<ClusterStatus | null>(null)
const health = ref<HealthResponse | null>(null)
let refreshInterval: number | null = null

const onlineBrokers = computed(() =>
  clusterStatus.value?.brokers.filter(b => b.online).length || 0
)

const offlineBrokers = computed(() =>
  clusterStatus.value?.brokers.filter(b => !b.online).length || 0
)

const totalPartitions = computed(() =>
  clusterStatus.value?.topics.reduce((sum, topic) => sum + topic.partitions, 0) || 0
)

const averageReplication = computed(() => {
  const topics = clusterStatus.value?.topics || []
  if (topics.length === 0) return 0
  const sum = topics.reduce((acc, topic) => acc + topic.replication_factor, 0)
  return sum / topics.length
})

const recentTopics = computed(() =>
  (clusterStatus.value?.topics || []).slice(0, 5)
)

const formatUptime = (seconds: number): string => {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)

  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${minutes}m`
  return `${minutes}m`
}

const formatDate = (dateStr: string): string => {
  const date = new Date(dateStr)
  return date.toLocaleString()
}

const fetchData = async () => {
  try {
    error.value = null
    const [clusterData, healthData] = await Promise.all([
      api.getClusterStatus(),
      api.getHealth()
    ])
    clusterStatus.value = clusterData
    health.value = healthData
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Unknown error'
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await fetchData()
  // Refresh every 5 seconds
  refreshInterval = setInterval(fetchData, 5000) as unknown as number
})

onBeforeUnmount(() => {
  if (refreshInterval !== null) {
    clearInterval(refreshInterval)
  }
})
</script>

<style scoped>
.dashboard {
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
</style>
