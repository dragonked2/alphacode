---
name: components
description: Component systems: atomic design, compound components, headless UI, render props, composition, polymorphic components, design tokens, Storybook, accessibility, and component testing.
---

# Component Systems Skill

## Core Principles

1. **Atomic design** — Build from atoms up to pages
2. **Single responsibility** — Each component does one thing well
3. **Composition over configuration** — Small pieces composed together
4. **Accessible by default** — ARIA, keyboard nav, focus management
5. **Typed props** — Explicit interfaces, no implicit any

## 1. Atomic Design Methodology

### Hierarchy

```
Atoms → Molecules → Organisms → Templates → Pages

Atoms:      Button, Input, Badge, Icon, Avatar, Typography
Molecules:  SearchBar, FormField, Card, Modal, Dropdown
Organisms:  Header, Sidebar, DataTable, HeroSection, Footer
Templates:  DashboardLayout, AuthLayout, LandingLayout
Pages:      Actual route implementations using templates
```

### Atom: Button

```typescript
import { forwardRef, type ButtonHTMLAttributes } from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

const buttonVariants = cva(
  'inline-flex items-center justify-center rounded-lg font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        primary: 'bg-primary-500 text-white hover:bg-primary-600 focus:ring-primary-500',
        secondary: 'bg-gray-100 text-gray-900 hover:bg-gray-200 focus:ring-gray-500 dark:bg-gray-800 dark:text-gray-100 dark:hover:bg-gray-700',
        ghost: 'text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800',
        danger: 'bg-red-500 text-white hover:bg-red-600 focus:ring-red-500',
      },
      size: {
        sm: 'h-8 px-3 text-sm',
        md: 'h-10 px-4 text-sm',
        lg: 'h-12 px-6 text-base',
      },
    },
    defaultVariants: {
      variant: 'primary',
      size: 'md',
    },
  }
);

interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  isLoading?: boolean;
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, isLoading, children, disabled, ...props }, ref) => {
    return (
      <button
        className={buttonVariants({ variant, size, className })}
        ref={ref}
        disabled={disabled || isLoading}
        {...props}
      >
        {isLoading && (
          <svg className="mr-2 h-4 w-4 animate-spin" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
        )}
        {children}
      </button>
    );
  }
);

Button.displayName = 'Button';
export { Button, buttonVariants };
```

### Molecule: FormField

```typescript
import { forwardRef, type InputHTMLAttributes } from 'react';

interface FormFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
  error?: string;
  hint?: string;
  required?: boolean;
}

const FormField = forwardRef<HTMLInputElement, FormFieldProps>(
  ({ label, error, hint, required, id, ...props }, ref) => {
    const inputId = id || label.toLowerCase().replace(/\s+/g, '-');

    return (
      <div className="space-y-1">
        <label
          htmlFor={inputId}
          className="block text-sm font-medium text-gray-700 dark:text-gray-300"
        >
          {label}
          {required && <span className="ml-1 text-red-500">*</span>}
        </label>
        <input
          ref={ref}
          id={inputId}
          aria-invalid={!!error}
          aria-describedby={error ? `${inputId}-error` : hint ? `${inputId}-hint` : undefined}
          className={`
            block w-full rounded-lg border px-3 py-2 shadow-sm
            focus:outline-none focus:ring-2
            disabled:cursor-not-allowed disabled:opacity-50
            ${error
              ? 'border-red-500 focus:border-red-500 focus:ring-red-500/20'
              : 'border-gray-300 focus:border-primary-500 focus:ring-primary-500/20 dark:border-gray-700'
            }
            dark:bg-gray-800 dark:text-white
          `}
          {...props}
        />
        {error && (
          <p id={`${inputId}-error`} className="text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}
        {hint && !error && (
          <p id={`${inputId}-hint`} className="text-sm text-gray-500">
            {hint}
          </p>
        )}
      </div>
    );
  }
);

FormField.displayName = 'FormField';
export { FormField };
```

### Organism: DataTable

```typescript
'use client';
import { useState, useMemo } from 'react';

interface Column<T> {
  key: keyof T;
  header: string;
  sortable?: boolean;
  render?: (value: T[keyof T], row: T) => React.ReactNode;
}

interface DataTableProps<T> {
  data: T[];
  columns: Column<T>[];
  keyExtractor: (row: T) => string;
  onRowClick?: (row: T) => void;
}

function DataTable<T>({ data, columns, keyExtractor, onRowClick }: DataTableProps<T>) {
  const [sortKey, setSortKey] = useState<keyof T | null>(null);
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('asc');

  const sortedData = useMemo(() => {
    if (!sortKey) return data;
    return [...data].sort((a, b) => {
      const aVal = a[sortKey];
      const bVal = b[sortKey];
      const comparison = String(aVal).localeCompare(String(bVal));
      return sortDirection === 'asc' ? comparison : -comparison;
    });
  }, [data, sortKey, sortDirection]);

  function handleSort(key: keyof T) {
    if (sortKey === key) {
      setSortDirection(d => d === 'asc' ? 'desc' : 'asc');
    } else {
      setSortKey(key);
      setSortDirection('asc');
    }
  }

  return (
    <div className="overflow-x-auto rounded-lg border border-gray-200 dark:border-gray-800">
      <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-800">
        <thead className="bg-gray-50 dark:bg-gray-900">
          <tr>
            {columns.map(col => (
              <th
                key={String(col.key)}
                className={`px-4 py-3 text-left text-sm font-medium text-gray-700 dark:text-gray-300 ${col.sortable ? 'cursor-pointer select-none hover:bg-gray-100 dark:hover:bg-gray-800' : ''}`}
                onClick={() => col.sortable && handleSort(col.key)}
              >
                <span className="flex items-center gap-1">
                  {col.header}
                  {sortKey === col.key && (
                    <span>{sortDirection === 'asc' ? '↑' : '↓'}</span>
                  )}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-200 dark:divide-gray-800">
          {sortedData.map(row => (
            <tr
              key={keyExtractor(row)}
              onClick={() => onRowClick?.(row)}
              className={`${onRowClick ? 'cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-900' : ''}`}
            >
              {columns.map(col => (
                <td key={String(col.key)} className="px-4 py-3 text-sm text-gray-900 dark:text-gray-100">
                  {col.render ? col.render(row[col.key], row) : String(row[col.key])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

## 2. Compound Components Pattern

### Select Component

```typescript
import { createContext, useContext, useState, type ReactNode } from 'react';

interface SelectContextType {
  value: string;
  onChange: (value: string) => void;
  isOpen: boolean;
  setIsOpen: (open: boolean) => void;
}

const SelectContext = createContext<SelectContextType | null>(null);

function useSelect() {
  const context = useContext(SelectContext);
  if (!context) throw new Error('Select compound components must be used within Select');
  return context;
}

// Root
function Select({ children, value, onChange }: { children: ReactNode; value: string; onChange: (v: string) => void }) {
  const [isOpen, setIsOpen] = useState(false);
  return (
    <SelectContext.Provider value={{ value, onChange, isOpen, setIsOpen }}>
      <div className="relative">{children}</div>
    </SelectContext.Provider>
  );
}

// Trigger
function SelectTrigger({ children }: { children: ReactNode }) {
  const { isOpen, setIsOpen } = useSelect();
  return (
    <button
      type="button"
      onClick={() => setIsOpen(!isOpen)}
      className="flex w-full items-center justify-between rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-800"
    >
      {children}
      <span className={`transition-transform ${isOpen ? 'rotate-180' : ''}`}>▼</span>
    </button>
  );
}

// Value display
function SelectValue({ placeholder }: { placeholder?: string }) {
  const { value } = useSelect();
  return <span>{value || placeholder}</span>;
}

// Options container
function SelectContent({ children }: { children: ReactNode }) {
  const { isOpen } = useSelect();
  if (!isOpen) return null;
  return (
    <div className="absolute z-50 mt-1 w-full rounded-lg border bg-white shadow-lg dark:border-gray-700 dark:bg-gray-800">
      {children}
    </div>
  );
}

// Individual option
function SelectItem({ value, children }: { value: string; children: ReactNode }) {
  const { value: selectedValue, onChange, setIsOpen } = useSelect();
  return (
    <button
      type="button"
      onClick={() => { onChange(value); setIsOpen(false); }}
      className={`flex w-full items-center px-3 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-700 ${selectedValue === value ? 'bg-primary-50 text-primary-600 dark:bg-primary-900/20' : ''}`}
    >
      {children}
    </button>
  );
}

// Attach compound components
Object.assign(Select, { Trigger, Value, Content, Item });

// Usage
<Select value={selected} onChange={setSelected}>
  <Select.Trigger>
    <Select.Value placeholder="Select an option" />
  </Select.Trigger>
  <Select.Content>
    <Select.Item value="apple">Apple</Select.Item>
    <Select.Item value="banana">Banana</Select.Item>
    <Select.Item value="cherry">Cherry</Select.Item>
  </Select.Content>
</Select>
```

## 3. Headless UI Patterns

### Headless Toggle

```typescript
import { useToggle } from '@headlessui/react';

function useSwitch(initial = false) {
  const [enabled, setEnabled] = useState(initial);
  return {
    enabled,
    toggle: () => setEnabled(e => !e),
    on: () => setEnabled(true),
    off: () => setEnabled(false),
  };
}

function Switch({ label, checked, onChange }: SwitchProps) {
  return (
    <label className="flex items-center gap-2">
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`
          relative inline-flex h-6 w-11 items-center rounded-full transition-colors
          ${checked ? 'bg-primary-500' : 'bg-gray-200 dark:bg-gray-700'}
        `}
      >
        <span
          className={`
            inline-block h-4 w-4 transform rounded-full bg-white transition-transform
            ${checked ? 'translate-x-6' : 'translate-x-1'}
          `}
        />
      </button>
      <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{label}</span>
    </label>
  );
}
```

### Headless Disclosure

```typescript
function Disclosure({ title, children }: { title: string; children: ReactNode }) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="border rounded-lg">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="flex w-full items-center justify-between px-4 py-3 text-left font-medium"
        aria-expanded={isOpen}
      >
        {title}
        <span className={`transition-transform ${isOpen ? 'rotate-180' : ''}`}>▼</span>
      </button>
      {isOpen && <div className="px-4 pb-4">{children}</div>}
    </div>
  );
}
```

## 4. Polymorphic Components

### The `as` Prop

```typescript
import { forwardRef, type ElementType, type ComponentPropsWithoutRef } from 'react';

type PolymorphicProps<C extends ElementType> = {
  as?: C;
} & ComponentPropsWithoutRef<C>;

const Text = forwardRef(function Text<C extends ElementType = 'p'>(
  { as, className, children, ...props }: PolymorphicProps<C>,
  ref: React.Ref<Element>
) {
  const Component = as || 'p';
  return (
    <Component ref={ref} className={className} {...props}>
      {children}
    </Component>
  );
});

// Usage
<Text as="p" className="text-gray-600">Paragraph</Text>
<Text as="h1" className="text-2xl font-bold">Heading</Text>
<Text as="a" href="/about" className="text-primary-500 hover:underline">Link</Text>
<Text as="span" className="text-sm text-gray-500">Span</Text>
```

### Advanced Polymorphic with Link

```typescript
import Link from 'next/link';

type LinkProps<C extends ElementType> = {
  as?: C;
  href: string;
} & Omit<ComponentPropsWithoutRef<C>, 'href'>;

function StyledLink<C extends ElementType = typeof Link>({
  as,
  href,
  className,
  children,
  ...props
}: LinkProps<C>) {
  const Component = as || Link;
  return (
    <Component
      href={href}
      className={`
        text-primary-500 underline-offset-4 hover:underline
        focus:outline-none focus:ring-2 focus:ring-primary-500 focus:ring-offset-2
        ${className || ''}
      `}
      {...props}
    >
      {children}
    </Component>
  );
}
```

## 5. Design Tokens Integration

### CSS Variables as Tokens

```css
:root {
  /* Colors */
  --color-primary: oklch(0.5 0.12 250);
  --color-primary-hover: oklch(0.45 0.14 250);
  --color-background: oklch(0.98 0.01 250);
  --color-foreground: oklch(0.15 0.05 250);

  /* Typography */
  --font-sans: 'Inter', system-ui, sans-serif;
  --font-mono: 'JetBrains Mono', monospace;

  /* Spacing */
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;

  /* Border radius */
  --radius-sm: 0.25rem;
  --radius-md: 0.5rem;
  --radius-lg: 0.75rem;

  /* Shadows */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.1);
}

.dark {
  --color-background: oklch(0.15 0.05 250);
  --color-foreground: oklch(0.98 0.01 250);
}
```

### Token Usage in Components

```typescript
const Button = styled.button`
  background-color: var(--color-primary);
  color: white;
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-md);

  &:hover {
    background-color: var(--color-primary-hover);
  }
`;
```

## 6. Accessibility (ARIA)

### Keyboard Navigation

```typescript
function Tabs({ tabs, defaultTab }: TabsProps) {
  const [activeTab, setActiveTab] = useState(defaultTab);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);

  function handleKeyDown(e: React.KeyboardEvent, index: number) {
    const tabCount = tabs.length;
    let newIndex = index;

    switch (e.key) {
      case 'ArrowRight':
        newIndex = (index + 1) % tabCount;
        break;
      case 'ArrowLeft':
        newIndex = (index - 1 + tabCount) % tabCount;
        break;
      case 'Home':
        newIndex = 0;
        break;
      case 'End':
        newIndex = tabCount - 1;
        break;
      default:
        return;
    }

    e.preventDefault();
    setActiveTab(tabs[newIndex].id);
    tabRefs.current[newIndex]?.focus();
  }

  return (
    <div>
      <div role="tablist" className="flex border-b border-gray-200 dark:border-gray-800">
        {tabs.map((tab, index) => (
          <button
            key={tab.id}
            ref={el => { tabRefs.current[index] = el; }}
            role="tab"
            aria-selected={activeTab === tab.id}
            aria-controls={`panel-${tab.id}`}
            tabIndex={activeTab === tab.id ? 0 : -1}
            onKeyDown={e => handleKeyDown(e, index)}
            onClick={() => setActiveTab(tab.id)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === tab.id
                ? 'border-primary-500 text-primary-600'
                : 'border-transparent text-gray-500 hover:text-gray-700'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {tabs.map(tab => (
        <div
          key={tab.id}
          role="tabpanel"
          id={`panel-${tab.id}`}
          aria-labelledby={`tab-${tab.id}`}
          hidden={activeTab !== tab.id}
          className="p-4"
        >
          {tab.content}
        </div>
      ))}
    </div>
  );
}
```

### Focus Management

```typescript
function Modal({ isOpen, onClose, children }: ModalProps) {
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (isOpen) {
      previousFocusRef.current = document.activeElement as HTMLElement;
      closeButtonRef.current?.focus();
    } else {
      previousFocusRef.current?.focus();
    }
  }, [isOpen]);

  // Trap focus within modal
  useEffect(() => {
    if (!isOpen) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Tab') return;

      const modal = document.querySelector('[role="dialog"]');
      if (!modal) return;

      const focusable = modal.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        className="relative z-10 w-full max-w-md rounded-xl bg-white p-6 shadow-xl dark:bg-gray-900"
      >
        <button
          ref={closeButtonRef}
          onClick={onClose}
          className="absolute right-4 top-4 rounded-lg p-1 hover:bg-gray-100 dark:hover:bg-gray-800"
          aria-label="Close"
        >
          ✕
        </button>
        {children}
      </div>
    </div>
  );
}
```

## 7. Component Testing

### Unit Test with Vitest + Testing Library

```typescript
// components/Button.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { Button } from './Button';

describe('Button', () => {
  it('renders with text', () => {
    render(<Button>Click me</Button>);
    expect(screen.getByRole('button', { name: /click me/i })).toBeInTheDocument();
  });

  it('calls onClick when clicked', () => {
    const handleClick = vi.fn();
    render(<Button onClick={handleClick}>Click</Button>);
    fireEvent.click(screen.getByRole('button'));
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it('is disabled when disabled prop is true', () => {
    render(<Button disabled>Click</Button>);
    expect(screen.getByRole('button')).toBeDisabled();
  });

  it('shows loading spinner when isLoading', () => {
    render(<Button isLoading>Submit</Button>);
    expect(screen.getByText(/submit/i)).toBeInTheDocument();
    expect(screen.getByRole('button')).toBeDisabled();
  });

  it('applies variant styles', () => {
    const { rerender } = render(<Button variant="primary">Test</Button>);
    expect(screen.getByRole('button')).toHaveClass('bg-primary-500');

    rerender(<Button variant="secondary">Test</Button>);
    expect(screen.getByRole('button')).toHaveClass('bg-gray-100');
  });
});
```

### Integration Test

```typescript
// components/FormField.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { FormField } from './FormField';

describe('FormField', () => {
  it('renders label and input', () => {
    render(<FormField label="Email" />);
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
  });

  it('shows error message when error prop provided', () => {
    render(<FormField label="Email" error="Email is required" />);
    expect(screen.getByText(/email is required/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/email/i)).toHaveAttribute('aria-invalid', 'true');
  });

  it('shows hint when no error', () => {
    render(<FormField label="Email" hint="We'll never share your email" />);
    expect(screen.getByText(/we'll never share/i)).toBeInTheDocument();
  });

  it('connects error to input via aria-describedby', () => {
    render(<FormField label="Email" error="Invalid email" />);
    const input = screen.getByLabelText(/email/i);
    const error = screen.getByText(/invalid email/i);
    expect(input).toHaveAttribute('aria-describedby', error.id);
  });
});
```

## Storybook Integration

### Story Configuration

```typescript
// components/Button.stories.tsx
import type { Meta, StoryObj } from '@storybook/react';
import { Button } from './Button';

const meta: Meta<typeof Button> = {
  title: 'Components/Button',
  component: Button,
  tags: ['autodocs'],
  argTypes: {
    variant: {
      control: 'select',
      options: ['primary', 'secondary', 'ghost', 'danger'],
    },
    size: {
      control: 'select',
      options: ['sm', 'md', 'lg'],
    },
  },
};

export default meta;
type Story = StoryObj<typeof Button>;

export const Primary: Story = {
  args: {
    children: 'Primary Button',
    variant: 'primary',
  },
};

export const Secondary: Story = {
  args: {
    children: 'Secondary Button',
    variant: 'secondary',
  },
};

export const Loading: Story = {
  args: {
    children: 'Submit',
    isLoading: true,
  },
};

export const Disabled: Story = {
  args: {
    children: 'Disabled',
    disabled: true,
  },
};
```

## Checklist

- [ ] Components follow atomic design
- [ ] Single responsibility per component
- [ ] TypeScript props interface defined
- [ ] Accessible by default (ARIA, keyboard)
- [ ] Focus management implemented
- [ ] Loading/error states handled
- [ ] Compound components for complex UI
- [ ] Polymorphic components where needed
- [ ] Design tokens integrated
- [ ] Storybook stories written
- [ ] Unit tests for all components
- [ ] Integration tests for user flows
