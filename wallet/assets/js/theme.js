// AUGECOIN Explorer — dark/light theme management.

const KEY = 'auge_theme';

export function initTheme() {
  const saved = (() => { try { return localStorage.getItem(KEY); } catch { return null; } })();
  const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
  const theme = saved || (prefersDark ? 'dark' : 'light');
  apply(theme);
  bindToggle();
}

function apply(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  document.querySelectorAll('[data-theme-toggle]').forEach((el) => {
    el.setAttribute('aria-pressed', String(theme === 'dark'));
  });
}

function bindToggle() {
  document.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-theme-toggle]');
    if (!btn) return;
    const current = document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
    const next = current === 'dark' ? 'light' : 'dark';
    try { localStorage.setItem(KEY, next); } catch { /* ignore */ }
    apply(next);
  });
}

export function currentTheme() {
  return document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
}
