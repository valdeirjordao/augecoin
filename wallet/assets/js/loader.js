import('./app.js?v=20260831-3').catch((error) => {
  console.error('[wallet] failed to load application:', error);
  const root = document.getElementById('page-root');
  if (root) {
    root.innerHTML = `<main class="container page"><div class="card"><h1>Não foi possível iniciar a wallet</h1><p class="muted">${String(error?.message || error)}</p><button class="btn btn-primary" type="button" data-reload-wallet>Tentar novamente</button></div></main>`;
    root.querySelector('[data-reload-wallet]')?.addEventListener('click', () => location.reload());
  }
});
