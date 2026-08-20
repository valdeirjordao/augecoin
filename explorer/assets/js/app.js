// AUGECOIN Explorer — application bootstrap & page dispatcher.

import { initTheme } from './theme.js';
import { renderHeader, renderFooter } from './components.js';
import { bindSearchForms } from './search.js';
import { getNodeStatus } from './api/client.js';

import * as home from './pages/home.js';
import * as blocks from './pages/blocks.js';
import * as block from './pages/block.js';
import * as transaction from './pages/transaction.js';
import * as account from './pages/account.js';
import * as validators from './pages/validators.js';
import * as supply from './pages/supply.js';
import * as richlist from './pages/richlist.js';
import * as search from './pages/search.js';

const PAGES = {
  home,
  blocks,
  block,
  transaction,
  account,
  validators,
  supply,
  richlist,
  search,
};

async function loadFooterStatus() {
  try {
    const s = await getNodeStatus();
    renderFooter(s);
  } catch {
    renderFooter();
  }
}

async function main() {
  initTheme();

  const page = document.body.dataset.page || 'home';
  renderHeader(page === 'home' ? 'index.html' : page + '.html');
  bindSearchForms();

  // Global delegated handlers (CSP-safe, no inline handlers).
  document.addEventListener('click', (e) => {
    if (e.target.closest('[data-retry]')) location.reload();
  });

  loadFooterStatus();

  const module = PAGES[page];
  if (module && module.render) {
    try {
      await module.render();
    } catch (err) {
      console.error(`[explorer] page "${page}" render failed:`, err);
      const host = document.getElementById('page-root');
      if (host) {
        host.innerHTML = `<div class="empty-state error"><h3>Failed to load data</h3><p>${err.message}</p><button class="btn" data-retry>Retry</button></div>`;
      }
    }
  }
}

main();
