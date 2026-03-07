# Cache: Response Caching

## Requirement: Cache-Aside Pattern

Read-heavy endpoints cache responses in Redis with configurable TTLs.

#### Scenario: Cache miss
Given a GET request to a cacheable endpoint
And no cached response exists
When the request is processed
Then the response is computed from the database
And stored in Redis with the configured TTL

#### Scenario: Cache hit
Given a GET request to a cacheable endpoint
And a valid cached response exists
When the request is processed
Then the cached response is returned
And the database is not queried

#### Scenario: Cache invalidation on write
Given a cached response for `/v2/users/123`
When a PUT request modifies user 123
Then the cache entry for `/v2/users/123` is evicted
And the next GET request triggers a cache miss
