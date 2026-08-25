// In-memory sliding-window rate limiter. Mirrors the intent of the node's
// RateLimiter without introducing consensus or persistent state.

const buckets = new Map();

/**
 * @param {string} key
 * @param {{limit:number, windowMs:number}} opts
 * @returns {{ok:boolean, remaining:number, retryAfterMs:number}}
 */
export function check(key, { limit, windowMs }) {
  const now = Date.now();
  const entry = buckets.get(key);
  if (!entry) {
    buckets.set(key, { hits: [now] });
    return { ok: true, remaining: limit - 1, retryAfterMs: 0 };
  }
  entry.hits = entry.hits.filter((t) => now - t < windowMs);
  if (entry.hits.length >= limit) {
    const oldest = entry.hits[0];
    const retryAfterMs = windowMs - (now - oldest);
    return { ok: false, remaining: 0, retryAfterMs: Math.max(retryAfterMs, 0) };
  }
  entry.hits.push(now);
  return { ok: true, remaining: limit - entry.hits.length, retryAfterMs: 0 };
}

export function reset(key) {
  buckets.delete(key);
}

// Periodic cleanup to avoid unbounded growth.
setInterval(() => {
  const now = Date.now();
  for (const [key, entry] of buckets) {
    entry.hits = entry.hits.filter((t) => now - t < 60_000);
    if (entry.hits.length === 0) buckets.delete(key);
  }
}, 60_000).unref();
