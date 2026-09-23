---
name: tailwind
description: Tailwind CSS 4+: utility-first design, responsive design, dark mode, themes, animations, component patterns, typography, colors, and plugin development.
---

# Tailwind CSS 4+ Development Skill

## Core Principles

1. **Utility-first** — Compose styles from small, single-purpose utilities
2. **Mobile-first** — Design for mobile, enhance for larger screens
3. **Constraint-based** — Use the design system, avoid arbitrary values
4. **Single source of truth** — CSS variables for theming

## 1. Design System Setup

### Theme Configuration

```css
/* app/globals.css */
@import "tailwindcss";

@theme {
  /* Colors */
  --color-primary-50: oklch(0.95 0.02 250);
  --color-primary-100: oklch(0.9 0.04 250);
  --color-primary-200: oklch(0.8 0.06 250);
  --color-primary-300: oklch(0.7 0.08 250);
  --color-primary-400: oklch(0.6 0.1 250);
  --color-primary-500: oklch(0.5 0.12 250);
  --color-primary-600: oklch(0.4 0.14 250);
  --color-primary-700: oklch(0.3 0.12 250);
  --color-primary-800: oklch(0.25 0.1 250);
  --color-primary-900: oklch(0.2 0.08 250);
  --color-primary-950: oklch(0.15 0.06 250);

  /* Semantic colors */
  --color-surface: var(--color-primary-50);
  --color-surface-dark: var(--color-primary-950);
  --color-text: var(--color-primary-950);
  --color-text-muted: var(--color-primary-600);
  --color-border: var(--color-primary-200);

  /* Spacing scale */
  --spacing-4xs: 0.125rem;
  --spacing-3xs: 0.25rem;
  --spacing-2xs: 0.375rem;
  --spacing-xs: 0.5rem;

  /* Border radius */
  --radius-sm: 0.25rem;
  --radius-md: 0.375rem;
  --radius-lg: 0.5rem;
  --radius-xl: 0.75rem;
  --radius-2xl: 1rem;

  /* Shadows */
  --shadow-sm: 0 1px 2px oklch(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px oklch(0 0 0 / 0.1);
  --shadow-lg: 0 10px 15px oklch(0 0 0 / 0.1);
  --shadow-xl: 0 20px 25px oklch(0 0 0 / 0.1);

  /* Animations */
  --animate-fade-in: fade-in 0.3s ease-out;
  --animate-slide-in: slide-in 0.3s ease-out;
  --animate-scale-in: scale-in 0.2s ease-out;
  --animate-pulse: pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes slide-in {
  from { transform: translateY(10px); opacity: 0; }
  to { transform: translateY(0); opacity: 1; }
}

@keyframes scale-in {
  from { transform: scale(0.95); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
```

### Dark Mode

```typescript
// Tailwind CSS 4 uses CSS variables for dark mode
// Set in globals.css
@custom-variant dark (&:is(.dark *));

// Usage in components
<div className="bg-white dark:bg-gray-900 text-gray-900 dark:text-gray-100">
  <h1 className="text-gray-900 dark:text-white">Title</h1>
  <p className="text-gray-600 dark:text-gray-400">Description</p>
</div>

// Toggle dark mode
'use client';
import { useEffect, useState } from 'react';

function ThemeToggle() {
  const [dark, setDark] = useState(false);

  useEffect(() => {
    document.documentElement.classList.toggle('dark', dark);
  }, [dark]);

  return (
    <button onClick={() => setDark(!dark)}>
      {dark ? '☀️' : '🌙'}
    </button>
  );
}
```

## 2. Responsive Design

### Breakpoint System

```typescript
// Mobile-first approach
// sm: 640px, md: 768px, lg: 1024px, xl: 1280px, 2xl: 1536px

// Card component - mobile first
<div className="
  grid grid-cols-1 gap-4
  sm:grid-cols-2
  md:grid-cols-3
  lg:grid-cols-4
  xl:grid-cols-5
">
  {items.map(item => <Card key={item.id} item={item} />)}
</div>

// Responsive typography
<h1 className="
  text-2xl font-bold
  sm:text-3xl
  md:text-4xl
  lg:text-5xl
">
  Responsive Heading
</h1>

// Container
<div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
  {children}
</div>
```

### Responsive Navigation

```typescript
function Navbar() {
  return (
    <nav className="bg-white dark:bg-gray-900 shadow">
      <div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
        <div className="flex h-16 items-center justify-between">
          {/* Logo */}
          <div className="flex-shrink-0">
            <span className="text-xl font-bold">Logo</span>
          </div>

          {/* Desktop nav */}
          <div className="hidden md:block">
            <div className="ml-10 flex items-baseline space-x-4">
              <a href="/" className="rounded-md px-3 py-2 text-sm font-medium hover:bg-gray-100 dark:hover:bg-gray-800">Home</a>
              <a href="/about" className="rounded-md px-3 py-2 text-sm font-medium hover:bg-gray-100 dark:hover:bg-gray-800">About</a>
            </div>
          </div>

          {/* Mobile menu button */}
          <div className="md:hidden">
            <button className="inline-flex items-center justify-center p-2 rounded-md hover:bg-gray-100 dark:hover:bg-gray-800">
              <span className="sr-only">Open menu</span>
              <Bars3Icon className="h-6 w-6" />
            </button>
          </div>
        </div>
      </div>
    </nav>
  );
}
```

## 3. Component Patterns

### Card

```typescript
function Card({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={`rounded-xl border border-gray-200 bg-white p-6 shadow-sm dark:border-gray-800 dark:bg-gray-900 ${className}`}>
      {children}
    </div>
  );
}

function CardHeader({ children }: { children: React.ReactNode }) {
  return <div className="mb-4 border-b border-gray-200 pb-4 dark:border-gray-800">{children}</div>;
}

function CardTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="text-lg font-semibold text-gray-900 dark:text-white">{children}</h3>;
}

function CardContent({ children }: { children: React.ReactNode }) {
  return <div>{children}</div>;
}

function CardFooter({ children }: { children: React.ReactNode }) {
  return <div className="mt-4 flex items-center justify-end gap-2">{children}</div>;
}
```

### Modal

```typescript
'use client';
import { Fragment } from 'react';
import { Dialog, Transition } from '@headlessui/react';

function Modal({ isOpen, onClose, title, children }: ModalProps) {
  return (
    <Transition appear show={isOpen} as={Fragment}>
      <Dialog as="div" onClose={onClose}>
        <Transition.Child
          as={Fragment}
          enter="ease-out duration-300"
          enterFrom="opacity-0"
          enterTo="opacity-100"
          leave="ease-in duration-200"
          leaveFrom="opacity-100"
          leaveTo="opacity-0"
        >
          <div className="fixed inset-0 bg-black/25" />
        </Transition.Child>

        <div className="fixed inset-0 overflow-y-auto">
          <div className="flex min-h-full items-center justify-center p-4">
            <Transition.Child
              as={Fragment}
              enter="ease-out duration-300"
              enterFrom="opacity-0 scale-95"
              enterTo="opacity-100 scale-100"
              leave="ease-in duration-200"
              leaveFrom="opacity-100 scale-100"
              leaveTo="opacity-0 scale-95"
            >
              <Dialog.Panel className="w-full max-w-md rounded-2xl bg-white p-6 shadow-xl dark:bg-gray-900">
                <Dialog.Title className="text-lg font-semibold text-gray-900 dark:text-white">
                  {title}
                </Dialog.Title>
                <div className="mt-4">{children}</div>
              </Dialog.Panel>
            </Transition.Child>
          </div>
        </div>
      </Dialog>
    </Transition>
  );
}
```

### Sidebar

```typescript
function Sidebar({ children, isOpen, onClose }: SidebarProps) {
  return (
    <>
      {/* Mobile overlay */}
      {isOpen && (
        <div className="fixed inset-0 z-40 bg-black/50 lg:hidden" onClick={onClose} />
      )}

      {/* Sidebar */}
      <aside className={`
        fixed inset-y-0 left-0 z-50 w-64 bg-white dark:bg-gray-900 shadow-lg transform transition-transform duration-300 ease-in-out
        lg:translate-x-0 lg:static lg:z-auto
        ${isOpen ? 'translate-x-0' : '-translate-x-full'}
      `}>
        <div className="flex h-full flex-col">
          {/* Header */}
          <div className="flex h-16 items-center justify-between px-4 border-b border-gray-200 dark:border-gray-800">
            <span className="text-xl font-bold text-gray-900 dark:text-white">Menu</span>
            <button onClick={onClose} className="lg:hidden">
              <XMarkIcon className="h-6 w-6" />
            </button>
          </div>

          {/* Navigation */}
          <nav className="flex-1 space-y-1 px-2 py-4">
            <SidebarLink href="/" icon={HomeIcon}>Dashboard</SidebarLink>
            <SidebarLink href="/settings" icon={CogIcon}>Settings</SidebarLink>
          </nav>

          {/* Footer */}
          <div className="border-t border-gray-200 p-4 dark:border-gray-800">
            <UserProfile />
          </div>
        </div>
      </aside>
    </>
  );
}

function SidebarLink({ href, icon: Icon, children }: SidebarLinkProps) {
  const isActive = usePathname() === href;
  return (
    <a
      href={href}
      className={`
        flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors
        ${isActive
          ? 'bg-primary-100 text-primary-700 dark:bg-primary-900 dark:text-primary-300'
          : 'text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800'
        }
      `}
    >
      <Icon className="h-5 w-5" />
      {children}
    </a>
  );
}
```

### Navbar

```typescript
function Navbar() {
  return (
    <header className="sticky top-0 z-40 border-b border-gray-200 bg-white/80 backdrop-blur-md dark:border-gray-800 dark:bg-gray-950/80">
      <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-4 sm:px-6 lg:px-8">
        {/* Logo */}
        <a href="/" className="flex items-center gap-2">
          <div className="h-8 w-8 rounded-lg bg-primary-500" />
          <span className="text-xl font-bold text-gray-900 dark:text-white">App</span>
        </a>

        {/* Desktop nav */}
        <nav className="hidden md:flex md:items-center md:gap-6">
          <a href="/" className="text-sm font-medium text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white">Home</a>
          <a href="/pricing" className="text-sm font-medium text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white">Pricing</a>
        </nav>

        {/* Actions */}
        <div className="flex items-center gap-4">
          <ThemeToggle />
          <Button variant="primary" size="sm">Sign In</Button>
        </div>
      </div>
    </header>
  );
}
```

## 4. Animations

### Transitions

```typescript
// Hover transitions
<button className="rounded-lg bg-primary-500 px-4 py-2 text-white transition-all hover:bg-primary-600 hover:shadow-lg active:scale-95">
  Click me
</button>

// Focus transitions
<input className="rounded-lg border border-gray-300 px-3 py-2 transition-colors focus:border-primary-500 focus:outline-none focus:ring-2 focus:ring-primary-500/20" />

// Group hover
<div className="group rounded-lg border p-4 transition-all hover:border-primary-500 hover:shadow-md">
  <h3 className="transition-colors group-hover:text-primary-600">Title</h3>
  <p className="text-gray-500 transition-colors group-hover:text-gray-700">Description</p>
</div>
```

### Custom Keyframes

```css
@keyframes bounce-subtle {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(-5px); }
}

@keyframes shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}

@keyframes progress {
  from { width: 0%; }
  to { width: 100%; }
}
```

### Skeleton Loading

```typescript
function Skeleton({ className }: { className?: string }) {
  return (
    <div className={`animate-pulse rounded bg-gray-200 dark:bg-gray-800 ${className}`} />
  );
}

function CardSkeleton() {
  return (
    <div className="rounded-xl border p-6">
      <Skeleton className="h-48 w-full rounded-lg" />
      <Skeleton className="mt-4 h-6 w-3/4" />
      <Skeleton className="mt-2 h-4 w-full" />
      <Skeleton className="mt-2 h-4 w-2/3" />
    </div>
  );
}
```

## 5. Typography

### Prose Classes

```typescript
// For rich text content
import { prose } from 'tailwindcss';

<article className="prose prose-gray dark:prose-invert max-w-none">
  <h1>Title</h1>
  <p>Paragraph with <strong>bold</strong> and <em>italic</em> text.</p>
  <ul>
    <li>List item 1</li>
    <li>List item 2</li>
  </ul>
  <blockquote>Important quote</blockquote>
  <pre><code>Code block</code></pre>
</article>
```

### Typography Scale

```typescript
// Display headings
<h1 className="text-4xl font-extrabold tracking-tight text-gray-900 dark:text-white sm:text-5xl lg:text-6xl">
  Large Display
</h1>

// Section headings
<h2 className="text-2xl font-bold tracking-tight text-gray-900 dark:text-white sm:text-3xl">
  Section Title
</h2>

// Body text
<p className="text-base text-gray-600 dark:text-gray-400">
  Body paragraph text
</p>

// Small/muted text
<p className="text-sm text-gray-500 dark:text-gray-500">
  Small muted text
</p>

// Labels
<label className="text-sm font-medium text-gray-700 dark:text-gray-300">
  Form label
</label>
```

## 6. Spacing System

### Consistent Spacing

```typescript
// Section spacing
<section className="py-12 sm:py-16 lg:py-20">
  <div className="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
    <div className="space-y-8">
      <div className="space-y-4">
        <h2>Section 1</h2>
      </div>
      <div className="space-y-4">
        <h2>Section 2</h2>
      </div>
    </div>
  </div>
</section>

// Card spacing
<div className="p-6">
  <div className="mb-4">Header content</div>
  <div className="space-y-2">
    <p>Item 1</p>
    <p>Item 2</p>
  </div>
  <div className="mt-6">Footer content</div>
</div>

// Grid gaps
<div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
  {items.map(item => <Card key={item.id} item={item} />)}
</div>
```

## 7. Color Palette Design

### Generating Palettes

```typescript
// Use Tailwind's color generator
// Generate from a base color using OKLCH

// Primary: Blue (#3B82F6)
// primary-50:  oklch(0.95 0.02 250)
// primary-100: oklch(0.90 0.04 250)
// primary-200: oklch(0.80 0.06 250)
// ... etc

// Semantic mapping
const colors = {
  success: {
    light: '#10B981', // green-500
    dark: '#34D399',  // green-400
  },
  warning: {
    light: '#F59E0B', // amber-500
    dark: '#FBBF24',  // amber-400
  },
  error: {
    light: '#EF4444', // red-500
    dark: '#F87171',  // red-400
  },
};
```

### Color Usage

```typescript
// Status indicators
<div className="flex items-center gap-2">
  <span className="h-2 w-2 rounded-full bg-green-500" />
  <span className="text-sm text-green-700 dark:text-green-400">Active</span>
</div>

<div className="flex items-center gap-2">
  <span className="h-2 w-2 rounded-full bg-yellow-500" />
  <span className="text-sm text-yellow-700 dark:text-yellow-400">Pending</span>
</div>

<div className="flex items-center gap-2">
  <span className="h-2 w-2 rounded-full bg-red-500" />
  <span className="text-sm text-red-700 dark:text-red-400">Error</span>
</div>

// Backgrounds with opacity
<div className="bg-primary-500/10 text-primary-700 dark:bg-primary-400/10 dark:text-primary-400">
  Info message
</div>

// Borders
<div className="border-l-4 border-primary-500 bg-primary-50 p-4 dark:bg-primary-950">
  Highlighted content
</div>
```

## 8. Form Patterns

### Form Layout

```typescript
<form className="space-y-6">
  <div>
    <label htmlFor="email" className="block text-sm font-medium text-gray-700 dark:text-gray-300">
      Email
    </label>
    <input
      id="email"
      type="email"
      className="mt-1 block w-full rounded-lg border border-gray-300 px-3 py-2 shadow-sm focus:border-primary-500 focus:outline-none focus:ring-2 focus:ring-primary-500/20 dark:border-gray-700 dark:bg-gray-800 dark:text-white"
    />
    <p className="mt-1 text-sm text-gray-500">We'll never share your email.</p>
  </div>

  <div>
    <label htmlFor="password" className="block text-sm font-medium text-gray-700 dark:text-gray-300">
      Password
    </label>
    <input
      id="password"
      type="password"
      className="mt-1 block w-full rounded-lg border border-gray-300 px-3 py-2 shadow-sm focus:border-primary-500 focus:outline-none focus:ring-2 focus:ring-primary-500/20 dark:border-gray-700 dark:bg-gray-800 dark:text-white"
    />
  </div>

  <button
    type="submit"
    className="w-full rounded-lg bg-primary-500 px-4 py-2 text-white font-medium hover:bg-primary-600 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2 transition-colors"
  >
    Sign In
  </button>
</form>
```

### Form States

```typescript
// Error state
<div>
  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">Email</label>
  <input className="mt-1 block w-full rounded-lg border border-red-500 px-3 py-2 focus:border-red-500 focus:ring-2 focus:ring-red-500/20" />
  <p className="mt-1 text-sm text-red-600 dark:text-red-400">Email is required</p>
</div>

// Success state
<div>
  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">Email</label>
  <input className="mt-1 block w-full rounded-lg border border-green-500 px-3 py-2 focus:border-green-500 focus:ring-2 focus:ring-green-500/20" />
  <p className="mt-1 text-sm text-green-600 dark:text-green-400">Email verified!</p>
</div>

// Disabled state
<button disabled className="w-full rounded-lg bg-gray-300 px-4 py-2 text-gray-500 cursor-not-allowed dark:bg-gray-700">
  Submitting...
</button>
```

## 9. Utilities

### Common Patterns

```typescript
// Truncate text
<p className="line-clamp-2 text-gray-600">
  This is a long paragraph that will be truncated after two lines.
</p>

// Gradient text
<h1 className="bg-gradient-to-r from-primary-500 to-purple-500 bg-clip-text text-transparent">
  Gradient Heading
</h1>

// Glassmorphism
<div className="rounded-xl border border-white/20 bg-white/10 p-6 backdrop-blur-md">
  Glass effect card
</div>

// Sticky footer layout
<div className="flex min-h-screen flex-col">
  <header className="sticky top-0 z-50">Header</header>
  <main className="flex-1">Content</main>
  <footer>Footer</footer>
</div>

// Aspect ratios
<div className="aspect-video rounded-lg bg-gray-200">
  16:9 container
</div>

<div className="aspect-square rounded-lg bg-gray-200">
  1:1 container
</div>
```

## Checklist

- [ ] Theme configured with CSS variables
- [ ] Dark mode support implemented
- [ ] Mobile-first responsive design
- [ ] Consistent spacing scale used
- [ ] Color palette follows accessibility (4.5:1 contrast)
- [ ] Animations smooth and purposeful
- [ ] Loading skeletons for async content
- [ ] Form states (idle, loading, success, error) styled
- [ ] No arbitrary values when possible
- [ ] Typography hierarchy clear
- [ ] Focus states visible for accessibility
- [ ] Reduced motion preference respected
