// AUGECOIN Explorer — realtime polling helpers.

import { CONFIG } from './config.js';

/**
 * Generic poller: run `fn` now and every `intervalMs`, re-running only when
 * the previous run has completed (no overlap).
 */
export function createPoller(fn, intervalMs = CONFIG.POLL_INTERVAL_MS, immediate = true) {
  let running = false;
  let stopped = false;

  const tick = async () => {
    if (stopped) return;
    if (running) return;
    running = true;
    try { await fn(); } catch { /* handled by fn */ }
    running = false;
  };

  if (immediate) tick();
  const timer = setInterval(tick, intervalMs);
  if (timer.unref) timer.unref();

  return {
    stop() { stopped = true; clearInterval(timer); },
    refresh: tick,
  };
}

/**
 * TPS estimator: samples block height over time.
 * Returns { tps, avgBlockTime } based on recent height deltas.
 */
export function createTpsTracker() {
  const samples = [];
  return {
    sample(height, tsMs = Date.now()) {
      samples.push({ height, ts: tsMs });
      if (samples.length > 12) samples.shift();
    },
    compute() {
      if (samples.length < 2) return { tps: 0, avgBlockTime: CONFIG.BLOCK_TIME_SECONDS };
      const first = samples[0];
      const last = samples[samples.length - 1];
      const dt = (last.ts - first.ts) / 1000;
      const dh = last.height - first.height;
      const avgBlockTime = dh > 0 ? dt / dh : CONFIG.BLOCK_TIME_SECONDS;
      const tps = dt > 0 ? dh / dt : 0;
      return { tps, avgBlockTime };
    },
  };
}
