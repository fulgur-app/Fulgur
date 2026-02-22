<template>
  <div class="user-dashboard" :class="{ dark: isDarkMode }">
    <header>
      <h1>{{ pageTitle }}</h1>
      <button @click="toggleDarkMode">Toggle Dark Mode</button>
    </header>

    <section v-if="isLoggedIn" class="profile">
      <img :src="user.avatar" :alt="user.name" />
      <p>Welcome, <strong>{{ user.name }}</strong>!</p>
      <span v-show="user.isPremium" class="badge">Premium</span>
    </section>
    <section v-else>
      <p>Please log in to continue.</p>
      <LoginForm @submit="handleLogin" />
    </section>

    <ul class="task-list">
      <li
        v-for="task in filteredTasks"
        :key="task.id"
        :class="{ done: task.completed }"
        @click="toggleTask(task.id)"
      >
        <input type="checkbox" v-model="task.completed" />
        {{ task.label }}
      </li>
    </ul>

    <footer>
      <p>Total tasks: {{ taskCount }} &mdash; Completed: {{ completedCount }}</p>
    </footer>
  </div>
</template>

<script>
import { ref, computed, onMounted, watch } from 'vue'
import LoginForm from './LoginForm.vue'
import { fetchUser, fetchTasks } from '@/api/user'

export default {
  name: 'UserDashboard',

  components: { LoginForm },

  props: {
    initialTheme: {
      type: String,
      default: 'light',
      validator: (value) => ['light', 'dark'].includes(value),
    },
  },

  emits: ['theme-changed'],

  setup(props, { emit }) {
    const isDarkMode = ref(props.initialTheme === 'dark')
    const isLoggedIn = ref(false)
    const user = ref({ name: '', avatar: '', isPremium: false })
    const tasks = ref([])
    const filterQuery = ref('')

    const pageTitle = computed(() =>
      isLoggedIn.value ? `${user.value.name}'s Dashboard` : 'Welcome'
    )

    const filteredTasks = computed(() =>
      tasks.value.filter((t) =>
        t.label.toLowerCase().includes(filterQuery.value.toLowerCase())
      )
    )

    const taskCount = computed(() => tasks.value.length)
    const completedCount = computed(() => tasks.value.filter((t) => t.completed).length)

    watch(isDarkMode, (newVal) => {
      emit('theme-changed', newVal ? 'dark' : 'light')
    })

    onMounted(async () => {
      try {
        user.value = await fetchUser()
        tasks.value = await fetchTasks()
        isLoggedIn.value = true
      } catch (error) {
        console.error('Failed to load user data:', error)
      }
    })

    function toggleDarkMode() {
      isDarkMode.value = !isDarkMode.value
    }

    function toggleTask(id) {
      const task = tasks.value.find((t) => t.id === id)
      if (task) task.completed = !task.completed
    }

    function handleLogin(credentials) {
      console.log('Logging in with', credentials)
      isLoggedIn.value = true
    }

    return {
      isDarkMode,
      isLoggedIn,
      user,
      tasks,
      filterQuery,
      pageTitle,
      filteredTasks,
      taskCount,
      completedCount,
      toggleDarkMode,
      toggleTask,
      handleLogin,
    }
  },
}
</script>

<style scoped>
.user-dashboard {
  font-family: sans-serif;
  padding: 1rem;
  background-color: #fff;
  color: #222;
  transition: background-color 0.3s, color 0.3s;
}

.user-dashboard.dark {
  background-color: #1e1e2e;
  color: #cdd6f4;
}

.badge {
  background: gold;
  color: #333;
  padding: 0.2rem 0.5rem;
  border-radius: 4px;
  font-size: 0.75rem;
}

.task-list li.done {
  text-decoration: line-through;
  opacity: 0.5;
}
</style>
