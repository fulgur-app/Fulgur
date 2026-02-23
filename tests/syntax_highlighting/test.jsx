import React, { useState, useEffect, useCallback, useMemo, createContext, useContext } from 'react';

// Context for theme management
const ThemeContext = createContext('light');

// Custom hook
function useLocalStorage(key, initialValue) {
  const [value, setValue] = useState(() => {
    const stored = localStorage.getItem(key);
    return stored !== null ? JSON.parse(stored) : initialValue;
  });

  useEffect(() => {
    localStorage.setItem(key, JSON.stringify(value));
  }, [key, value]);

  return [value, setValue];
}

// Small presentational component with destructured props
function Badge({ label, color = '#6366f1' }) {
  return <span style={{ background: color, padding: '2px 8px', borderRadius: 4 }}>{label}</span>;
}

// Task item with event handling and conditional rendering
function TaskItem({ task, onToggle, onDelete }) {
  const theme = useContext(ThemeContext);
  const isDark = theme === 'dark';

  return (
    <li className={isDark ? 'task-dark' : 'task-light'}>
      <input type="checkbox" checked={task.done} onChange={() => onToggle(task.id)} />
      <span style={{ textDecoration: task.done ? 'line-through' : 'none' }}>
        {task.title}
      </span>
      {task.priority === 'high' && <Badge label="urgent" color="#ef4444" />}
      <button onClick={() => onDelete(task.id)} aria-label={`Delete ${task.title}`}>
        &times;
      </button>
    </li>
  );
}

// Main application component
export default function App() {
  const [theme, setTheme] = useLocalStorage('theme', 'light');
  const [tasks, setTasks] = useState([]);
  const [input, setInput] = useState('');
  const [filter, setFilter] = useState('all');

  useEffect(() => {
    const initial = [
      { id: 1, title: 'Learn React hooks', done: true, priority: 'low' },
      { id: 2, title: 'Build a todo app', done: false, priority: 'high' },
      { id: 3, title: 'Write tests', done: false, priority: 'medium' },
    ];
    setTasks(initial);
  }, []);

  const addTask = useCallback(() => {
    if (!input.trim()) return;
    setTasks((prev) => [
      ...prev,
      { id: Date.now(), title: input.trim(), done: false, priority: 'medium' },
    ]);
    setInput('');
  }, [input]);

  const toggleTask = useCallback((id) => {
    setTasks((prev) => prev.map((t) => (t.id === id ? { ...t, done: !t.done } : t)));
  }, []);

  const deleteTask = useCallback((id) => {
    setTasks((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const filtered = useMemo(() => {
    switch (filter) {
      case 'done':
        return tasks.filter((t) => t.done);
      case 'pending':
        return tasks.filter((t) => !t.done);
      default:
        return tasks;
    }
  }, [tasks, filter]);

  const stats = useMemo(() => ({
    total: tasks.length,
    done: tasks.filter((t) => t.done).length,
    pending: tasks.filter((t) => !t.done).length,
  }), [tasks]);

  return (
    <ThemeContext.Provider value={theme}>
      <div className={`app ${theme}`}>
        <h1>Task Manager</h1>
        <button onClick={() => setTheme(theme === 'light' ? 'dark' : 'light')}>
          {theme === 'light' ? 'Dark mode' : 'Light mode'}
        </button>

        <form onSubmit={(e) => { e.preventDefault(); addTask(); }}>
          <input value={input} onChange={(e) => setInput(e.target.value)} placeholder="New task..." />
          <button type="submit">Add</button>
        </form>

        <nav>
          {['all', 'pending', 'done'].map((f) => (
            <button key={f} className={filter === f ? 'active' : ''} onClick={() => setFilter(f)}>
              {f.charAt(0).toUpperCase() + f.slice(1)}
            </button>
          ))}
        </nav>

        <ul>
          {filtered.length === 0
            ? <li>No tasks to show.</li>
            : filtered.map((task) => (
                <TaskItem key={task.id} task={task} onToggle={toggleTask} onDelete={deleteTask} />
              ))
          }
        </ul>

        <footer>
          <p>{`${stats.done} of ${stats.total} completed — ${stats.pending} remaining`}</p>
        </footer>
      </div>
    </ThemeContext.Provider>
  );
}
