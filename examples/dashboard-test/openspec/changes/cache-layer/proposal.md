# Cache Layer

## Problem
Database queries account for 70% of p99 latency. The v2 API is expected to handle 10x traffic. No caching exists.

## Solution
Add Redis-backed caching for read-heavy endpoints. Cache-aside pattern with configurable TTLs per resource type.

## Design Reference
`design/cache-layer.md` — seed status, open questions remain.

## Scope
- `src/cache/` — new (cache client, invalidation)
- `src/api/middleware/cache.ts` — new (response caching middleware)
- `src/config/cache.ts` — new (TTL configuration)
