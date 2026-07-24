---
name: frontend-patterns
description: Use when building React components, optimizing frontend performance, or implementing UI best practices
triggers:
  - react
  - component
  - frontend
  - ui
  - hook
  - state
  - render
  - performance
tags:
  - frontend
  - react
  - patterns
levels:
  0: "name + description"
  1: "detailed instructions"
  2: "reference code snippets"
---

# Frontend Patterns Skill

You are an expert in React component patterns and frontend performance optimization.

## Component Patterns

### Functional Component with Hooks
```tsx
function UserProfile({ userId }: { userId: string }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchUser(userId).then(setUser).finally(() => setLoading(false));
  }, [userId]);

  if (loading) return <Skeleton />;
  if (!user) return <NotFound />;
  return <Profile user={user} />;
}
```

### Custom Hook Pattern
```tsx
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState(value);

  useEffect(() => {
    const handler = setTimeout(() => setDebouncedValue(value), delay);
    return () => clearTimeout(handler);
  }, [value, delay]);

  return debouncedValue;
}
```

### Compound Component Pattern
```tsx
function Tabs({ children }: { children: React.ReactNode }) {
  const [activeTab, setActiveTab] = useState(0);
  return (
    <TabsContext.Provider value={{ activeTab, setActiveTab }}>
      <div className="tabs">{children}</div>
    </TabsContext.Provider>
  );
}

Tabs.Panel = function Panel({ index, children }: { index: number; children: React.ReactNode }) {
  const { activeTab } = useContext(TabsContext);
  return activeTab === index ? <div>{children}</div> : null;
};
```

## State Management

### Local State vs Global State
- Use local state for component-specific data
- Use context for shared UI state
- Use external store (Zustand, Redux) for complex global state

### Lifting State
```tsx
function Parent() {
  const [value, setValue] = useState('');
  return <Child value={value} onChange={setValue} />;
}
```

## Performance Patterns

### Memoization
```tsx
const expensiveValue = useMemo(() => computeExpensiveValue(a, b), [a, b]);

const MemoizedComponent = React.memo(ChildComponent, (prev, next) => {
  return prev.id === next.id;
});
```

### Code Splitting
```tsx
const HeavyComponent = React.lazy(() => import('./HeavyComponent'));

function App() {
  return (
    <Suspense fallback={<Loading />}>
      <HeavyComponent />
    </Suspense>
  );
}
```

### Virtualization for Lists
```tsx
import { FixedSizeList } from 'react-window';

function VirtualList({ items }: { items: Item[] }) {
  return (
    <FixedSizeList height={400} itemCount={items.length} itemSize={50}>
      {({ index, style }) => (
        <div style={style}>{items[index].name}</div>
      )}
    </FixedSizeList>
  );
}
```

## Error Handling

### Error Boundaries
```tsx
class ErrorBoundary extends React.Component<Props, State> {
  state = { hasError: false };

  static getDerivedStateFromError(error: Error) {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Error:', error, info);
  }

  render() {
    if (this.state.hasError) {
      return <FallbackComponent />;
    }
    return this.props.children;
  }
}
```

## Accessibility

### ARIA Attributes
```tsx
<button
  aria-label="Close dialog"
  aria-expanded={isOpen}
  aria-controls="dialog-content"
>
  <Icon name="close" />
</button>
```

### Keyboard Navigation
```tsx
function KeyboardNav() {
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleSelect();
    }
  };

  return (
    <div role="listbox" tabIndex={0} onKeyDown={handleKeyDown}>
      {/* options */}
    </div>
  );
}
```

## Testing Patterns

### Component Testing
```tsx
describe('Button', () => {
  it('renders with correct text', () => {
    render(<Button>Click me</Button>);
    expect(screen.getByText('Click me')).toBeInTheDocument();
  });

  it('calls onClick handler', async () => {
    const onClick = jest.fn();
    render(<Button onClick={onClick}>Click me</Button>);
    await userEvent.click(screen.getByRole('button'));
    expect(onClick).toHaveBeenCalled();
  });
});
```
