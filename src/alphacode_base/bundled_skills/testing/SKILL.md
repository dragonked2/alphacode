---
name: testing
description: Expert software testing — unit tests, integration tests, end-to-end tests, mocking, test design patterns, coverage strategy, and test architecture that catches real bugs without slowing development.
---

# Testing — AlphaCode Edition

You are a quality engineer who writes tests that catch real bugs, not tests that just increase coverage numbers. Every test has a purpose, every assertion matters, and every test suite is maintainable.

## Core Principles

1. **Test behavior, not implementation** — tests should survive refactors
2. **One assertion per test concept** — each test verifies one thing
3. **Fast and deterministic** — tests that are slow or flaky get ignored
4. **Test the edges** — null, empty, boundary values, errors
5. **Not too many, not too few** — test what matters, skip what's obvious

## 1. Test Types

### Unit Tests (Fast, Isolated)
```typescript
// Test a single function in isolation
describe('calculateDiscount', () => {
  it('returns 0 for orders under $100', () => {
    expect(calculateDiscount(50)).toBe(0);
  });

  it('returns 10% for orders between $100-$499', () => {
    expect(calculateDiscount(200)).toBe(20);
  });

  it('returns 20% for orders $500 and above', () => {
    expect(calculateDiscount(1000)).toBe(200);
  });

  it('throws for negative amounts', () => {
    expect(() => calculateDiscount(-10)).toThrow('Amount must be positive');
  });

  it('handles zero', () => {
    expect(calculateDiscount(0)).toBe(0);
  });
});
```

### Integration Tests (Multiple Components)
```typescript
// Test how components work together
describe('User Registration', () => {
  let db: Database;
  let mailer: MockMailer;

  beforeEach(async () => {
    db = await createTestDatabase();
    mailer = new MockMailer();
  });

  afterEach(async () => {
    await db.cleanup();
  });

  it('creates user and sends welcome email', async () => {
    const result = await registerUser(db, mailer, {
      email: 'test@example.com',
      password: 'securePassword123!',
    });

    expect(result.success).toBe(true);
    expect(result.user.email).toBe('test@example.com');
    expect(mailer.sent).toHaveLength(1);
    expect(mailer.sent[0].to).toBe('test@example.com');
  });

  it('rejects duplicate emails', async () => {
    await registerUser(db, mailer, { email: 'test@example.com', password: 'pass' });
    
    const result = await registerUser(db, mailer, { 
      email: 'test@example.com', 
      password: 'pass' 
    });
    
    expect(result.success).toBe(false);
    expect(result.error).toBe('Email already registered');
  });
});
```

### End-to-End Tests (Full Stack)
```typescript
// Test complete user flows
describe('Checkout Flow', () => {
  it('completes purchase as guest', async () => {
    // Add item to cart
    await page.goto('/products/widget');
    await page.click('[data-testid="add-to-cart"]');
    
    // Go to checkout
    await page.click('[data-testid="checkout"]');
    
    // Fill shipping info
    await page.fill('[data-testid="email"]', 'buyer@example.com');
    await page.fill('[data-testid="address"]', '123 Main St');
    
    // Complete purchase
    await page.click('[data-testid="place-order"]');
    
    // Verify confirmation
    await expect(page.locator('h1')).toContainText('Order Confirmed');
    await expect(page.locator('[data-testid="order-number"]')).toBeVisible();
  });
});
```

## 2. Test Design Patterns

### Arrange-Act-Assert (AAA)
```python
def test_user_creation():
    # Arrange
    db = create_test_db()
    user_data = {"name": "Alice", "email": "alice@test.com"}
    
    # Act
    user = create_user(db, user_data)
    
    # Assert
    assert user.name == "Alice"
    assert user.email == "alice@test.com"
    assert user.id is not None
```

### Given-When-Then (BDD)
```gherkin
Feature: User Login

  Scenario: Successful login with valid credentials
    Given a registered user with email "alice@example.com"
    When the user submits valid login credentials
    Then the user is redirected to the dashboard
    And a session token is created

  Scenario: Failed login with wrong password
    Given a registered user with email "alice@example.com"
    When the user submits incorrect password
    Then an error message "Invalid credentials" is shown
    And no session is created
```

### Table-Driven Tests
```go
func TestAdd(t *testing.T) {
    tests := []struct {
        name     string
        a, b     int
        expected int
    }{
        {"positive numbers", 2, 3, 5},
        {"negative numbers", -2, -3, -5},
        {"mixed signs", -2, 3, 1},
        {"with zero", 5, 0, 5},
        {"both zero", 0, 0, 0},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            result := Add(tt.a, tt.b)
            if result != tt.expected {
                t.Errorf("Add(%d, %d) = %d, want %d", 
                    tt.a, tt.b, result, tt.expected)
            }
        })
    }
}
```

### Snapshot Tests
```javascript
// React component snapshot test
it('renders correctly', () => {
    const tree = renderer.create(
        <UserCard name="Alice" role="admin" />
    ).toJSON();
    expect(tree).toMatchSnapshot();
});

// Update snapshots after intentional changes:
// npm test -- --updateSnapshot
```

## 3. Mocking

### When to Mock
- ✅ External services (APIs, databases, email)
- ✅ Time-dependent code
- ✅ Random number generators
- ✅ File system operations
- ❌ Internal business logic (test it directly)
- ❌ Data transformations (test with real logic)

### Mocking Examples
```python
# Mock external API
@patch('requests.get')
def test_fetch_user(mock_get):
    mock_get.return_value.json.return_value = {"name": "Alice"}
    
    user = fetch_user(123)
    assert user["name"] == "Alice"
    mock_get.assert_called_once_with("https://api.example.com/users/123")

# Mock database
@patch('database.query')
def test_get_orders(mock_query):
    mock_query.return_value = [{"id": 1, "total": 100}]
    
    orders = get_user_orders(123)
    assert len(orders) == 1
    assert orders[0]["total"] == 100
```

### Test Doubles
```
Dummy    — passed around but never used
Stub     — returns predefined values
Spy      — records how it was called
Mock     — pre-programmed with expectations
Fake     — working implementation for testing
```

## 4. Edge Cases to Always Test

```typescript
// Empty inputs
test('handles empty array', () => { expect(process([])).toEqual([]); });
test('handles empty string', () => { expect(validate('')).toBe(false); });
test('handles null input', () => { expect(parse(null)).toBe(null); });
test('handles undefined input', () => { expect(parse(undefined)).toBe(null); });

// Boundary values
test('handles zero', () => { expect(divide(10, 0)).toBe(Infinity); });
test('handles negative numbers', () => { expect(abs(-5)).toBe(5); });
test('handles MAX_SAFE_INTEGER', () => { /* ... */ });
test('handles empty string as name', () => { /* ... */ });

// Unicode and special characters
test('handles unicode in name', () => { expect(normalize('José')).toBe('jose'); });
test('handles emoji in input', () => { expect(sanitize('👍')).toBe('👍'); });
test('handles RTL text', () => { /* ... */ });

// Concurrent access
test('handles concurrent updates', async () => {
    const results = await Promise.all([
        updateUser(1, { name: 'Alice' }),
        updateUser(1, { name: 'Bob' }),
    ]);
    // Verify no race condition corruption
});
```

## 5. Test Architecture

### Test File Organization
```
src/
├── services/
│   ├── user.service.ts
│   └── user.service.test.ts      # co-located unit tests
├── api/
│   ├── user.controller.ts
│   └── user.controller.test.ts   # co-located unit tests
tests/
├── integration/
│   ├── user-registration.test.ts  # integration tests
│   └── payment.test.ts
├── e2e/
│   ├── checkout.test.ts           # end-to-end tests
│   └── auth.test.ts
└── fixtures/
    ├── users.json                 # test data
    └── orders.json
```

### Test Naming Convention
```typescript
// Pattern: <what> <condition> <expected>
describe('UserService', () => {
    describe('createUser', () => {
        it('returns new user when given valid data', () => { /* ... */ });
        it('throws ValidationError when email is invalid', () => { /* ... */ });
        it('throws ConflictError when email already exists', () => { /* ... */ });
        it('hashes password before storing', () => { /* ... */ });
    });
});
```

### Shared Test Utilities
```python
# conftest.py (pytest)
@pytest.fixture
def test_db():
    db = create_test_database()
    yield db
    db.cleanup()

@pytest.fixture
def sample_user(test_db):
    return create_user(test_db, {
        "name": "Test User",
        "email": "test@example.com"
    })

# Usage in tests
def test_update_user(test_db, sample_user):
    updated = update_user(test_db, sample_user.id, {"name": "New Name"})
    assert updated.name == "New Name"
```

## 6. Coverage Strategy

### What to Test (Priority Order)
1. **Critical paths** — payment, auth, data integrity
2. **Edge cases** — null, empty, boundary values
3. **Error handling** — exceptions, retries, timeouts
4. **Business logic** — rules, calculations, validations
5. **UI components** — user interactions, state changes
6. **Utilities** — pure functions, helpers

### What NOT to Test
- ❌ Third-party library internals
- ❌ Trivial getters/setters
- ❌ Framework boilerplate
- ❌ Implementation details (private methods)
- ❌ Configuration (unless it affects behavior)

### Coverage Targets
```
Line coverage:    80%+ (aim for, don't chase 100%)
Branch coverage:  75%+ (more meaningful than line coverage)
Critical paths:   100% (payment, auth, data mutations)
```

## 7. Anti-Patterns to Avoid

### Tests That Test Implementation
```python
# ❌ Brittle — breaks when implementation changes
mock_db.query.assert_called_once_with(
    "SELECT * FROM users WHERE id = ?", (123,)
)

# ✅ Robust — tests behavior, survives refactors
user = get_user(123)
assert user.name == "Alice"
```

### Shared Mutable State
```python
# ❌ Tests depend on execution order
user = None

def test_create_user():
    global user
    user = create_user({"name": "Alice"})

def test_update_user():
    user["name"] = "Bob"  # depends on test_create_user running first
```

### Testing Too Much in One Test
```python
# ❌ One test doing too many things
def test_everything():
    user = create_user()
    order = create_order(user)
    payment = process_payment(order)
    send_email(user, payment)
    update_inventory(order)
    # If this fails, which part broke?
```

### Flaky Tests
```python
# ❌ Time-dependent — fails at midnight
def test_token_expiry():
    token = create_token(expires_in=1)  # 1 second
    time.sleep(2)
    assert validate_token(token) is False

# ✅ Mock time
@patch('time.time')
def test_token_expiry(mock_time):
    mock_time.return_value = 1000
    token = create_token(expires_in=3600)
    mock_time.return_value = 5000  # 4000 seconds later
    assert validate_token(token) is False
```

## 8. Test Checklist

- [ ] Test is named clearly (what, condition, expected)
- [ ] Test has one clear purpose
- [ ] Test is deterministic (no random, no timing)
- [ ] Test is isolated (no shared state)
- [ ] Test covers happy path AND error path
- [ ] Test covers edge cases (null, empty, boundary)
- [ ] Test runs fast (< 1 second for unit tests)
- [ ] Test doesn't depend on external services
- [ ] Test failure message is helpful
- [ ] Test is maintainable (not brittle to refactors)
