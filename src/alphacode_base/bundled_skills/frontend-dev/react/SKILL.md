---
name: react
description: React 19+ development methodology: hooks, Server Components, concurrent features, state management, custom hooks patterns, error boundaries, and performance optimization.
---

# React 19+ Development Skill

## Core Principles

1. **Server Components by default** — Only add 'use client' when interactivity is required
2. **TypeScript always** — No any types, strict mode enabled
3. **Hooks over classes** — Functional components exclusively
4. **Composition over inheritance** — Small, composable components
5. **Explicit data flow** — Props down, events up

## 1. Hooks Reference

### useState

```typescript
// Basic state
const [count, setCount] = useState(0);

// Lazy initialization (expensive computations)
const [state, setState] = useState(() => computeExpensiveValue());

// Functional updates (when state depends on previous)
setCount(prev => prev + 1);

// Object state — always spread
const [user, setUser] = useState<User>({ name: '', email: '' });
setUser(prev => ({ ...prev, name: 'John' }));
```

### useEffect

```typescript
// Side effect with cleanup
useEffect(() => {
  const controller = new AbortController();
  fetchData(signal: controller.signal);
  return () => controller.abort();
}, [dependency]);

// One-time effect (mount only)
useEffect(() => {
  analytics.track('page_view');
}, []);

// Avoid: useEffect for derived state — use useMemo instead
// Avoid: useEffect for event handlers — use event handlers directly
```

### useCallback

```typescript
// Memoize functions passed to child components
const handleSubmit = useCallback((data: FormData) => {
  submitToApi(data);
}, []);

// When passing to memoized children
<MemoizedChild onSubmit={handleSubmit} />

// Do NOT memoize: inline functions in JSX, functions not passed as props
```

### useMemo

```typescript
// Expensive computations
const sortedItems = useMemo(() => {
  return items.sort((a, b) => a.name.localeCompare(b.name));
}, [items]);

// Reference equality for objects/arrays
const config = useMemo(() => ({
  url: API_URL,
  headers: { Authorization: token }
}), [token]);

// Do NOT memoize: simple calculations, primitive values
```

### useRef

```typescript
// DOM references
const inputRef = useRef<HTMLInputElement>(null);
inputRef.current?.focus();

// Mutable values that don't cause re-renders
const intervalRef = useRef<NodeJS.Timeout | null>(null);
useEffect(() => {
  intervalRef.current = setInterval(tick, 1000);
  return () => clearInterval(intervalRef.current!);
}, []);

// Previous value pattern
const prevCount = useRef(count);
useEffect(() => {
  prevCount.current = count;
}, [count]);
```

### useReducer

```typescript
// Complex state logic
type Action =
  | { type: 'increment' }
  | { type: 'decrement' }
  | { type: 'reset'; value: number };

function reducer(state: number, action: Action): number {
  switch (action.type) {
    case 'increment': return state + 1;
    case 'decrement': return state - 1;
    case 'reset': return action.value;
  }
}

const [count, dispatch] = useReducer(reducer, 0);

// When to use: complex state transitions, multiple related values
// When NOT to use: simple boolean toggles, single values
```

### useSyncExternalStore

```typescript
// Subscribe to external stores (Redux, Zustand, custom)
import { useSyncExternalStore } from 'react';

function useTheme() {
  return useSyncExternalStore(
    (callback) => {
      window.matchMedia('(prefers-color-scheme: dark)')
        .addEventListener('change', callback);
      return () => {
        window.matchMedia('(prefers-color-scheme: dark)')
          .removeEventListener('change', callback);
      };
    },
    () => window.matchMedia('(prefers-color-scheme: dark)').matches,
    () => false // server snapshot
  );
}
```

## 2. Server Components

### When to Use Server Components

- Data fetching (API calls, database queries)
- Accessing backend resources directly
- Keeping sensitive data on server (API keys, tokens)
- Large dependencies that don't need client-side

### When to Use Client Components

- Event handlers (onClick, onChange)
- Browser APIs (localStorage, window)
- State (useState, useReducer)
- Effects (useEffect)
- Custom hooks that use any of the above

### Server Component Pattern

```typescript
// app/dashboard/page.tsx — Server Component
import { getUser } from '@/lib/auth';
import { getProjects } from '@/lib/api';
import { DashboardClient } from './dashboard-client';

export default async function DashboardPage() {
  const user = await getUser();
  const projects = await getProjects(user.id);

  // Pass server data to client component
  return <DashboardClient user={user} projects={projects} />;
}

// app/dashboard/dashboard-client.tsx — Client Component
'use client';
import { useState } from 'react';

export function DashboardClient({ user, projects }) {
  const [filter, setFilter] = useState('all');
  // Interactive UI here
}
```

### Server Actions

```typescript
// app/actions.ts
'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';

export async function createPost(formData: FormData) {
  const title = formData.get('title') as string;
  const content = formData.get('content') as string;

  // Validate
  if (!title || !content) {
    return { error: 'Title and content required' };
  }

  await db.posts.create({ data: { title, content } });
  revalidatePath('/posts');
  redirect('/posts');
}
```

## 3. Concurrent Features

### Suspense

```typescript
// Server Component with Suspense
import { Suspense } from 'react';
import { PostList } from './post-list';
import { PostSkeleton } from './post-skeleton';

export default function PostsPage() {
  return (
    <div>
      <h1>Posts</h1>
      <Suspense fallback={<PostSkeleton />}>
        <PostList />
      </Suspense>
    </div>
  );
}
```

### useTransition

```typescript
// Non-blocking state updates
const [isPending, startTransition] = useTransition();

function handleFilter(value: string) {
  startTransition(() => {
    setFilter(value); // Low-priority update
  });
}

// Show loading state during transition
return <div className={isPending ? 'opacity-50' : ''}>
  <FilterInput onChange={handleFilter} />
  <Results filter={filter} />
</div>;
```

### useDeferredValue

```typescript
// Defer expensive re-renders
const [query, setQuery] = useState('');
const deferredQuery = useDeferredValue(query);

// List re-renders with deferred value (lower priority)
return <SearchResults query={deferredQuery} />;
// Input stays responsive while results update
```

## 4. State Management

### Zustand (Client State)

```typescript
import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';

interface CounterStore {
  count: number;
  increment: () => void;
  decrement: () => void;
  reset: () => void;
}

export const useCounterStore = create<CounterStore>()(
  devtools(
    persist(
      (set) => ({
        count: 0,
        increment: () => set((state) => ({ count: state.count + 1 })),
        decrement: () => set((state) => ({ count: state.count - 1 })),
        reset: () => set({ count: 0 }),
      }),
      { name: 'counter-storage' }
    )
  )
);

// Selectors for performance
const count = useCounterStore((state) => state.count);
```

### React Query / TanStack Query (Server State)

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';

// Fetch data
function usePosts() {
  return useQuery({
    queryKey: ['posts'],
    queryFn: () => fetch('/api/posts').then(r => r.json()),
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

// Mutate data
function useCreatePost() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (newPost: CreatePostInput) =>
      fetch('/api/posts', { method: 'POST', body: JSON.stringify(newPost) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['posts'] });
    },
  });
}
```

## 5. Custom Hooks Patterns

### Data Fetching Hook

```typescript
function useFetch<T>(url: string, options?: RequestInit) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    const controller = new AbortController();

    async function fetchData() {
      try {
        const response = await fetch(url, { ...options, signal: controller.signal });
        if (!response.ok) throw new Error(response.statusText);
        const json = await response.json();
        setData(json);
      } catch (err) {
        if (err instanceof Error && err.name !== 'AbortError') {
          setError(err);
        }
      } finally {
        setIsLoading(false);
      }
    }

    fetchData();
    return () => controller.abort();
  }, [url]);

  return { data, error, isLoading };
}
```

### Local Storage Hook

```typescript
function useLocalStorage<T>(key: string, initialValue: T) {
  const [storedValue, setStoredValue] = useState<T>(() => {
    if (typeof window === 'undefined') return initialValue;
    try {
      const item = window.localStorage.getItem(key);
      return item ? JSON.parse(item) : initialValue;
    } catch {
      return initialValue;
    }
  });

  const setValue = (value: T | ((val: T) => T)) => {
    const valueToStore = value instanceof Function ? value(storedValue) : value;
    setStoredValue(valueToStore);
    window.localStorage.setItem(key, JSON.stringify(valueToStore));
  };

  return [storedValue, setValue] as const;
}
```

### Media Query Hook

```typescript
function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(false);

  useEffect(() => {
    const media = window.matchMedia(query);
    setMatches(media.matches);

    const listener = (e: MediaQueryListEvent) => setMatches(e.matches);
    media.addEventListener('change', listener);
    return () => media.removeEventListener('change', listener);
  }, [query]);

  return matches;
}
```

## 6. Error Handling

### Error Boundary

```typescript
'use client';

import { Component, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error caught:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback || (
        <div className="p-4 bg-red-50 border border-red-200 rounded">
          <h2 className="text-red-800 font-semibold">Something went wrong</h2>
          <p className="text-red-600 text-sm mt-1">{this.state.error?.message}</p>
        </div>
      );
    }
    return this.props.children;
  }
}
```

### Next.js Error Pages

```typescript
// app/error.tsx — Client error boundary
'use client';
export default function Error({ error, reset }) {
  return (
    <div>
      <h2>Error: {error.message}</h2>
      <button onClick={reset}>Try again</button>
    </div>
  );
}

// app/not-found.tsx — 404 page
export default function NotFound() {
  return <div>Page not found</div>;
}

// app/global-error.tsx — Root error boundary
'use client';
export default function GlobalError({ error, reset }) {
  return (
    <html>
      <body>
        <h2>Something went wrong!</h2>
        <button onClick={reset}>Try again</button>
      </body>
    </html>
  );
}
```

## 7. Performance Optimization

### Render Optimization

```typescript
// React.memo for expensive components
const ExpensiveList = React.memo(function ExpensiveList({ items }: { items: Item[] }) {
  return items.map(item => <ExpensiveItem key={item.id} item={item} />);
});

// useMemo for expensive computations
const sortedItems = useMemo(() => {
  return items.toSorted((a, b) => a.name.localeCompare(b.name));
}, [items]);

// useCallback for handler stability
const handleClick = useCallback((id: string) => {
  onSelect(id);
}, [onSelect]);
```

### Virtual Scrolling (Large Lists)

```typescript
// Use @tanstack/react-virtual for large lists
import { useVirtualizer } from '@tanstack/react-virtual';

function VirtualList({ items }: { items: Item[] }) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 50,
  });

  return (
    <div ref={parentRef} className="h-[500px] overflow-auto">
      <div style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtualRow) => (
          <div
            key={virtualRow.key}
            style={{
              position: 'absolute',
              top: virtualRow.start,
              height: virtualRow.size,
            }}
          >
            {items[virtualRow.index].name}
          </div>
        ))}
      </div>
    </div>
  );
}
```

## 8. React DevTools Profiling

### Steps

1. Install React DevTools browser extension
2. Open Profiler tab
3. Click Record
4. Perform interaction
5. Stop recording
6. Analyze:
   - Which components re-rendered
   - Why they re-rendered (props change, hooks, parent)
   - Render duration
   - Commit timeline

### Common Issues Found

| Symptom | Solution |
|---------|----------|
| Unnecessary re-renders | React.memo, useMemo, useCallback |
| Slow renders | Virtualization, code splitting |
| Large bundle | Dynamic imports, tree shaking |
| Memory leaks | Cleanup in useEffect, AbortController |

## Checklist

- [ ] TypeScript strict mode enabled
- [ ] No `any` types
- [ ] Server Components for data fetching
- [ ] Client Components only when needed
- [ ] Custom hooks extracted for reuse
- [ ] Error boundaries at route level
- [ ] Loading states for async operations
- [ ] Memoization applied where beneficial
- [ ] No console.log in production code
- [ ] All effects have cleanup functions
