<template>
  <div class="acl">
    <h1>Access Control List (ACL)</h1>

    <div class="card">
      <h2>ACL Management</h2>
      <div style="padding: 3rem 2rem; text-align: center; color: #a1a1aa;">
        <div style="font-size: 3rem; margin-bottom: 1rem;">🔒</div>
        <h3 style="color: #e5e5e5; margin-bottom: 0.5rem;">ACL Management Coming Soon</h3>
        <p style="max-width: 600px; margin: 0 auto; line-height: 1.6;">
          ACL (Access Control List) management allows you to control which principals (users/services)
          can perform operations on topics and resources.
        </p>
        <p style="max-width: 600px; margin: 1rem auto 0; line-height: 1.6;">
          This feature is currently under development. It will include:
        </p>
        <ul style="list-style: none; padding: 0; margin: 1.5rem auto; max-width: 400px; text-align: left;">
          <li style="padding: 0.5rem 0;">✓ Principal-based access control</li>
          <li style="padding: 0.5rem 0;">✓ Resource-level permissions</li>
          <li style="padding: 0.5rem 0;">✓ Role-based access control (RBAC)</li>
          <li style="padding: 0.5rem 0;">✓ Audit logging</li>
        </ul>
      </div>
    </div>

    <div class="card">
      <h2>Current Security Status</h2>
      <div class="grid grid-cols-3">
        <div class="metric">
          <span class="metric-label">Authentication</span>
          <div class="metric-value" style="font-size: 1rem;">
            <span class="status-indicator status-warning"></span>
            Not configured
          </div>
        </div>
        <div class="metric">
          <span class="metric-label">ACL Rules</span>
          <div class="metric-value">0</div>
        </div>
        <div class="metric">
          <span class="metric-label">Principals</span>
          <div class="metric-value">0</div>
        </div>
      </div>

      <div style="margin-top: 1.5rem; padding: 1rem; background-color: #2a2a2a; border-radius: 4px; border-left: 3px solid #fbbf24;">
        <p style="color: #fbbf24; font-weight: 600; margin: 0 0 0.5rem 0;">⚠️ Security Notice</p>
        <p style="color: #a1a1aa; font-size: 0.875rem; margin: 0;">
          No authentication is currently enabled for this WebUI. For production deployments,
          it is strongly recommended to enable mTLS authentication and configure ACL rules
          to protect your cluster.
        </p>
      </div>
    </div>

    <div class="card">
      <h2>Security Best Practices</h2>
      <div style="color: #a1a1aa; line-height: 1.8;">
        <h3 style="color: #e5e5e5; font-size: 1rem; margin-bottom: 0.75rem;">Recommendations:</h3>
        <ul style="margin-left: 1.5rem;">
          <li>Enable mTLS for all client connections</li>
          <li>Create dedicated service accounts with minimal permissions</li>
          <li>Regularly rotate credentials and certificates</li>
          <li>Monitor and audit access logs</li>
          <li>Use separate ACL rules for production and development environments</li>
        </ul>

        <h3 style="color: #e5e5e5; font-size: 1rem; margin: 1.5rem 0 0.75rem;">Documentation:</h3>
        <p>
          For detailed security configuration, refer to the RustMQ security documentation:
          <code style="background-color: #1a1a1a; padding: 0.25rem 0.5rem; border-radius: 3px; margin-left: 0.5rem;">
            config/README.md
          </code>
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// Placeholder component for future ACL management
</script>

<style scoped>
.acl {
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
