<template>
  <div class="app-container">
    <nav class="sidebar">
      <div class="logo">
        <h1>🦀 RustMQ</h1>
        <p class="version">v{{ version }}</p>
      </div>
      <ul class="nav-links">
        <li>
          <router-link to="/" :class="{ active: $route.path === '/' }">
            📊 Dashboard
          </router-link>
        </li>
        <li>
          <router-link to="/topics" :class="{ active: $route.path === '/topics' }">
            📝 Topics
          </router-link>
        </li>
        <li>
          <router-link to="/brokers" :class="{ active: $route.path === '/brokers' }">
            🖥️ Brokers
          </router-link>
        </li>
        <li>
          <router-link to="/acl" :class="{ active: $route.path === '/acl' }">
            🔒 ACL
          </router-link>
        </li>
        <li>
          <router-link to="/config" :class="{ active: $route.path === '/config' }">
            ⚙️ Configuration
          </router-link>
        </li>
      </ul>
    </nav>
    <main class="main-content">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const version = ref('1.0.0')
</script>

<style scoped>
.app-container {
  display: flex;
  min-height: 100vh;
}

.sidebar {
  width: 250px;
  background-color: #1a1a1a;
  border-right: 1px solid #3f3f3f;
  padding: 2rem 0;
  position: fixed;
  height: 100vh;
  overflow-y: auto;
}

.logo {
  padding: 0 1.5rem;
  margin-bottom: 2rem;
}

.logo h1 {
  font-size: 1.5rem;
  margin: 0;
  color: #646cff;
}

.version {
  font-size: 0.75rem;
  color: #a1a1aa;
  margin-top: 0.25rem;
}

.nav-links {
  list-style: none;
  padding: 0;
  margin: 0;
}

.nav-links li {
  margin: 0;
}

.nav-links a {
  display: block;
  padding: 0.75rem 1.5rem;
  color: #a1a1aa;
  text-decoration: none;
  transition: all 0.2s;
}

.nav-links a:hover {
  background-color: #2a2a2a;
  color: white;
}

.nav-links a.active {
  background-color: #646cff;
  color: white;
}

.main-content {
  flex: 1;
  margin-left: 250px;
  padding: 2rem;
  background-color: #242424;
}

@media (max-width: 768px) {
  .sidebar {
    width: 100%;
    position: relative;
    height: auto;
  }

  .main-content {
    margin-left: 0;
  }
}
</style>
