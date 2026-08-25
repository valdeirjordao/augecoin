# AUGECOIN Operacional — Painel Administrativo

Painel de administração da rede de validadores, servido em
`operacional.augeco.in`. SPA vanilla (HTML5 + CSS3 + JS, sem build step),
consumindo a API do backend `augecoin-ops`.

## Arquitetura

```
operacional.augeco.in (nginx, estático + TLS)
        │  /api/*  (proxy_pass → 127.0.0.1:8790)
        ▼
augecoin-ops (Rust/axum)  ──▶  PostgreSQL (licenças, validadores, recompensas)
        │  bridge JSON-RPC (validatoradd/remove/deactivate)
        ▼
node AUGECOIN (assina e submete operações administrativas)
```

O painel **não** fala com o node RPC diretamente: toda autorização, licença,
monitoramento e auditoria passam pela API do `augecoin-ops`. O consenso continua
sendo a autoridade — o painel só autoriza/monitora.

## Segurança

- Acesso restrito por `x-api-key` (header), enviada a cada requisição.
- A chave fica apenas em `sessionStorage` (não persiste).
- TLS obrigatório (nginx) + HSTS, CSP, `X-Frame-Options: DENY`, no-store.
- Valores monetários (augesat) chegam como **string** para evitar perda de
  precisão (acima de 2^53) — o frontend usa `BigInt`.

## Estrutura

```
index.html                    shell + CSP
assets/css/*.css              design system (dark theme, glassmorphism, cards)
assets/js/config.js           endpoint /api + chave (sessionStorage)
assets/js/api.js              cliente REST (x-api-key, erros normalizados)
assets/js/charts.js           gráficos SVG leves (barra/linha)
assets/js/app.js              SPA: login, sidebar, rotas e views
assets/js/{theme,utils,components}.js   helpers compartilhados
```

## Views (rotas hash)

| Rota | Descrição |
|---|---|
| `#/dashboard` | KPIs de rede (`GET /metrics`) + gráfico de distribuição |
| `#/validators` | Tabela de validadores + filtros + aprovar/suspender/revogar |
| `#/validators/{id}` | Página individual: info, uptime, ganhos AUGE/AUGEID |
| `#/licenses` | Licenças: emitir, suspender, revogar |
| `#/ganhos` | Dashboard financeiro (rede + por validador) |
| `#/monitoramento` | Heartbeats (verde/amarelo/vermelho) + alertas |
| `#/auditoria` | Trilha imutável (`GET /audit`) paginada |
| `#/configuracoes` | Endpoint, chave, releases (OTA) |

## Deploy

O backend roda como serviço systemd (`augecoin-ops.service`, porta 8790). O
nginx faz proxy de `/api/` → `http://127.0.0.1:8790/` (ver
`/etc/nginx/sites-available/operacional.augeco.in`). Para trocar o endpoint em
runtime: `?ops=<url>` ou `localStorage.auge_ops_api`.
