---
name: nextjs
description: Next.js 15+ development: App Router, Server Components, Server Actions, ISR/SSG/SSR, middleware, image/font optimization, metadata, routes, caching, and streaming.
---

# Next.js 15+ Development Skill

## Core Principles

1. **App Router exclusively** — No Pages Router
2. **Server Components by default** — Minimize client bundle
3. **Progressive enhancement** — Works without JavaScript, enhanced with it
4. **Type safety end-to-end** — Server Actions with Zod validation

## 1. App Router Architecture

### Route Structure

```
app/
├── layout.tsx              # Root layout (required)
├── page.tsx                # Home page (/)
├── loading.tsx             # Loading UI (Suspense)
├── error.tsx               # Error boundary
├── not-found.tsx           # 404 page
├── template.tsx            # Re-renders on navigation
├── default.tsx             # Parallel routes fallback
├── (marketing)/            # Route group (no URL segment)
│   ├── layout.tsx
│   ├── about/page.tsx      # /about
│   └── blog/page.tsx       # /blog
├── (dashboard)/            # Route group
│   ├── layout.tsx
│   ├── dashboard/page.tsx  # /dashboard
│   └── settings/page.tsx   # /settings
├── api/                    # API routes
│   └── webhook/route.ts    # /api/webhook
└── [...catchAll]/          # Catch-all route
    └── page.tsx            # matches /anything/here
```

### Layout Pattern

```typescript
// app/layout.tsx — Root layout
import { Inter } from 'next/font/google';
import { Providers } from '@/components/providers';

const inter = Inter({ subsets: ['latin'] });

export const metadata = {
  title: 'My App',
  description: 'Built with Next.js',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={inter.className}>
      <body>
        <Providers>
          <Header />
          <main>{children}</main>
          <Footer />
        </Providers>
      </body>
    </html>
  );
}

// app/(dashboard)/layout.tsx — Nested layout
export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex">
      <Sidebar />
      <div className="flex-1">{children}</div>
    </div>
  );
}
```

### Loading UI

```typescript
// app/loading.tsx
export default function Loading() {
  return (
    <div className="flex items-center justify-center min-h-screen">
      <div className="animate-spin rounded-full h-32 w-32 border-b-2 border-gray-900" />
    </div>
  );
}

// Per-route Suspense
import { Suspense } from 'react';
import { PostList } from './post-list';

export default function PostsPage() {
  return (
    <Suspense fallback={<PostSkeleton />}>
      <PostList />
    </Suspense>
  );
}
```

## 2. Server vs Client Components

### Decision Matrix

| Feature | Server Component | Client Component |
|---------|-----------------|-----------------|
| Data fetching | ✅ Direct | ❌ Via API/hooks |
| Backend access | ✅ Yes | ❌ No |
| Interactivity | ❌ No | ✅ Yes |
| useState/useEffect | ❌ No | ✅ Yes |
| Browser APIs | ❌ No | ✅ Yes |
| Bundle impact | ✅ None | ⚠️ Adds to bundle |

### Server Component Pattern

```typescript
// app/products/page.tsx — Server Component
import { db } from '@/lib/database';
import { ProductCard } from './product-card';
import { ProductFilters } from './product-filters';

export default async function ProductsPage({
  searchParams,
}: {
  searchParams: Promise<{ category?: string; sort?: string }>;
}) {
  const params = await searchParams;
  const products = await db.product.findMany({
    where: params.category ? { category: params.category } : undefined,
    orderBy: params.sort === 'price' ? { price: 'asc' } : undefined,
  });

  return (
    <div>
      <h1>Products</h1>
      <ProductFilters />
      <div className="grid grid-cols-3 gap-4">
        {products.map(product => (
          <ProductCard key={product.id} product={product} />
        ))}
      </div>
    </div>
  );
}
```

### Client Component Pattern

```typescript
// app/products/product-filters.tsx — Client Component
'use client';

import { useRouter, useSearchParams } from 'next/navigation';
import { useTransition } from 'react';

export function ProductFilters() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [isPending, startTransition] = useTransition();

  function handleFilter(category: string) {
    startTransition(() => {
      const params = new URLSearchParams(searchParams);
      if (category) {
        params.set('category', category);
      } else {
        params.delete('category');
      }
      router.push(`?${params.toString()}`);
    });
  }

  return (
    <div className={isPending ? 'opacity-50' : ''}>
      <button onClick={() => handleFilter('')}>All</button>
      <button onClick={() => handleFilter('electronics')}>Electronics</button>
      <button onClick={() => handleFilter('clothing')}>Clothing</button>
    </div>
  );
}
```

## 3. Server Actions

### Basic Usage

```typescript
// app/actions.ts
'use server';

import { revalidatePath, revalidateTag } from 'next/cache';
import { redirect } from 'next/navigation';
import { z } from 'zod';

const CreatePostSchema = z.object({
  title: z.string().min(1).max(100),
  content: z.string().min(1),
  published: z.boolean().default(false),
});

export type CreatePostInput = z.infer<typeof CreatePostSchema>;

export async function createPost(formData: FormData) {
  const rawData = {
    title: formData.get('title'),
    content: formData.get('content'),
    published: formData.get('published') === 'on',
  };

  const validated = CreatePostSchema.safeParse(rawData);

  if (!validated.success) {
    return { error: validated.error.flatten().fieldErrors };
  }

  await db.post.create({ data: validated.data });
  revalidatePath('/posts');
  redirect('/posts');
}
```

### Form with Optimistic Updates

```typescript
// app/posts/post-form.tsx
'use client';

import { useOptimistic } from 'react';
import { createPost } from '../actions';

export function PostForm({ posts }: { posts: Post[] }) {
  const [optimisticPosts, addOptimisticPost] = useOptimistic(
    posts,
    (state, newPost: Post) => [...state, { ...newPost, id: 'temp-id' }]
  );

  async function handleSubmit(formData: FormData) {
    const title = formData.get('title') as string;
    const content = formData.get('content') as string;

    addOptimisticPost({ id: 'temp', title, content, createdAt: new Date() });

    await createPost(formData);
  }

  return (
    <form action={handleSubmit}>
      <input name="title" required />
      <textarea name="content" required />
      <button type="submit">Create Post</button>
      <div>
        {optimisticPosts.map(post => (
          <div key={post.id}>{post.title}</div>
        ))}
      </div>
    </form>
  );
}
```

### Mutations with React Query + Server Actions

```typescript
// hooks/use-posts.ts
'use client';

import { useMutation, useQueryClient } from '@tanstack/react-query';
import { createPost } from '@/app/actions';

export function useCreatePost() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (formData: FormData) => {
      const result = await createPost(formData);
      if (result?.error) throw new Error('Failed to create post');
      return result;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['posts'] });
    },
  });
}
```

## 4. ISR / SSG / SSR Strategies

### Static Site Generation (SSG)

```typescript
// app/blog/[slug]/page.tsx
export async function generateStaticParams() {
  const posts = await fetch('https://api.example.com/posts').then(r => r.json());
  return posts.map((post: Post) => ({ slug: post.slug }));
}

export default async function BlogPost({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const post = await fetch(`https://api.example.com/posts/${slug}`).then(r => r.json());
  return <article>{post.content}</article>;
}
```

### Incremental Static Regeneration (ISR)

```typescript
// app/products/[id]/page.tsx
export const revalidate = 60; // Revalidate every 60 seconds

export default async function ProductPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const product = await fetch(`https://api.example.com/products/${id}`, {
    next: { revalidate: 60 },
  }).then(r => r.json());

  return <ProductDetail product={product} />;
}

// Tag-based revalidation
const data = await fetch('https://api.example.com/data', {
  next: { tags: ['data'] },
});

// Revalidate by tag
revalidateTag('data');

// Revalidate by path
revalidatePath('/products');
```

### Server-Side Rendering (SSR)

```typescript
// Force dynamic rendering
export const dynamic = 'force-dynamic';

// Or per-route with cookies/headers
export default async function DashboardPage({
  cookies,
  headers,
}: {
  cookies: Promise<ReadonlyRequestCookies>;
  headers: Promise<Headers>;
}) {
  const cookieStore = await cookies();
  const session = cookieStore.get('session');

  if (!session) redirect('/login');

  return <Dashboard session={session.value} />;
}
```

## 5. Middleware

### Authentication Middleware

```typescript
// middleware.ts
import { NextResponse, type NextRequest } from 'next/server';

const protectedRoutes = ['/dashboard', '/settings', '/profile'];
const authRoutes = ['/login', '/register'];

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const token = request.cookies.get('token')?.value;

  // Protected routes — redirect to login if no token
  if (protectedRoutes.some(route => pathname.startsWith(route))) {
    if (!token) {
      const loginUrl = new URL('/login', request.url);
      loginUrl.searchParams.set('redirect', pathname);
      return NextResponse.redirect(loginUrl);
    }
  }

  // Auth routes — redirect to dashboard if already logged in
  if (authRoutes.some(route => pathname.startsWith(route))) {
    if (token) {
      return NextResponse.redirect(new URL('/dashboard', request.url));
    }
  }

  return NextResponse.next();
}

export const config = {
  matcher: ['/dashboard/:path*', '/settings/:path*', '/profile/:path*', '/login', '/register'],
};
```

### Redirects and Rewrites

```typescript
// next.config.ts
const nextConfig = {
  async redirects() {
    return [
      {
        source: '/old-blog/:slug',
        destination: '/blog/:slug',
        permanent: true,
      },
      {
        source: '/docs/:path*',
        destination: 'https://docs.example.com/:path*',
        permanent: false,
      },
    ];
  },
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'https://backend.example.com/api/:path*',
      },
    ];
  },
};

export default nextConfig;
```

### Edge Middleware (Performance)

```typescript
// middleware.ts — Runs at edge, not Node.js
import { NextResponse } from 'next/server';
import type { NextRequest } from 'next/server';

export function middleware(request: NextRequest) {
  // A/B testing
  const bucket = request.cookies.get('ab-bucket')?.value || 'control';
  const response = NextResponse.next();
  response.headers.set('x-ab-bucket', bucket);

  // Geo-based routing
  const country = request.geo?.country || 'US';
  if (country === 'EU') {
    response.headers.set('x-eu-user', 'true');
  }

  return response;
}

export const config = {
  matcher: '/:path*',
  runtime: 'edge', // Run on edge runtime
};
```

## 6. Image Optimization

### next/image Usage

```typescript
import Image from 'next/image';

// Remote images
<Image
  src="https://example.com/hero.jpg"
  alt="Hero image"
  width={1200}
  height={600}
  priority // Load immediately (above fold)
  sizes="(max-width: 768px) 100vw, 50vw"
/>

// Local images
import heroImg from '@/public/hero.jpg';
<Image
  src={heroImg}
  alt="Hero"
  placeholder="blur" // Automatic blur placeholder
/>

// Background image pattern
<div className="relative h-96">
  <Image
    src="/bg.jpg"
    alt=""
    fill
    className="object-cover"
    sizes="100vw"
  />
</div>
```

### Image Configuration

```typescript
// next.config.ts
const nextConfig = {
  images: {
    formats: ['image/avif', 'image/webp'],
    remotePatterns: [
      {
        protocol: 'https',
        hostname: '**.example.com',
      },
    ],
    deviceSizes: [640, 750, 828, 1080, 1200, 1920, 2048],
    imageSizes: [16, 32, 48, 64, 96, 128, 256, 384],
  },
};
```

## 7. Font Optimization

```typescript
// app/layout.tsx
import { Inter, Playfair_Display } from 'next/font/google';

const inter = Inter({
  subsets: ['latin'],
  display: 'swap', // Show text immediately with fallback
  variable: '--font-inter',
});

const playfair = Playfair_Display({
  subsets: ['latin'],
  display: 'swap',
  variable: '--font-playfair',
});

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${inter.variable} ${playfair.variable}`}>
      <body className={inter.className}>
        {children}
      </body>
    </html>
  );
}
```

## 8. Metadata API

### Basic Metadata

```typescript
// app/page.tsx
import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: {
    template: '%s | My App',
    default: 'My App',
  },
  description: 'Built with Next.js',
  keywords: ['nextjs', 'react', 'typescript'],
  authors: [{ name: 'Me' }],
  openGraph: {
    title: 'My App',
    description: 'Built with Next.js',
    url: 'https://myapp.com',
    siteName: 'My App',
    locale: 'en_US',
    type: 'website',
  },
  twitter: {
    card: 'summary_large_image',
    title: 'My App',
    description: 'Built with Next.js',
  },
  robots: {
    index: true,
    follow: true,
  },
};
```

### Dynamic Metadata

```typescript
// app/blog/[slug]/page.tsx
export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  const post = await getPost(slug);

  return {
    title: post.title,
    description: post.excerpt,
    openGraph: {
      title: post.title,
      description: post.excerpt,
      type: 'article',
      publishedTime: post.publishedAt,
      authors: [post.author],
      images: [{ url: post.coverImage, width: 1200, height: 630 }],
    },
  };
}
```

### OG Image Generation

```typescript
// app/api/og/route.tsx
import { ImageResponse } from 'next/og';

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const title = searchParams.get('title') || 'My App';

  return new ImageResponse(
    (
      <div style={{ fontSize: 48, background: 'white', width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        {title}
      </div>
    ),
    { width: 1200, height: 630 }
  );
}
```

## 9. Advanced Routes

### Parallel Routes

```typescript
// app/layout.tsx
export default function Layout({ children, analytics, team }) {
  return (
    <div>
      {children}
      {analytics}
      {team}
    </div>
  );
}

// app/@analytics/page.tsx
export default function AnalyticsPage() {
  return <AnalyticsPanel />;
}

// app/@team/page.tsx
export default function TeamPage() {
  return <TeamPanel />;
}
```

### Intercepting Routes

```typescript
// app/feed/(..)photo/[id]/page.tsx — Intercept at /feed
// When navigating from /feed, shows as modal
// When accessed directly at /photo/[id], shows as page

export default function PhotoModal({ params }: { params: Promise<{ id: string }> }) {
  return (
    <Modal>
      <PhotoDetail id={params.id} />
    </Modal>
  );
}
```

### Route Handlers

```typescript
// app/api/users/route.ts
import { NextResponse } from 'next/server';

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const page = searchParams.get('page') || '1';

  const users = await getUsers(page);
  return NextResponse.json(users);
}

export async function POST(request: Request) {
  const body = await request.json();
  const user = await createUser(body);
  return NextResponse.json(user, { status: 201 });
}
```

## 10. Caching Strategies

### Route Segment Config

```typescript
// Time-based revalidation
export const revalidate = 3600; // 1 hour

// Force dynamic (no caching)
export const dynamic = 'force-dynamic';

// Force static (always cached)
export const dynamic = 'force-static';
```

### unstable_cache (Next.js 15+)

```typescript
import { unstable_cache } from 'next/cache';

const getCachedProducts = unstable_cache(
  async (category: string) => {
    return db.product.findMany({ where: { category } });
  },
  ['products-cache'],
  { revalidate: 3600, tags: ['products'] }
);

// Usage
const products = await getCachedProducts('electronics');
```

### fetch Caching

```typescript
// Default: cached
const data = await fetch('https://api.example.com/data');

// Opt out of cache
const data = await fetch('https://api.example.com/data', {
  cache: 'no-store',
});

// Revalidate specific fetch
const data = await fetch('https://api.example.com/data', {
  next: { revalidate: 3600 },
});

// Tag for targeted revalidation
const data = await fetch('https://api.example.com/data', {
  next: { tags: ['data'] },
});
```

## 11. Streaming with Suspense

### Streaming Layout

```typescript
// app/dashboard/layout.tsx
import { Suspense } from 'react';

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex">
      <aside className="w-64">
        <Suspense fallback={<SidebarSkeleton />}>
          <Sidebar />
        </Suspense>
      </aside>
      <main className="flex-1">{children}</main>
    </div>
  );
}
```

### Streaming Page

```typescript
// app/dashboard/page.tsx
import { Suspense } from 'react';
import { StatsCards } from './stats-cards';
import { RecentActivity } from './recent-activity';
import { QuickActions } from './quick-actions';

export default function DashboardPage() {
  return (
    <div className="space-y-6">
      <h1>Dashboard</h1>

      <Suspense fallback={<StatsSkeleton />}>
        <StatsCards />
      </Suspense>

      <div className="grid grid-cols-2 gap-6">
        <Suspense fallback={<ActivitySkeleton />}>
          <RecentActivity />
        </Suspense>
        <QuickActions />
      </div>
    </div>
  );
}
```

## Checklist

- [ ] App Router structure follows conventions
- [ ] Server Components used by default
- [ ] Client Components minimal and necessary
- [ ] Server Actions with Zod validation
- [ ] Metadata defined for all routes
- [ ] Images use next/image with proper sizing
- [ ] Fonts use next/font
- [ ] Loading states via Suspense boundaries
- [ ] Error boundaries at route level
- [ ] Middleware for auth/routing logic
- [ ] Caching strategy configured per route
- [ ] No client-side data fetching where server can handle it
