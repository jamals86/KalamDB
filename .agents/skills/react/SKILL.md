# React Best Practices for KalamDB

This skill contains comprehensive React performance optimization guidelines adapted from [Vercel's React Best Practices](https://vercel.com/blog/introducing-react-best-practices). Apply these patterns when building, reviewing, or refactoring React code in the KalamDB UI.

## When to Apply

Reference these guidelines when:
- Writing new React components for the KalamDB admin UI
- Implementing data fetching (client or server-side)
- Reviewing code for performance issues
- Refactoring existing React code in the UI
- Optimizing bundle size or load times

## Rule Categories by Priority

| Priority | Category | Impact | Description |
|----------|----------|--------|-------------|
| 1 | Eliminating Waterfalls | CRITICAL | Avoid sequential async operations that should run in parallel |
| 2 | Bundle Size Optimization | CRITICAL | Minimize JavaScript bundle size and lazy-load heavy components |
| 3 | Server-Side Performance | HIGH | Optimize server-rendered content and API response times |
| 4 | Client-Side Data Fetching | MEDIUM-HIGH | Efficient data fetching patterns (SWR, React Query, suspense) |
| 5 | Re-render Optimization | MEDIUM | Prevent unnecessary component re-renders |
| 6 | Rendering Performance | MEDIUM | Optimize DOM rendering and layout operations |
| 7 | JavaScript Performance | LOW-MEDIUM | Micro-optimizations for hot paths |
| 8 | Advanced Patterns | LOW | Advanced patterns for specific edge cases |

## 1. Eliminating Waterfalls (CRITICAL)

Waterfalls are the #1 performance killer. Each sequential await adds full network latency. Eliminating them yields the largest gains.

### 1.1 Defer Await Until Needed

Move `await` statements into branches where they're actually used, not before.

**Incorrect (blocks both branches):**
```typescript
async function handleRequest(userId: string, skipProcessing: boolean) {
  const userData = await fetchUserData(userId)
  
  if (skipProcessing) {
    // Data loaded but not used
    return { skipped: true }
  }
  
  return processUserData(userData)
}
```

**Correct (only blocks when needed):**
```typescript
async function handleRequest(userId: string, skipProcessing: boolean) {
  if (skipProcessing) {
    return { skipped: true }
  }
  
  const userData = await fetchUserData(userId)
  return processUserData(userData)
}
```

### 1.2 Dependency-Based Parallelization

When fetching multiple data sources, start promises early and await them together.

**Incorrect (sequential):**
```typescript
const user = await fetchUser(id)
const posts = await fetchPosts(user.id)
const comments = await fetchComments(posts[0].id)
```

**Correct (parallel when independent):**
```typescript
const userPromise = fetchUser(id)
const user = await userPromise

// These don't depend on each other
const [posts, profile] = await Promise.all([
  fetchPosts(user.id),
  fetchProfile(user.id)
])

// Only this depends on posts
const comments = await fetchComments(posts[0].id)
```

### 1.3 Promise.all() for Independent Operations

Use `Promise.all()` to run multiple operations in parallel instead of sequentially.

**Incorrect:**
```typescript
const metadata = await fetchMetadata()
const analytics = await fetchAnalytics()
const settings = await fetchSettings()
```

**Correct:**
```typescript
const [metadata, analytics, settings] = await Promise.all([
  fetchMetadata(),
  fetchAnalytics(),
  fetchSettings()
])
```

### 1.4 Strategic Suspense Boundaries

Use Suspense to stream content as data arrives, don't wait for all queries.

**Incorrect (blocking):**
```tsx
async function Page() {
  const data1 = await heavyQuery1()
  const data2 = await heavyQuery2()
  return <Component data1={data1} data2={data2} />
}
```

**Correct (streaming):**
```tsx
function Page() {
  return (
    <>
      <Suspense fallback={<Skeleton />}>
        <Component1 promise={heavyQuery1()} />
      </Suspense>
      <Suspense fallback={<Skeleton />}>
        <Component2 promise={heavyQuery2()} />
      </Suspense>
    </>
  )
}
```

## 2. Bundle Size Optimization (CRITICAL)

Large bundles slow down initial load. Every KB matters for time-to-interactive.

### 2.1 Avoid Barrel File Imports

Import directly from modules instead of re-exporting through barrel files, allowing tree-shaking.

**Incorrect (prevents tree-shaking):**
```typescript
import { Button, Modal, Input } from '@/components'
```

**Correct (specific imports):**
```typescript
import { Button } from '@/components/button'
import { Modal } from '@/components/modal'
import { Input } from '@/components/input'
```

### 2.2 Lazy Load Heavy Components

Use `React.lazy()` and `next/dynamic` for components you don't need immediately.

**Incorrect (loads with main bundle):**
```tsx
import { ChartComponent } from '@/components/charts'

export default function Dashboard() {
  return <ChartComponent data={data} />
}
```

**Correct (code-split):**
```tsx
import dynamic from 'next/dynamic'

const ChartComponent = dynamic(
  () => import('@/components/charts').then(m => m.ChartComponent),
  { loading: () => <p>Loading...</p> }
)

export default function Dashboard() {
  return <ChartComponent data={data} />
}
```

### 2.3 Conditional Module Loading

Load modules only when a feature is actually activated.

**Incorrect:**
```typescript
import { analytics } from '@/lib/analytics'

// Only used if user has premium
const trackPremiumEvent = () => analytics.track('premium_event')
```

**Correct:**
```typescript
const trackPremiumEvent = async () => {
  const { analytics } = await import('@/lib/analytics')
  analytics.track('premium_event')
}
```

### 2.4 Defer Third-Party Scripts

Load analytics, tracking, and non-critical libraries after hydration.

**Incorrect:**
```tsx
import { Analytics } from '@/lib/analytics'

export function RootLayout() {
  return (
    <>
      <body>...</body>
      <Analytics />
    </>
  )
}
```

**Correct:**
```tsx
import { useEffect } from 'react'

export function RootLayout() {
  useEffect(() => {
    // Load after hydration
    import('@/lib/analytics').then(m => m.init())
  }, [])
  
  return <body>...</body>
}
```

## 3. Server-Side Performance (HIGH)

Optimize what runs on the server to improve time-to-first-byte and reduce client work.

### 3.1 RSC (React Server Components) for Data Fetching

Fetch data on the server, not in browser JavaScript.

**Incorrect (client-side fetch):**
```tsx
'use client'

export default function UserList() {
  const [users, setUsers] = useState([])
  
  useEffect(() => {
    fetch('/api/users').then(r => r.json()).then(setUsers)
  }, [])
  
  return <div>{users.map(u => <User key={u.id} user={u} />)}</div>
}
```

**Correct (server-side fetch):**
```tsx
// Server Component by default
async function UserList() {
  const users = await db.users.findAll()
  return <div>{users.map(u => <User key={u.id} user={u} />)}</div>
}
```

### 3.2 Stream Content Early

Start rendering as soon as possible, fetch missing pieces in parallel.

**Incorrect (blocking):**
```tsx
async function Page() {
  const criticalData = await fetchCritical()
  const nonCriticalData = await fetchNonCritical()
  return <Layout critical={criticalData} extra={nonCriticalData} />
}
```

**Correct (stream critical, load rest in parallel):**
```tsx
async function Page() {
  const criticalData = await fetchCritical()
  return (
    <Layout critical={criticalData}>
      <Suspense>
        <Extra promise={fetchNonCritical()} />
      </Suspense>
    </Layout>
  )
}
```

## 4. Client-Side Data Fetching (MEDIUM-HIGH)

When fetching on the client, use deduplication, caching, and background updates.

### 4.1 Use Deduplication & Caching

Avoid duplicate requests for the same data.

**Incorrect (duplicated fetches):**
```tsx
function UserProfile() {
  const [user, setUser] = useState(null)
  
  useEffect(() => {
    fetch(`/api/user/${id}`).then(r => r.json()).then(setUser)
  }, [id])
  
  return <div>{user?.name}</div>
}

function UserSettings() {
  const [user, setUser] = useState(null)
  
  useEffect(() => {
    fetch(`/api/user/${id}`).then(r => r.json()).then(setUser)
  }, [id])
  
  return <div>{user?.email}</div>
}
```

**Correct (deduplicated with SWR/React Query):**
```tsx
function UserProfile() {
  const { data: user } = useSWR(`/api/user/${id}`)
  return <div>{user?.name}</div>
}

function UserSettings() {
  const { data: user } = useSWR(`/api/user/${id}`) // Same request, cached
  return <div>{user?.email}</div>
}
```

### 4.2 Fetch on User Interaction

Don't fetch data until the user needs it.

**Incorrect (eager fetching):**
```tsx
function Modal() {
  const [details, setDetails] = useState(null)
  
  useEffect(() => {
    // Fetches even if modal never opens
    fetchDetails(itemId).then(setDetails)
  }, [itemId])
  
  return <div>{details?.content}</div>
}
```

**Correct (lazy fetch on open):**
```tsx
function Modal({ isOpen, itemId }) {
  const { data: details } = useSWR(
    isOpen ? `/api/details/${itemId}` : null
  )
  
  return <div>{details?.content}</div>
}
```

## 5. Re-render Optimization (MEDIUM)

Prevent unnecessary re-renders that slow down the app.

### 5.1 Use Primitive Dependencies in Effects

Specify primitive values instead of objects in effect dependencies.

**Incorrect (re-runs on any user field):**
```typescript
useEffect(() => {
  console.log(user.id)
}, [user]) // Re-runs if ANY field changes
```

**Correct (re-runs only when id changes):**
```typescript
useEffect(() => {
  console.log(user.id)
}, [user.id]) // Re-runs only if id changes
```

### 5.2 Extract Expensive Work to Memoized Components

Split large components into smaller memoized pieces.

**Incorrect (whole component re-renders):**
```tsx
function Dashboard({ user, expensiveData }) {
  return (
    <div>
      <Header user={user} />
      <ExpensiveChart data={expensiveData} /> {/* Re-renders even if data unchanged */}
    </div>
  )
}
```

**Correct (memoized expensive component):**
```tsx
const MemoizedChart = memo(function ExpensiveChart({ data }) {
  // Only re-renders if data reference changes
  return <svg>{/* expensive rendering */}</svg>
})

function Dashboard({ user, expensiveData }) {
  return (
    <div>
      <Header user={user} />
      <MemoizedChart data={expensiveData} />
    </div>
  )
}
```

### 5.3 Avoid useMemo for Simple Expressions

Don't wrap simple primitive values in useMemo—the overhead isn't worth it.

**Incorrect (useless memoization):**
```typescript
const isActive = useMemo(() => status === 'active', [status])
const count = useMemo(() => items.length, [items])
```

**Correct (compute directly):**
```typescript
const isActive = status === 'active'
const count = items.length
```

### 5.4 Use Functional setState for Stable Callbacks

When setState depends on previous state, use functional updates.

**Incorrect (recreates callback on each render):**
```tsx
const [count, setCount] = useState(0)

// Callback recreated every render
const increment = () => setCount(count + 1)

return <button onClick={increment}>+</button>
```

**Correct (stable callback):**
```tsx
const [count, setCount] = useState(0)

// Callback never changes
const increment = () => setCount(c => c + 1)

return <button onClick={increment}>+</button>
```

### 5.5 Lazy State Initialization

For expensive initial state, pass a function to useState.

**Incorrect (parse on every render):**
```typescript
const [config, setConfig] = useState(JSON.parse(localStorage.getItem('config')))
```

**Correct (parse only on mount):**
```typescript
const [config, setConfig] = useState(() => 
  JSON.parse(localStorage.getItem('config') || '{}')
)
```

## 6. Rendering Performance (MEDIUM)

Optimize how components render to the DOM.

### 6.1 Use content-visibility for Long Lists

CSS property that skips rendering off-screen content.

**Before:**
```tsx
function LongList({ items }) {
  return (
    <div>
      {items.map(item => <Item key={item.id} item={item} />)}
    </div>
  )
}
```

**After:**
```tsx
function LongList({ items }) {
  return (
    <div>
      {items.map(item => (
        <div key={item.id} style={{ contentVisibility: 'auto' }}>
          <Item item={item} />
        </div>
      ))}
    </div>
  )
}
```

### 6.2 Use Ternary Instead of &&

The `&&` operator can cause rendering glitches; use ternary for conditionals.

**Incorrect:**
```tsx
{isLoading && <Spinner />}
```

**Correct:**
```tsx
{isLoading ? <Spinner /> : null}
```

### 6.3 Hoist Static JSX

Extract JSX that doesn't depend on props outside the component.

**Incorrect (recreated every render):**
```tsx
function Layout() {
  const header = <header>App</header> // Recreated every render
  return <div>{header}...</div>
}
```

**Correct (defined once):**
```tsx
const HEADER = <header>App</header>

function Layout() {
  return <div>{HEADER}...</div>
}
```

## 7. JavaScript Performance (LOW-MEDIUM)

Micro-optimizations for hot code paths.

### 7.1 Combine Multiple Loops into One

One loop scanning data multiple times ≠ several loops.

**Incorrect (scans list 3 times):**
```typescript
const total = items.reduce((sum, item) => sum + item.price, 0)
const count = items.filter(item => item.active).length
const expensive = items.find(item => item.cost > 1000)
```

**Correct (single pass):**
```typescript
let total = 0
let count = 0
let expensive = null

for (let i = 0; i < items.length; i++) {
  total += items[i].price
  if (items[i].active) count++
  if (!expensive && items[i].cost > 1000) expensive = items[i]
}
```

### 7.2 Cache Repeated Lookups

Store repeated object property or Map lookups in a variable.

**Incorrect (redundant access):**
```typescript
for (let i = 0; i < users.length; i++) {
  console.log(users[i].profile.settings.theme) // Three lookups per iteration
}
```

**Correct (cache properties):**
```typescript
for (let i = 0; i < users.length; i++) {
  const user = users[i]
  const settings = user.profile.settings
  console.log(settings.theme)
}
```

### 7.3 Use Set/Map for O(1) Lookups

Don't iterate arrays for membership checks; use Set or Map.

**Incorrect (O(n) lookup):**
```typescript
const adminIds = [1, 2, 3, 4]

if (adminIds.includes(userId)) {
  // Permission granted
}
```

**Correct (O(1) lookup):**
```typescript
const adminIds = new Set([1, 2, 3, 4])

if (adminIds.has(userId)) {
  // Permission granted
}
```

### 7.4 Hoist RegExp Outside Render

Don't create RegExp inside render/loops; define once at module scope.

**Incorrect:**
```tsx
function Formatter({ text, pattern }: Props) {
  const regex = new RegExp(pattern, 'g') // New object every render
  return <div>{text.replace(regex, '...')}</div>
}
```

**Correct:**
```tsx
function Formatter({ text, pattern }: Props) {
  const regex = useMemo(
    () => new RegExp(pattern, 'g'),
    [pattern]
  )
  return <div>{text.replace(regex, '...')}</div>
}
```

## 8. Advanced Patterns (LOW)

Advanced techniques for specific use cases.

### 8.1 useEffectEvent (React 19+)

Stable callback references that don't trigger dependency re-runs.

**Incorrect (recreates on dependency change):**
```tsx
function Page({ userId }) {
  const [data, setData] = useState(null)
  
  const handleSuccess = useCallback((result) => {
    setData(result)
  }, [userId]) // Recreates if userId changes
  
  useEffect(() => {
    fetch(`/api/user/${userId}`).then(handleSuccess)
  }, [userId, handleSuccess])
}
```

**Correct (stable callback):**
```tsx
function Page({ userId }) {
  const [data, setData] = useState(null)
  
  const handleSuccess = useEffectEvent((result) => {
    setData(result)
  })
  
  useEffect(() => {
    fetch(`/api/user/${userId}`).then(handleSuccess)
  }, [userId])
}
```

### 8.2 Move Interaction Logic to Event Handlers

Don't put interaction logic in effects; use event handlers.

**Incorrect (effect for user action):**
```tsx
function Input({ value, onChange }) {
  useEffect(() => {
    if (value.length > 10) {
      logEvent('input_too_long')
    }
  }, [value])
  
  return <input value={value} onChange={onChange} />
}
```

**Correct (event handler):**
```tsx
function Input({ value, onChange }) {
  const handleChange = (e) => {
    if (e.target.value.length > 10) {
      logEvent('input_too_long')
    }
    onChange(e)
  }
  
  return <input value={value} onChange={handleChange} />
}
```

## References

- [Vercel React Best Practices Blog](https://vercel.com/blog/introducing-react-best-practices)
- [React Documentation](https://react.dev)
- [Next.js Framework](https://nextjs.org)
- [SWR (Data Fetching)](https://swr.vercel.app)
- [React Query](https://tanstack.com/query/latest)

## Additional Resources

For KalamDB-specific patterns:
- Review existing components in `/ui/src/components/` for examples
- Follow TypeScript patterns defined in the project
- Use the component library established in the UI codebase
