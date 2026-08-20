// AUGECOIN Wallet — network particle background for the public landing hero.
//
// Pure Canvas (no external libs), requestAnimationFrame, pauses when the tab
// is hidden, honors `prefers-reduced-motion: reduce` (no animation loop) and
// reduces particle count on small screens. Nodes are connected by thin
// `--border` lines; active nodes glow with `--color-primary`.

export function initHomepageBg(canvas) {
  if (!canvas || typeof window === 'undefined') return null;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;

  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const css = getComputedStyle(document.documentElement);
  const borderColor = css.getPropertyValue('--border').trim() || '#2B3A55';
  const primary = css.getPropertyValue('--color-primary').trim() || '#F97316';
  const primaryRgb = css.getPropertyValue('--color-primary-rgb').trim() || '249, 115, 22';

  let width = 0;
  let height = 0;
  let particles = [];
  let rafId = null;
  let running = false;

  const isMobile = () => window.innerWidth < 768;
  const linkDist = () => (isMobile() ? 90 : 132);
  const particleCount = () => (isMobile() ? 26 : 60);

  function seed() {
    const count = particleCount();
    particles = Array.from({ length: count }, () => ({
      x: Math.random() * width,
      y: Math.random() * height,
      vx: (Math.random() - 0.5) * 0.32,
      vy: (Math.random() - 0.5) * 0.32,
      r: Math.random() * 1.5 + 0.9,
      phase: Math.random() * Math.PI * 2,
    }));
  }

  function resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    width = canvas.offsetWidth || canvas.clientWidth || 1;
    height = canvas.offsetHeight || canvas.clientHeight || 1;
    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    seed();
    if (!running && !reduceMotion) drawFrame(0);
  }

  function drawFrame(ts) {
    const t = ts * 0.001;
    ctx.clearRect(0, 0, width, height);

    const n = particles.length;
    const dist = linkDist();

    // edges
    ctx.lineWidth = 0.6;
    for (let i = 0; i < n; i++) {
      const a = particles[i];
      for (let j = i + 1; j < n; j++) {
        const b = particles[j];
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        const d = Math.hypot(dx, dy);
        if (d < dist) {
          ctx.strokeStyle = borderColor;
          ctx.globalAlpha = (1 - d / dist) * 0.28;
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
      }
    }
    ctx.globalAlpha = 1;

    // nodes (cheap glow via a soft halo circle instead of shadowBlur)
    for (let i = 0; i < n; i++) {
      const p = particles[i];
      const active = Math.sin(t * 1.1 + p.phase) > 0.82;
      if (active) {
        ctx.fillStyle = `rgba(${primaryRgb}, 0.18)`;
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.r * 3.2, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = primary;
      } else {
        ctx.fillStyle = borderColor;
      }
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  let lastDraw = 0;

  function step(ts) {
    if (!running) return;
    if (document.hidden) {
      rafId = requestAnimationFrame(step);
      return;
    }
    if (!canvas.isConnected) {
      running = false;
      return;
    }
    for (const p of particles) {
      p.x += p.vx;
      p.y += p.vy;
      if (p.x < -20) p.x = width + 20;
      else if (p.x > width + 20) p.x = -20;
      if (p.y < -20) p.y = height + 20;
      else if (p.y > height + 20) p.y = -20;
    }
    // Cap drawing at ~30fps to keep the main thread light (60fps target).
    if (ts - lastDraw >= 33) {
      drawFrame(ts);
      lastDraw = ts;
    }
    rafId = requestAnimationFrame(step);
  }

  function start() {
    if (running || reduceMotion || !inViewport) return;
    running = true;
    rafId = requestAnimationFrame(step);
  }

  function stop() {
    running = false;
    if (rafId) cancelAnimationFrame(rafId);
  }

  function onVisibility() {
    if (document.hidden) stop();
    else start();
  }

  // Pause when the hero scrolls out of view (saves CPU/GPU).
  let inViewport = true;
  let io = null;
  if (typeof IntersectionObserver !== 'undefined') {
    io = new IntersectionObserver((entries) => {
      inViewport = entries.some((e) => e.isIntersecting);
      if (inViewport) start();
      else stop();
    });
    io.observe(canvas);
  }

  resize();
  window.addEventListener('resize', resize);
  document.addEventListener('visibilitychange', onVisibility);

  if (!reduceMotion) {
    // Initial static frame so the hero never flashes empty before the loop.
    drawFrame(0);
    start();
  }

  return () => {
    stop();
    window.removeEventListener('resize', resize);
    document.removeEventListener('visibilitychange', onVisibility);
    if (io) io.disconnect();
  };
}
