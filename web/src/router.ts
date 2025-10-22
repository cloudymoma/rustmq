import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from './views/Dashboard.vue'
import Topics from './views/Topics.vue'
import Brokers from './views/Brokers.vue'
import ACL from './views/ACL.vue'
import Config from './views/Config.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: 'dashboard',
      component: Dashboard,
    },
    {
      path: '/topics',
      name: 'topics',
      component: Topics,
    },
    {
      path: '/brokers',
      name: 'brokers',
      component: Brokers,
    },
    {
      path: '/acl',
      name: 'acl',
      component: ACL,
    },
    {
      path: '/config',
      name: 'config',
      component: Config,
    },
  ],
})

export default router
