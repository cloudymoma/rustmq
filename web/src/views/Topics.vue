<template>
  <div class="topics">
    <div class="header">
      <h1>Topics</h1>
      <button class="btn-primary" @click="showCreateModal = true">+ Create Topic</button>
    </div>

    <div v-if="loading" class="loading">Loading topics...</div>
    <div v-else-if="error" class="error-message">{{ error }}</div>
    <div v-else-if="successMessage" class="success-message">{{ successMessage }}</div>

    <div v-if="!loading && !error" class="card">
      <div v-if="topics.length === 0" style="color: #a1a1aa; text-align: center; padding: 2rem;">
        No topics created yet. Click "Create Topic" to get started.
      </div>
      <table v-else>
        <thead>
          <tr>
            <th>Topic Name</th>
            <th>Partitions</th>
            <th>Replication Factor</th>
            <th>Created At</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="topic in topics" :key="topic.name">
            <td><strong>{{ topic.name }}</strong></td>
            <td>{{ topic.partitions }}</td>
            <td>{{ topic.replication_factor }}</td>
            <td>{{ formatDate(topic.created_at) }}</td>
            <td>
              <button class="btn-secondary" @click="viewDetails(topic.name)" style="margin-right: 0.5rem;">
                View
              </button>
              <button class="btn-danger" @click="deleteTopic(topic.name)">
                Delete
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Create Topic Modal -->
    <div v-if="showCreateModal" class="modal-overlay" @click.self="showCreateModal = false">
      <div class="modal">
        <h2>Create New Topic</h2>
        <form @submit.prevent="createTopic">
          <div class="form-group">
            <label>Topic Name</label>
            <input v-model="newTopic.name" required pattern="[a-zA-Z0-9_-]+" placeholder="e.g., events" />
          </div>
          <div class="form-group">
            <label>Partitions</label>
            <input v-model.number="newTopic.partitions" type="number" min="1" required />
          </div>
          <div class="form-group">
            <label>Replication Factor</label>
            <input v-model.number="newTopic.replication_factor" type="number" min="1" required />
          </div>
          <div class="form-group">
            <label>Retention (hours, optional)</label>
            <input v-model.number="newTopic.retention_hours" type="number" min="1" placeholder="e.g., 168" />
          </div>
          <div class="form-group">
            <label>Compression Type (optional)</label>
            <select v-model="newTopic.compression_type">
              <option value="">None</option>
              <option value="gzip">GZIP</option>
              <option value="lz4">LZ4</option>
              <option value="snappy">Snappy</option>
              <option value="zstd">ZSTD</option>
            </select>
          </div>
          <div style="display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 1.5rem;">
            <button type="button" class="btn-secondary" @click="showCreateModal = false">Cancel</button>
            <button type="submit" class="btn-primary" :disabled="creating">
              {{ creating ? 'Creating...' : 'Create' }}
            </button>
          </div>
        </form>
      </div>
    </div>

    <!-- Topic Details Modal -->
    <div v-if="selectedTopic" class="modal-overlay" @click.self="selectedTopic = null">
      <div class="modal" style="max-width: 700px;">
        <h2>Topic: {{ selectedTopic.name }}</h2>
        <div class="topic-details">
          <div class="metric">
            <span class="metric-label">Partitions</span>
            <div class="metric-value">{{ selectedTopic.partitions }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Replication Factor</span>
            <div class="metric-value">{{ selectedTopic.replication_factor }}</div>
          </div>
          <div class="metric">
            <span class="metric-label">Created At</span>
            <div class="metric-value" style="font-size: 1rem;">{{ formatDate(selectedTopic.created_at) }}</div>
          </div>
        </div>

        <h3 style="margin-top: 1.5rem; margin-bottom: 1rem;">Partition Assignments</h3>
        <table>
          <thead>
            <tr>
              <th>Partition</th>
              <th>Leader</th>
              <th>Replicas</th>
              <th>In-Sync</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="partition in selectedTopic.partition_assignments" :key="partition.partition">
              <td>{{ partition.partition }}</td>
              <td>{{ partition.leader }}</td>
              <td>{{ partition.replicas.join(', ') }}</td>
              <td>{{ partition.in_sync_replicas.join(', ') }}</td>
            </tr>
          </tbody>
        </table>

        <div style="margin-top: 1.5rem; text-align: right;">
          <button class="btn-secondary" @click="selectedTopic = null">Close</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import api from '../api/client'
import type { TopicSummary, TopicDetail } from '../api/types'

const loading = ref(true)
const error = ref<string | null>(null)
const successMessage = ref<string | null>(null)
const topics = ref<TopicSummary[]>([])
const showCreateModal = ref(false)
const creating = ref(false)
const selectedTopic = ref<TopicDetail | null>(null)

const newTopic = ref({
  name: '',
  partitions: 3,
  replication_factor: 2,
  retention_hours: null as number | null,
  compression_type: ''
})

const formatDate = (dateStr: string): string => {
  const date = new Date(dateStr)
  return date.toLocaleString()
}

const fetchTopics = async () => {
  try {
    loading.value = true
    error.value = null
    topics.value = await api.listTopics()
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load topics'
  } finally {
    loading.value = false
  }
}

const createTopic = async () => {
  try {
    creating.value = true
    error.value = null
    successMessage.value = null

    const request: any = {
      name: newTopic.value.name,
      partitions: newTopic.value.partitions,
      replication_factor: newTopic.value.replication_factor,
    }

    if (newTopic.value.retention_hours) {
      request.retention_ms = newTopic.value.retention_hours * 3600000
    }

    if (newTopic.value.compression_type) {
      request.compression_type = newTopic.value.compression_type
    }

    const result = await api.createTopic(request)
    successMessage.value = result
    showCreateModal.value = false

    // Reset form
    newTopic.value = {
      name: '',
      partitions: 3,
      replication_factor: 2,
      retention_hours: null,
      compression_type: ''
    }

    // Refresh topics list
    await fetchTopics()

    // Clear success message after 3 seconds
    setTimeout(() => { successMessage.value = null }, 3000)
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to create topic'
  } finally {
    creating.value = false
  }
}

const deleteTopic = async (name: string) => {
  if (!confirm(`Are you sure you want to delete topic "${name}"?`)) {
    return
  }

  try {
    error.value = null
    successMessage.value = null
    const result = await api.deleteTopic(name)
    successMessage.value = result
    await fetchTopics()

    // Clear success message after 3 seconds
    setTimeout(() => { successMessage.value = null }, 3000)
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to delete topic'
  }
}

const viewDetails = async (name: string) => {
  try {
    selectedTopic.value = await api.getTopic(name)
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Failed to load topic details'
  }
}

onMounted(fetchTopics)
</script>

<style scoped>
.topics {
  max-width: 1400px;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 1.5rem;
}

h1 {
  font-size: 2rem;
  margin: 0;
}

.topic-details {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 1rem;
  margin-top: 1rem;
}

h3 {
  color: #e5e5e5;
}
</style>
