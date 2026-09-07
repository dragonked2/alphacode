---
name: testing
description: Frontend testing: Vitest, React Testing Library, Playwright E2E, Storybook visual/interaction testing, accessibility testing, MSW mocking, TDD, and CI/CD integration.
---

# Frontend Testing Skill

## Test Pyramid

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

## 1. Vitest Configuration

### Setup

```typescript
// vitest.config.ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./tests/setup.ts'],
    include: ['**/*.{test,spec}.{ts,tsx}'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'tests/'],
      thresholds: {
        statements: 80,
        branches: 80,
        functions: 80,
        lines: 80,
      },
    },
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
});
```

### Test Setup

```typescript
// tests/setup.ts
import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach, beforeAll, afterAll } from 'vitest';
import { server } from './mocks/server';

// Cleanup after each test
afterEach(() => {
  cleanup();
});

// Start MSW server
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterAll(() => server.close());

// Mock window.matchMedia
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation(query => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

// Mock IntersectionObserver
class MockIntersectionObserver {
  observe = vi.fn();
  unobserve = vi.fn();
  disconnect = vi.fn();
}
Object.defineProperty(window, 'IntersectionObserver', {
  writable: true,
  value: MockIntersectionObserver,
});

// Mock scrollTo
window.scrollTo = vi.fn();
```

## 2. Unit Testing (Vitest + React Testing Library)

### Component Testing

```typescript
// components/__tests__/Button.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Button } from '../Button';

describe('Button', () => {
  it('renders children correctly', () => {
    render(<Button>Click me</Button>);
    expect(screen.getByRole('button', { name: /click me/i })).toBeInTheDocument();
  });

  it('calls onClick when clicked', async () => {
    const user = userEvent.setup();
    const handleClick = vi.fn();
    render(<Button onClick={handleClick}>Click</Button>);

    await user.click(screen.getByRole('button'));
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it('does not call onClick when disabled', async () => {
    const user = userEvent.setup();
    const handleClick = vi.fn();
    render(<Button disabled onClick={handleClick}>Click</Button>);

    await user.click(screen.getByRole('button'));
    expect(handleClick).not.toHaveBeenCalled();
  });

  it('shows loading state', () => {
    render(<Button isLoading>Submit</Button>);
    expect(screen.getByRole('button')).toBeDisabled();
    expect(screen.getByText(/submit/i)).toBeInTheDocument();
  });

  it('applies variant styles', () => {
    const { rerender } = render(<Button variant="primary">Test</Button>);
    expect(screen.getByRole('button')).toHaveClass('bg-primary-500');

    rerender(<Button variant="secondary">Test</Button>);
    expect(screen.getByRole('button')).toHaveClass('bg-gray-100');
  });

  it('forwards ref', () => {
    const ref = { current: null };
    render(<Button ref={ref}>Test</Button>);
    expect(ref.current).toBeInstanceOf(HTMLButtonElement);
  });
});
```

### Hook Testing

```typescript
// hooks/__tests__/useCounter.test.ts
import { renderHook, act } from '@testing-library/react';
import { useCounter } from '../useCounter';

describe('useCounter', () => {
  it('initializes with default value', () => {
    const { result } = renderHook(() => useCounter(0));
    expect(result.current.count).toBe(0);
  });

  it('increments', () => {
    const { result } = renderHook(() => useCounter(0));
    act(() => result.current.increment());
    expect(result.current.count).toBe(1);
  });

  it('decrements', () => {
    const { result } = renderHook(() => useCounter(0));
    act(() => result.current.decrement());
    expect(result.current.count).toBe(-1);
  });

  it('resets to initial value', () => {
    const { result } = renderHook(() => useCounter(5));
    act(() => result.current.increment());
    act(() => result.current.increment());
    act(() => result.current.reset());
    expect(result.current.count).toBe(5);
  });
});
```

### Utility Testing

```typescript
// lib/__tests__/utils.test.ts
import { formatDate, slugify, truncate } from '../utils';

describe('formatDate', () => {
  it('formats date correctly', () => {
    const date = new Date('2024-01-15T12:00:00Z');
    expect(formatDate(date)).toBe('January 15, 2024');
  });

  it('handles relative time', () => {
    const now = new Date();
    const fiveMinutesAgo = new Date(now.getTime() - 5 * 60 * 1000);
    expect(formatDate(fiveMinutesAgo, 'relative')).toBe('5 minutes ago');
  });
});

describe('slugify', () => {
  it('converts string to slug', () => {
    expect(slugify('Hello World')).toBe('hello-world');
  });

  it('removes special characters', () => {
    expect(slugify('Hello! @World#')).toBe('hello-world');
  });

  it('handles multiple spaces', () => {
    expect(slugify('Hello   World')).toBe('hello-world');
  });
});

describe('truncate', () => {
  it('truncates long text', () => {
    expect(truncate('Hello World', 5)).toBe('Hello...');
  });

  it('does not truncate short text', () => {
    expect(truncate('Hi', 5)).toBe('Hi');
  });
});
```

## 3. Integration Testing (Vitest + MSW)

### MSW Setup

```typescript
// tests/mocks/handlers.ts
import { http, HttpResponse } from 'msw';

const API_BASE = 'https://api.example.com';

export const handlers = [
  // GET /posts
  http.get(`${API_BASE}/posts`, ({ request }) => {
    const url = new URL(request.url);
    const page = url.searchParams.get('page') || '1';

    return HttpResponse.json({
      posts: [
        { id: '1', title: 'Post 1', content: 'Content 1' },
        { id: '2', title: 'Post 2', content: 'Content 2' },
      ],
      pagination: { page: Number(page), total: 10 },
    });
  }),

  // GET /posts/:id
  http.get(`${API_BASE}/posts/:id`, ({ params }) => {
    const { id } = params;
    if (id === '999') {
      return new HttpResponse(null, { status: 404 });
    }
    return HttpResponse.json({
      id,
      title: `Post ${id}`,
      content: `Content ${id}`,
    });
  }),

  // POST /posts
  http.post(`${API_BASE}/posts`, async ({ request }) => {
    const body = await request.json() as { title: string; content: string };
    return HttpResponse.json(
      { id: '3', ...body, createdAt: new Date().toISOString() },
      { status: 201 }
    );
  }),
];

// tests/mocks/server.ts
import { setupServer } from 'msw/node';
import { handlers } from './handlers';

export const server = setupServer(...handlers);
```

### Integration Test

```typescript
// components/__tests__/PostList.integration.test.tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PostList } from '../PostList';
import { server } from '../../tests/mocks/server';
import { http, HttpResponse } from 'msw';

function renderWithProviders(ui: React.ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>
  );
}

describe('PostList Integration', () => {
  it('loads and displays posts', async () => {
    renderWithProviders(<PostList />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Post 1')).toBeInTheDocument();
      expect(screen.getByText('Post 2')).toBeInTheDocument();
    });
  });

  it('handles API error', async () => {
    server.use(
      http.get('https://api.example.com/posts', () => {
        return new HttpResponse(null, { status: 500 });
      })
    );

    renderWithProviders(<PostList />);

    await waitFor(() => {
      expect(screen.getByText(/error loading posts/i)).toBeInTheDocument();
    });
  });

  it('creates a new post', async () => {
    const user = userEvent.setup();
    renderWithProviders(<PostList />);

    await screen.findByText('Post 1');

    await user.click(screen.getByRole('button', { name: /new post/i }));
    await user.type(screen.getByLabelText(/title/i), 'My New Post');
    await user.type(screen.getByLabelText(/content/i), 'Post content');
    await user.click(screen.getByRole('button', { name: /submit/i }));

    await waitFor(() => {
      expect(screen.getByText('My New Post')).toBeInTheDocument();
    });
  });
});
```

## 4. E2E Testing (Playwright)

### Configuration

```typescript
// playwright.config.ts
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [
    ['html', { open: 'never' }],
    ['json', { outputFile: 'test-results.json' }],
  ],
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
    { name: 'firefox', use: { browserName: 'firefox' } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
  webServer: {
    command: 'npm run dev',
    port: 3000,
    reuseExistingServer: !process.env.CI,
  },
});
```

### Page Object Pattern

```typescript
// e2e/pages/LoginPage.ts
import { type Page, type Locator, expect } from '@playwright/test';

export class LoginPage {
  readonly page: Page;
  readonly emailInput: Locator;
  readonly passwordInput: Locator;
  readonly submitButton: Locator;
  readonly errorMessage: Locator;

  constructor(page: Page) {
    this.page = page;
    this.emailInput = page.getByLabel(/email/i);
    this.passwordInput = page.getByLabel(/password/i);
    this.submitButton = page.getByRole('button', { name: /sign in/i });
    this.errorMessage = page.getByRole('alert');
  }

  async goto() {
    await this.page.goto('/login');
  }

  async login(email: string, password: string) {
    await this.emailInput.fill(email);
    await this.passwordInput.fill(password);
    await this.submitButton.click();
  }
}
```

### E2E Test

```typescript
// e2e/auth.spec.ts
import { test, expect } from '@playwright/test';
import { LoginPage } from './pages/LoginPage';

test.describe('Authentication', () => {
  test('user can login with valid credentials', async ({ page }) => {
    const loginPage = new LoginPage(page);
    await loginPage.goto();
    await loginPage.login('user@example.com', 'password123');

    await expect(page).toHaveURL('/dashboard');
    await expect(page.getByText(/welcome/i)).toBeVisible();
  });

  test('user sees error with invalid credentials', async ({ page }) => {
    const loginPage = new LoginPage(page);
    await loginPage.goto();
    await loginPage.login('wrong@example.com', 'wrong');

    await expect(loginPage.errorMessage).toBeVisible();
    await expect(loginPage.errorMessage).toContainText(/invalid/i);
  });

  test('user can logout', async ({ page }) => {
    // Login first
    await page.goto('/login');
    await page.getByLabel(/email/i).fill('user@example.com');
    await page.getByLabel(/password/i).fill('password123');
    await page.getByRole('button', { name: /sign in/i }).click();
    await expect(page).toHaveURL('/dashboard');

    // Logout
    await page.getByRole('button', { name: /logout/i }).click();
    await expect(page).toHaveURL('/login');
  });
});

test.describe('Protected Routes', () => {
  test('redirects to login when not authenticated', async ({ page }) => {
    await page.goto('/dashboard');
    await expect(page).toHaveURL(/.*login/);
  });

  test('allows access when authenticated', async ({ page }) => {
    // Setup authenticated state
    await page.goto('/login');
    await page.getByLabel(/email/i).fill('user@example.com');
    await page.getByLabel(/password/i).fill('password123');
    await page.getByRole('button', { name: /sign in/i }).click();

    await page.goto('/dashboard');
    await expect(page).toHaveURL('/dashboard');
  });
});
```

### Visual Regression

```typescript
// e2e/visual.spec.ts
import { test, expect } from '@playwright/test';

test('homepage matches screenshot', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveScreenshot('homepage.png', {
    maxDiffPixelRatio: 0.01,
  });
});

test('dashboard matches screenshot', async ({ page }) => {
  // Login
  await page.goto('/login');
  await page.getByLabel(/email/i).fill('user@example.com');
  await page.getByLabel(/password/i).fill('password123');
  await page.getByRole('button', { name: /sign in/i }).click();

  await expect(page).toHaveScreenshot('dashboard.png', {
    maxDiffPixelRatio: 0.01,
  });
});
```

## 5. Accessibility Testing

### axe-core Integration

```typescript
// tests/accessibility/home.test.tsx
import { render } from '@testing-library/react';
import { axe, toHaveNoViolations } from 'jest-axe';
import { HomePage } from '@/app/page';

expect.extend(toHaveNoViolations);

describe('HomePage Accessibility', () => {
  it('has no accessibility violations', async () => {
    const { container } = render(<HomePage />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});

// Playwright accessibility testing
// e2e/accessibility.spec.ts
import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test('homepage has no accessibility violations', async ({ page }) => {
  await page.goto('/');
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .analyze();
  expect(results.violations).toEqual([]);
});

test('login page has no accessibility violations', async ({ page }) => {
  await page.goto('/login');
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa'])
    .analyze();
  expect(results.violations).toEqual([]);
});
```

### Keyboard Navigation Testing

```typescript
// e2e/keyboard.spec.ts
import { test, expect } from '@playwright/test';

test('keyboard navigation through form', async ({ page }) => {
  await page.goto('/login');

  // Tab to email input
  await page.keyboard.press('Tab');
  await expect(page.getByLabel(/email/i)).toBeFocused();

  // Tab to password input
  await page.keyboard.press('Tab');
  await expect(page.getByLabel(/password/i)).toBeFocused();

  // Tab to submit button
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: /sign in/i })).toBeFocused();

  // Enter to submit
  await page.getByLabel(/email/i).fill('user@example.com');
  await page.getByLabel(/password/i).fill('password123');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL('/dashboard');
});

test('modal keyboard trap', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /open modal/i }).click();

  const modal = page.getByRole('dialog');
  await expect(modal).toBeVisible();

  // Tab should cycle within modal
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');

  // Focus should stay in modal
  const focused = await page.evaluate(() => document.activeElement?.closest('[role="dialog"]'));
  expect(focused).toBeTruthy();
});
```

## 6. Mock Strategies

### MSW for API Mocking

```typescript
// Mock handlers for different scenarios
const handlers = [
  // Success response
  http.get('/api/users', () => {
    return HttpResponse.json([
      { id: '1', name: 'John' },
      { id: '2', name: 'Jane' },
    ]);
  }),

  // Error response
  http.get('/api/users/:id', ({ params }) => {
    const { id } = params;
    if (id === '999') {
      return new HttpResponse(
        JSON.stringify({ error: 'User not found' }),
        { status: 404 }
      );
    }
    return HttpResponse.json({ id, name: 'John' });
  }),

  // Delayed response
  http.get('/api/slow', async () => {
    await new Promise(resolve => setTimeout(resolve, 2000));
    return HttpResponse.json({ data: 'slow response' });
  }),

  // Request verification
  http.post('/api/posts', async ({ request }) => {
    const body = await request.json() as any;
    return HttpResponse.json(
      { id: '1', ...body },
      { status: 201 }
    );
  }),
];
```

### Mock Custom Hooks

```typescript
// hooks/__tests__/useUser.test.ts
import { renderHook } from '@testing-library/react';
import { useUser } from '../useUser';
import { server } from '../../tests/mocks/server';
import { http, HttpResponse } from 'msw';

describe('useUser', () => {
  it('returns user data', async () => {
    const { result } = renderHook(() => useUser('1'));

    expect(result.current.isLoading).toBe(true);

    // Wait for data to load
    await vi.waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.user).toEqual({ id: '1', name: 'John' });
  });

  it('handles error', async () => {
    server.use(
      http.get('/api/users/999', () => {
        return new HttpResponse(null, { status: 404 });
      })
    );

    const { result } = renderHook(() => useUser('999'));

    await vi.waitFor(() => {
      expect(result.current.error).toBeTruthy();
    });
  });
});
```

### Module Mocking

```typescript
// Mock Next.js navigation
vi.mock('next/navigation', () => ({
  useRouter: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    prefetch: vi.fn(),
    back: vi.fn(),
  }),
  usePathname: () => '/',
  useSearchParams: () => new URLSearchParams(),
}));

// Mock Next.js Link
vi.mock('next/link', () => {
  return {
    default: ({ href, children, ...props }: any) => (
      <a href={href} {...props}>{children}</a>
    ),
  };
});

// Mock local storage
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] || null,
    setItem: (key: string, value: string) => { store[key] = value; },
    removeItem: (key: string) => { delete store[key]; },
    clear: () => { store = {}; },
  };
})();
Object.defineProperty(window, 'localStorage', { value: localStorageMock });
```

## 7. Testing Patterns

### AAA Pattern (Arrange, Act, Assert)

```typescript
it('calculates total price with tax', () => {
  // Arrange
  const items = [
    { name: 'Item 1', price: 10 },
    { name: 'Item 2', price: 20 },
  ];
  const taxRate = 0.1;

  // Act
  const total = calculateTotal(items, taxRate);

  // Assert
  expect(total).toBe(33); // (10 + 20) * 1.1
});
```

### Given-When-Then Pattern

```typescript
describe('Shopping Cart', () => {
  it('Given items in cart, When removing item, Then total updates', () => {
    // Given
    const cart = new ShoppingCart();
    cart.addItem({ id: '1', price: 10 });
    cart.addItem({ id: '2', price: 20 });

    // When
    cart.removeItem('1');

    // Then
    expect(cart.total).toBe(20);
    expect(cart.items).toHaveLength(1);
  });
});
```

### Snapshot Testing

```typescript
it('renders correctly', () => {
  const { container } = render(
    <Card title="Test Card">
      <p>Card content</p>
    </Card>
  );
  expect(container).toMatchSnapshot();
});

// Inline snapshots
it('formats user name', () => {
  expect(formatName({ first: 'John', last: 'Doe' })).toMatchInlineSnapshot(`"John Doe"`);
});
```

## 8. TDD Workflow

### Red-Green-Refactor

```typescript
// 1. RED — Write failing test
it('adds two numbers', () => {
  expect(add(1, 2)).toBe(3); // FAIL
});

// 2. GREEN — Write minimal implementation
function add(a: number, b: number): number {
  return a + b; // PASS
}

// 3. REFACTOR — Improve code
// (If needed, refactor while keeping tests green)
```

### TDD Example: Calculator

```typescript
// Step 1: Write tests first
describe('Calculator', () => {
  let calc: Calculator;

  beforeEach(() => {
    calc = new Calculator();
  });

  it('starts at 0', () => {
    expect(calc.value).toBe(0);
  });

  it('adds numbers', () => {
    calc.add(5);
    expect(calc.value).toBe(5);
  });

  it('subtracts numbers', () => {
    calc.add(10);
    calc.subtract(3);
    expect(calc.value).toBe(7);
  });

  it('multiplies numbers', () => {
    calc.add(4);
    calc.multiply(3);
    expect(calc.value).toBe(12);
  });

  it('resets to 0', () => {
    calc.add(5);
    calc.reset();
    expect(calc.value).toBe(0);
  });
});

// Step 2: Implement
class Calculator {
  private _value = 0;

  get value() { return this._value; }

  add(n: number) { this._value += n; }
  subtract(n: number) { this._value -= n; }
  multiply(n: number) { this._value *= n; }
  reset() { this._value = 0; }
}
```

## 9. CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/test.yml
name: Test

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'

      - run: npm ci

      - name: Lint
        run: npm run lint

      - name: Type Check
        run: npm run typecheck

      - name: Unit Tests
        run: npm run test:coverage

      - name: Upload Coverage
        uses: codecov/codecov-action@v3
        with:
          files: ./coverage/coverage-final.json

      - name: Build
        run: npm run build

      - name: E2E Tests
        run: npx playwright test

      - name: Upload Playwright Report
        uses: actions/upload-artifact@v3
        if: always()
        with:
          name: playwright-report
          path: playwright-report/
```

### Pre-commit Hooks

```json
// package.json
{
  "scripts": {
    "test": "vitest",
    "test:coverage": "vitest run --coverage",
    "test:e2e": "playwright test",
    "test:watch": "vitest --watch",
    "lint": "next lint",
    "typecheck": "tsc --noEmit"
  },
  "lint-staged": {
    "*.{ts,tsx}": [
      "eslint --fix",
      "vitest run --bail --findRelatedTests"
    ],
    "*.{ts,tsx,css,md}": "prettier --write"
  }
}
```

## 10. Test Coverage

### Coverage Configuration

```typescript
// vitest.config.ts
export default defineConfig({
  test: {
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html', 'lcov'],
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/**/*.test.{ts,tsx}',
        'src/**/*.spec.{ts,tsx}',
        'src/**/index.ts',
        'src/types/**',
      ],
      thresholds: {
        statements: 80,
        branches: 80,
        functions: 80,
        lines: 80,
      },
    },
  },
});
```

### Coverage Reports

```bash
# Generate coverage report
npm run test:coverage

# View HTML report
open coverage/index.html

# Check thresholds (CI)
npm run test:coverage -- --run
```

## Checklist

- [ ] Vitest configured with jsdom
- [ ] MSW setup for API mocking
- [ ] Unit tests for all components
- [ ] Unit tests for all hooks
- [ ] Unit tests for all utilities
- [ ] Integration tests for user flows
- [ ] E2E tests for critical paths
- [ ] Accessibility tests with axe-core
- [ ] Keyboard navigation tested
- [ ] Visual regression tests
- [ ] Error states tested
- [ ] Loading states tested
- [ ] Coverage thresholds enforced
- [ ] CI/CD pipeline configured
- [ ] Pre-commit hooks running tests
