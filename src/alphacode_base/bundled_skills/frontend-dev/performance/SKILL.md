---
name: performance
description: Frontend performance: Core Web Vitals, code splitting, tree shaking, lazy loading, image optimization, caching, bundle analysis, font loading, virtual scrolling, and Web Workers.
---

# Frontend Performance Skill

## Performance Budget

| Metric | Target | Critical |
|--------|--------|----------|
| LCP | < 2.5s | > 4.0s |
| INP | < 200ms | > 500ms |
| CLS | < 0.1 | > 0.25 |
| FCP | < 1.8s | > 3.0s |
| TTFB | < 800ms | > 1.8s |
| Bundle (initial JS) | < 200KB | > 500KB |
| Total page weight | < 1MB | > 3MB |

## 1. Core Web Vitals

### LCP (Largest Contentful Paint)

**Measures**: Loading performance — when the largest content element becomes visible.

**Optimize by**:
- Preload hero images and critical resources
- Use `next/image` with priority for above-fold images
- Minimize server response time (TTFB)
- Remove render-blocking resources

```typescript
// Preload critical resources
<head>
  <link rel="preload" as="image" href="/hero.webp" />
  <link rel="preload" as="font" href="/font.woff2" crossOrigin="anonymous" />
</head>

// Use priority for above-fold images
<Image src="/hero.webp" alt="Hero" priority width={1200} height={600} />
```

### INP (Interaction to Next Paint)

**Measures**: Responsiveness — time from user interaction to visual response.

**Optimize by**:
- Minimize main thread work
- Break long tasks into smaller chunks
- Use `useTransition` for non-blocking updates
- Offload heavy computation to Web Workers

```typescript
// Break up long tasks
function processLargeDataset(items: Item[]) {
  const CHUNK_SIZE = 100;
  let index = 0;

  function processChunk(deadline: IdleDeadline) {
    while (index < items.length && deadline.timeRemaining() > 0) {
      processItem(items[index]);
      index++;
    }
    if (index < items.length) {
      requestIdleCallback(processChunk);
    }
  }

  requestIdleCallback(processChunk);
}
```

### CLS (Cumulative Layout Shift)

**Measures**: Visual stability — unexpected layout movement.

**Optimize by**:
- Set explicit dimensions for images and embeds
- Reserve space for dynamic content
- Avoid inserting content above existing content
- Use CSS `contain` for stability

```typescript
// Always set width/height on images
<Image src="/photo.jpg" alt="Photo" width={400} height={300} />

// Reserve space for lazy content
function CardSkeleton() {
  return (
    <div className="h-48 animate-pulse bg-gray-200 rounded-lg" />
  );
}

// Use aspect-ratio for responsive containers
<div className="aspect-video bg-gray-100">
  <iframe src="..." />
</div>
```

## 2. Code Splitting

### Route-Level Splitting

```typescript
// Next.js automatic code splitting
// Each route is automatically a separate chunk

// Dynamic imports for heavy components
import dynamic from 'next/dynamic';

const HeavyChart = dynamic(() => import('@/components/HeavyChart'), {
  loading: () => <div className="h-96 animate-pulse bg-gray-100" />,
  ssr: false, // Skip SSR if not needed
});

// Conditional dynamic import
const AdminPanel = dynamic(() => import('@/components/AdminPanel'));
const Analytics = dynamic(() => import('@/components/Analytics'));

function Dashboard({ role }: { role: string }) {
  return (
    <div>
      {role === 'admin' && <AdminPanel />}
      <Analytics />
    </div>
  );
}
```

### Component-Level Splitting

```typescript
// React.lazy + Suspense
import { lazy, Suspense } from 'react';

const MarkdownEditor = lazy(() => import('./MarkdownEditor'));
const VideoPlayer = lazy(() => import('./VideoPlayer'));

function PostEditor() {
  const [showPreview, setShowPreview] = useState(false);

  return (
    <div>
      <MarkdownEditor />
      {showPreview && (
        <Suspense fallback={<div>Loading preview...</div>}>
          <VideoPlayer src="/preview.mp4" />
        </Suspense>
      )}
    </div>
  );
}
```

### Webpack Bundle Analysis

```typescript
// next.config.ts
const nextConfig = {
  webpack: (config, { isServer }) => {
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
      };
    }
    return config;
  },
};

// Run: ANALYZE=true npm run build
```

## 3. Tree Shaking

### Best Practices

```typescript
// BAD: Import entire library
import _ from 'lodash';
const result = _.debounce(fn, 300);

// GOOD: Import specific function
import debounce from 'lodash/debounce';
const result = debounce(fn, 300);

// BAD: Import entire icon library
import { Icon } from '@heroicons/react/24/outline';

// GOOD: Import specific icon
import { ChevronDownIcon } from '@heroicons/react/24/outline';

// BAD: Barrel file re-exports everything
// utils/index.ts
export * from './auth';
export * from './api';
export * from './helpers';

// GOOD: Direct imports
import { login } from '@/utils/auth';
import { fetchAPI } from '@/utils/api';
```

### Package Optimization

```json
// package.json — check for sideEffects
{
  "sideEffects": false
}

// Or specify which files have side effects
{
  "sideEffects": ["*.css", "*.scss"]
}
```

## 4. Lazy Loading

### Image Lazy Loading

```typescript
// Next.js automatic lazy loading (default)
<Image src="/photo.jpg" alt="Photo" width={400} height={300} />

// Intersection Observer pattern
function LazyImage({ src, alt }: { src: string; alt: string }) {
  const imgRef = useRef<HTMLImageElement>(null);
  const [isVisible, setIsVisible] = useState(false);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: '200px' } // Start loading 200px before visible
    );

    if (imgRef.current) observer.observe(imgRef.current);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={imgRef} className="aspect-video">
      {isVisible && (
        <img src={src} alt={alt} className="w-full h-full object-cover" loading="lazy" />
      )}
    </div>
  );
}
```

### Component Lazy Loading

```typescript
// Below-fold components
function BelowFold() {
  const [shouldLoad, setShouldLoad] = useState(false);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setShouldLoad(true);
          observer.disconnect();
        }
      }
    );

    const sentinel = document.getElementById('below-fold-sentinel');
    if (sentinel) observer.observe(sentinel);
    return () => observer.disconnect();
  }, []);

  return (
    <>
      <div id="below-fold-sentinel" />
      {shouldLoad && <HeavyComponent />}
    </>
  );
}
```

### Route Lazy Loading

```typescript
// Next.js automatic route-based splitting
// Each page is automatically a separate chunk

// Manual route splitting for non-Next.js
const Home = lazy(() => import('./pages/Home'));
const About = lazy(() => import('./pages/About'));
const Dashboard = lazy(() => import('./pages/Dashboard'));

function App() {
  return (
    <Suspense fallback={<PageLoader />}>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/about" element={<About />} />
        <Route path="/dashboard" element={<Dashboard />} />
      </Routes>
    </Suspense>
  );
}
```

## 5. Image Optimization

### Format Strategy

```
Modern browsers: AVIF > WebP > JPEG
Fallback: JPEG for older browsers
```

### Image Sizes

```typescript
// Responsive image with sizes
<Image
  src="/hero.jpg"
  alt="Hero"
  width={1200}
  height={600}
  sizes="(max-width: 768px) 100vw, (max-width: 1200px) 50vw, 33vw"
/>

// Next.js generates multiple sizes automatically
// 16, 32, 48, 64, 96, 128, 256, 384, 640, 750, 828, 1080, 1200, 1920, 2048
```

### Blur Placeholder

```typescript
// Automatic blur placeholder
<Image
  src="/photo.jpg"
  alt="Photo"
  width={400}
  height={300}
  placeholder="blur"
  blurDataURL="data:image/jpeg;base64,/9j/4AAQSkZJRg..." // Generate with sharp
/>
```

### External Image Optimization

```typescript
// next.config.ts
const nextConfig = {
  images: {
    formats: ['image/avif', 'image/webp'],
    remotePatterns: [
      { protocol: 'https', hostname: '**.example.com' },
    ],
    minimumCacheTTL: 60 * 60 * 24 * 30, // 30 days
  },
};
```

## 6. Caching Strategies

### HTTP Cache Headers

```typescript
// next.config.ts
const nextConfig = {
  async headers() {
    return [
      {
        source: '/api/:path*',
        headers: [
          { key: 'Cache-Control', value: 'public, s-maxage=60, stale-while-revalidate=3600' },
        ],
      },
      {
        source: '/_next/static/:path*',
        headers: [
          { key: 'Cache-Control', value: 'public, max-age=31536000, immutable' },
        ],
      },
    ];
  },
};
```

### Service Worker

```typescript
// public/sw.js
const CACHE_NAME = 'v1';
const STATIC_ASSETS = ['/', '/offline', '/styles.css', '/app.js'];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(STATIC_ASSETS))
  );
});

self.addEventListener('fetch', (event) => {
  event.respondWith(
    caches.match(event.request).then((cached) => {
      // Stale-while-revalidate
      const fetchPromise = fetch(event.request).then((response) => {
        if (response.ok) {
          const clone = response.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(event.request, clone));
        }
        return response;
      }).catch(() => cached || caches.match('/offline'));

      return cached || fetchPromise;
    })
  );
});
```

### React Query Caching

```typescript
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,      // 5 minutes
      gcTime: 10 * 60 * 1000,         // 10 minutes (garbage collection)
      retry: 3,
      refetchOnWindowFocus: false,
    },
  },
});
```

## 7. Bundle Analysis

### Analysis Tools

```bash
# Next.js bundle analyzer
ANALYZE=true npm run build

# webpack-bundle-analyzer
npx webpack-bundle-analyzer .next/stats.json

# Source map explorer
npx source-map-explorer 'dist/**/*.js'

# Size limit
npx size-limit
```

### Bundle Size Checklist

```typescript
// .size-limit.json
[
  {
    "path": "dist/**/*.js",
    "limit": "200 KB",
    "webpack": false
  },
  {
    "path": "dist/chunks/**/*.js",
    "limit": "50 KB"
  }
]
```

### Common Large Dependencies

| Package | Alternative | Size Savings |
|---------|-------------|--------------|
| moment.js | date-fns / dayjs | ~300KB |
| lodash | lodash-es / ramda | ~70KB |
| @mui/material | tailwindcss | ~500KB |
| axios | fetch | ~30KB |

## 8. Font Loading

### Font Strategy

```typescript
// next/font — optimal font loading
import { Inter, Playfair_Display } from 'next/font/google';

const inter = Inter({
  subsets: ['latin'],
  display: 'swap', // Show text immediately with fallback
  preload: true,   // Preload font files
  fallback: ['system-ui', 'sans-serif'],
});

const playfair = Playfair_Display({
  subsets: ['latin'],
  display: 'swap',
  preload: false, // Only preload primary font
});
```

### Font Loading Pattern

```css
/* Fallback font metrics matching */
@font-face {
  font-family: 'Inter';
  src: url('/fonts/inter.woff2') format('woff2');
  font-weight: 100 900;
  font-display: swap;
}

/* Match fallback metrics */
:root {
  --font-inter: 'Inter', system-ui, -apple-system, sans-serif;
  --font-mono: 'JetBrains Mono', ui-monospace, monospace;
}
```

## 9. Preloading & Prefetching

### Resource Hints

```typescript
// head.tsx or layout.tsx
<head>
  {/* Preconnect to external domains */}
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://api.example.com" />

  {/* DNS prefetch for less critical */}
  <link rel="dns-prefetch" href="https://analytics.example.com" />

  {/* Prefetch next page */}
  <link rel="prefetch" href="/dashboard" />

  {/* Preload critical resources */}
  <link rel="preload" as="font" href="/fonts/inter.woff2" type="font/woff2" crossOrigin="anonymous" />
</head>
```

### Next.js Link Prefetching

```typescript
// Automatic prefetching on hover (default)
<Link href="/about">About</Link>

// Disable prefetch for distant routes
<Link href="/admin" prefetch={false}>Admin</Link>

// Prefetch on viewport intersection
function NavLink({ href, children }: { href: string; children: ReactNode }) {
  const linkRef = useRef<HTMLAnchorElement>(null);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          const link = linkRef.current;
          if (link) {
            const prefetchUrl = new URL(link.href, window.location.origin);
            const prefetchLink = document.createElement('link');
            prefetchLink.rel = 'prefetch';
            prefetchLink.href = prefetchUrl.pathname;
            document.head.appendChild(prefetchLink);
          }
          observer.disconnect();
        }
      },
      { rootMargin: '200px' }
    );

    if (linkRef.current) observer.observe(linkRef.current);
    return () => observer.disconnect();
  }, []);

  return <a ref={linkRef} href={href}>{children}</a>;
}
```

## 10. Virtual Scrolling

### Large List Optimization

```typescript
'use client';
import { useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';

interface VirtualListProps<T> {
  items: T[];
  renderItem: (item: T, index: number) => React.ReactNode;
  estimateSize?: number;
}

function VirtualList<T>({ items, renderItem, estimateSize = 50 }: VirtualListProps<T>) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    overscan: 5, // Render 5 extra items above/below viewport
  });

  return (
    <div
      ref={parentRef}
      className="h-[500px] overflow-auto rounded-lg border"
    >
      <div
        className="relative w-full"
        style={{ height: virtualizer.getTotalSize() }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => (
          <div
            key={virtualRow.key}
            className="absolute left-0 top-0 w-full"
            style={{
              height: virtualRow.size,
              transform: `translateY(${virtualRow.start}px)`,
            }}
          >
            {renderItem(items[virtualRow.index], virtualRow.index)}
          </div>
        ))}
      </div>
    </div>
  );
}

// Usage
<VirtualList
  items={users}
  renderItem={(user) => (
    <div className="flex items-center gap-3 border-b p-3">
      <Avatar src={user.avatar} />
      <span>{user.name}</span>
    </div>
  )}
/>
```

## 11. Web Workers

### CPU-Intensive Tasks

```typescript
// workers/image-processor.worker.ts
self.onmessage = function(e) {
  const { imageData, operation } = e.data;

  let result;
  switch (operation) {
    case 'grayscale':
      result = applyGrayscale(imageData);
      break;
    case 'blur':
      result = applyBlur(imageData, e.data.radius);
      break;
    default:
      result = imageData;
  }

  self.postMessage({ result });
};

function applyGrayscale(imageData: ImageData): ImageData {
  const data = imageData.data;
  for (let i = 0; i < data.length; i += 4) {
    const avg = (data[i] + data[i + 1] + data[i + 2]) / 3;
    data[i] = avg;
    data[i + 1] = avg;
    data[i + 2] = avg;
  }
  return imageData;
}

// hooks/use-web-worker.ts
function useWebWorker(workerPath: string) {
  const workerRef = useRef<Worker | null>(null);

  useEffect(() => {
    workerRef.current = new Worker(workerPath);
    return () => workerRef.current?.terminate();
  }, [workerPath]);

  function postMessage<T>(data: T): Promise<T> {
    return new Promise((resolve) => {
      const worker = workerRef.current;
      if (!worker) return;

      worker.onmessage = (e) => resolve(e.data.result);
      worker.postMessage(data);
    });
  }

  return { postMessage };
}

// Usage
function ImageEditor() {
  const { postMessage } = useWebWorker('/workers/image-processor.worker.js');

  async function handleGrayscale(imageData: ImageData) {
    const result = await postMessage({ imageData, operation: 'grayscale' });
    // Update canvas with result
  }

  return <button onClick={() => handleGrayscale(currentImage)}>Grayscale</button>;
}
```

## 12. Lighthouse Optimization

### Audit Checklist

```bash
# Run Lighthouse
npx lighthouse https://myapp.com --output html --output-path ./report.html

# CI integration
npx lighthouse-ci autorun --config=lighthouse.config.js
```

### Common Fixes

| Issue | Solution |
|-------|----------|
| Eliminate render-blocking resources | Defer non-critical JS/CSS, inline critical CSS |
| Properly size images | Use `next/image` with sizes prop |
| Serve images in modern formats | Use AVIF/WebP via `next/image` |
| Preconnect to required origins | Add `<link rel="preconnect">` |
| Reduce unused JavaScript | Code split, tree shake |
| Minimize main-thread work | Offload to Web Workers |
| Avoid excessive DOM size | Virtualize large lists |
| Use efficient cache policy | Set proper Cache-Control headers |

## Checklist

- [ ] Core Web Vitals within targets
- [ ] Code splitting at route level
- [ ] Images optimized (WebP/AVIF, proper sizing)
- [ ] Fonts loaded with `next/font`
- [ ] Caching headers configured
- [ ] Bundle size under 200KB initial
- [ ] Tree shaking verified
- [ ] Third-party scripts minimized
- [ ] Virtual scrolling for large lists
- [ ] Web Workers for CPU-intensive tasks
- [ ] Lighthouse score > 90
- [ ] Performance regression tests in CI
