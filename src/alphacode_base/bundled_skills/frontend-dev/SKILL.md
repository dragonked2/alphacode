---
name: frontend-dev
description: Comprehensive frontend development methodology covering modern SPA architecture, component design, state management, performance optimization, testing, and deployment workflows.
---

# Frontend Development Skill

Complete methodology for building production-grade frontend applications.

## Workflow Overview

```
Requirements → Design → Implement → Test → Optimize → Deploy
```

## 1. Requirements Gathering

### Checklist
- [ ] Define user stories and acceptance criteria
- [ ] Identify browser support targets (Chrome, Firefox, Safari, Edge)
- [ ] Determine accessibility requirements (WCAG 2.1 AA minimum)
- [ ] Establish performance budgets (LCP < 2.5s, CLS < 0.1, INP < 200ms)
- [ ] Define SEO requirements and metadata strategy
- [ ] Identify third-party integrations and APIs
- [ ] Determine responsive breakpoints and device targets
- [ ] Document authentication/authorization flows

## 2. Architecture Design

### Technology Stack Selection

```
Framework:    Next.js 15+ (App Router)
UI Library:   React 19+
Styling:      Tailwind CSS 4+
State:        Zustand (client) + React Query (server)
Testing:      Vitest + Playwright
Deployment:   Vercel / Docker + CDN
```

### Project Structure

```
src/
├── app/                    # Next.js App Router
│   ├── (routes)/           # Route groups
│   ├── layout.tsx          # Root layout
│   ├── page.tsx            # Home page
│   ├── loading.tsx         # Suspense fallback
│   ├── error.tsx           # Error boundary
│   └── not-found.tsx       # 404 page
├── components/
│   ├── ui/                 # Primitive components (atoms)
│   ├── features/           # Feature components (molecules)
│   ├── layouts/            # Layout components (organisms)
│   └── providers/          # Context providers
├── hooks/                  # Custom hooks
├── lib/                    # Utilities, API clients, configs
├── stores/                 # Zustand stores
├── types/                  # TypeScript types
└── styles/                 # Global styles, Tailwind config
```

### Component Architecture

Follow atomic design:
- **Atoms**: Button, Input, Badge, Icon, Typography
- **Molecules**: SearchBar, FormField, Card, Modal
- **Organisms**: Header, Sidebar, DataTable, HeroSection
- **Templates**: DashboardLayout, AuthLayout, LandingLayout
- **Pages**: Actual route implementations

## 3. Implementation

### TypeScript First

```typescript
// Always type props explicitly
interface ButtonProps {
  variant?: 'primary' | 'secondary' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
  children: React.ReactNode;
  disabled?: boolean;
  onClick?: () => void;
}

// Use discriminated unions for complex state
type FormState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'success'; data: FormData }
  | { status: 'error'; error: string };
```

### Server vs Client Components

```typescript
// Server Component (default in App Router)
// - Fetches data directly
// - Accesses backend resources
// - Reduces client bundle
async function ProductList() {
  const products = await fetchProducts();
  return <ProductGrid products={products} />;
}

// Client Component (when interactivity needed)
'use client';
function SearchInput({ onSearch }: { onSearch: (q: string) => void }) {
  const [query, setQuery] = useState('');
  return <input value={query} onChange={e => { setQuery(e.target.value); onSearch(e.target.value); }} />;
}
```

### State Management Decision Tree

```
Is it server state? → React Query / SWR
Is it form state? → React Hook Form + Zod
Is it URL state? → useSearchParams / nuqs
Is it global client state? → Zustand
Is it local component state? → useState / useReducer
```

## 4. Testing Strategy

### Test Pyramid

```
         ┌─────────┐
         │  E2E    │  10% — Playwright
         │ (Few)   │  Critical user journeys
         ├─────────┤
         │  INT    │  30% — Vitest + MSW
         │(Some)   │  API integration, data flows
         ├─────────┤
         │  UNIT   │  60% — Vitest + RTL
         │ (Many)  │  Components, hooks, utils
         └─────────┘
```

### Testing Checklist

- [ ] Unit tests for all utility functions
- [ ] Component tests for all UI components
- [ ] Hook tests for custom hooks
- [ ] Integration tests for API interactions
- [ ] E2E tests for critical user flows
- [ ] Accessibility tests with axe-core
- [ ] Visual regression tests for key pages

## 5. Performance Optimization

### Core Web Vitals Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| LCP | < 2.5s | Largest Contentful Paint |
| INP | < 200ms | Interaction to Next Paint |
| CLS | < 0.1 | Cumulative Layout Shift |
| FCP | < 1.8s | First Contentful Paint |
| TTFB | < 800ms | Time to First Byte |

### Optimization Checklist

- [ ] Code split at route level
- [ ] Lazy load below-fold components
- [ ] Optimize images (WebP/AVIF, proper sizing)
- [ ] Preload critical fonts
- [ ] Implement proper caching headers
- [ ] Minimize third-party scripts
- [ ] Use React Profiler to find re-renders
- [ ] Bundle analysis < 200KB initial JS
- [ ] Tree shake unused dependencies

## 6. Deployment

### Pre-Deployment Checklist

```bash
# Type checking
npm run typecheck

# Linting
npm run lint

# Tests
npm run test
npm run test:e2e

# Build
npm run build

# Performance audit
npx lighthouse-ci --config=lighthouse.config.js
```

### Deployment Steps

1. Push to main branch (triggers CI)
2. CI runs: lint → typecheck → test → build
3. Preview deployment for PRs
4. Production deployment on merge to main
5. Post-deploy: smoke tests, monitoring alerts

### Environment Variables

```
# Required
NEXT_PUBLIC_API_URL=
DATABASE_URL=
AUTH_SECRET=

# Optional
NEXT_PUBLIC_ANALYTICS_ID=
SENTRY_DSN=
```

## 7. Code Quality

### ESLint Rules

```json
{
  "extends": [
    "next/core-web-vitals",
    "plugin:@typescript-eslint/recommended",
    "plugin:react-hooks/recommended"
  ],
  "rules": {
    "no-console": "warn",
    "no-unused-vars": "off",
    "@typescript-eslint/no-unused-vars": ["error", { "argsIgnorePattern": "^_" }]
  }
}
```

### Git Conventions

```
feat:     New feature
fix:      Bug fix
refactor: Code change that neither fixes a bug nor adds a feature
style:    Formatting, no code change
test:     Adding missing tests
docs:     Documentation changes
perf:     Performance improvement
ci:       CI/CD changes
```

## Quick Reference

| Task | Command |
|------|---------|
| Dev server | `npm run dev` |
| Build | `npm run build` |
| Type check | `npm run typecheck` |
| Lint | `npm run lint` |
| Test | `npm run test` |
| Test E2E | `npm run test:e2e` |
| Format | `npm run format` |
