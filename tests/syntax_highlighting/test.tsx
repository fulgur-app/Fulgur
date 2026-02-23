// @ts-nocheck
import React, { useState, useEffect, useReducer, useRef, createContext, useContext } from 'react';

// Type definitions
interface Task {
  id: number;
  title: string;
  done: boolean;
  priority: 'low' | 'medium' | 'high';
  createdAt: Date;
}

type FilterMode = 'all' | 'pending' | 'done';

type TaskAction =
  | { type: 'add'; payload: Omit<Task, 'id' | 'createdAt'> }
  | { type: 'toggle'; payload: number }
  | { type: 'delete'; payload: number }
  | { type: 'load'; payload: Task[] };

// Generic utility type
type Optional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

// Context with typed value
interface ThemeContextValue {
  theme: 'light' | 'dark';
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue>({
  theme: 'light',
  toggle: () => {},
});

// Custom generic hook
function useDebounced<T>(value: T, delay: number = 300): T {
  const [debounced, setDebounced] = useState<T>(value);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);

  return debounced;
}

// Reducer for complex state management
function taskReducer(state: Task[], action: TaskAction): Task[] {
  switch (action.type) {
    case 'add':
      return [...state, { ...action.payload, id: Date.now(), createdAt: new Date() }];
    case 'toggle':
      return state.map((t) => (t.id === action.payload ? { ...t, done: !t.done } : t));
    case 'delete':
      return state.filter((t) => t.id !== action.payload);
    case 'load':
      return action.payload;
  }
}

// Component with generic props
interface ListProps<T> {
  items: T[];
  renderItem: (item: T, index: number) => React.ReactNode;
  emptyMessage?: string;
}

function List<T extends { id: number }>({ items, renderItem, emptyMessage = 'Nothing here.' }: ListProps<T>) {
  if (items.length === 0) {
    return <p className="empty">{emptyMessage}</p>;
  }
  return <ul>{items.map((item, i) => <li key={item.id}>{renderItem(item, i)}</li>)}</ul>;
}

// Component with discriminated union props
type NotificationProps =
  | { variant: 'success'; message: string }
  | { variant: 'error'; message: string; retry: () => void };

function Notification(props: NotificationProps) {
  return (
    <div className={`notification ${props.variant}`} role="alert">
      <span>{props.message}</span>
      {props.variant === 'error' && (
        <button onClick={props.retry}>Retry</button>
      )}
    </div>
  );
}

// Main application
const App: React.FC = () => {
  const [tasks, dispatch] = useReducer(taskReducer, []);
  const [filter, setFilter] = useState<FilterMode>('all');
  const [search, setSearch] = useState('');
  const [theme, setTheme] = useState<'light' | 'dark'>('light');
  const inputRef = useRef<HTMLInputElement>(null);
  const debouncedSearch = useDebounced(search, 250);

  useEffect(() => {
    const initial: Task[] = [
      { id: 1, title: 'Set up TypeScript', done: true, priority: 'high', createdAt: new Date() },
      { id: 2, title: 'Add type safety', done: false, priority: 'medium', createdAt: new Date() },
      { id: 3, title: 'Write generic components', done: false, priority: 'low', createdAt: new Date() },
    ];
    dispatch({ type: 'load', payload: initial });
  }, []);

  const filtered = tasks
    .filter((t) => (filter === 'done' ? t.done : filter === 'pending' ? !t.done : true))
    .filter((t) => t.title.toLowerCase().includes(debouncedSearch.toLowerCase()));

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const title = inputRef.current?.value.trim();
    if (!title) return;
    dispatch({ type: 'add', payload: { title, done: false, priority: 'medium' } });
    inputRef.current!.value = '';
  };

  const stats = {
    total: tasks.length,
    done: tasks.filter((t) => t.done).length,
  } as const;

  return (
    <ThemeContext.Provider value={{ theme, toggle: () => setTheme((t) => (t === 'light' ? 'dark' : 'light')) }}>
      <div className={`app ${theme}`}>
        <h1>Task Manager</h1>

        <form onSubmit={handleSubmit}>
          <input ref={inputRef} placeholder="New task..." />
          <button type="submit">Add</button>
        </form>

        <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search..." />

        <nav>
          {(['all', 'pending', 'done'] as const).map((f) => (
            <button key={f} className={filter === f ? 'active' : ''} onClick={() => setFilter(f)}>
              {f.charAt(0).toUpperCase() + f.slice(1)}
            </button>
          ))}
        </nav>

        <List<Task>
          items={filtered}
          emptyMessage="No matching tasks."
          renderItem={(task) => (
            <>
              <input type="checkbox" checked={task.done} onChange={() => dispatch({ type: 'toggle', payload: task.id })} />
              <span style={{ textDecoration: task.done ? 'line-through' : 'none' }}>{task.title}</span>
              <small>({task.priority})</small>
              <button onClick={() => dispatch({ type: 'delete', payload: task.id })}>&times;</button>
            </>
          )}
        />

        {stats.done === stats.total && stats.total > 0 && (
          <Notification variant="success" message="All tasks completed!" />
        )}

        <footer>
          <p>{`${stats.done} of ${stats.total} completed`}</p>
        </footer>
      </div>
    </ThemeContext.Provider>
  );
};

export default App;
