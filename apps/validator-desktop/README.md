# AUGECOIN Validator Desktop (Tauri)

Software de validação para Windows e Linux. Núcleo Rust + interface Tauri.

## Fluxo

1. **Ativação** — o usuário informa a licença (+ AUGEID opcional). O app:
   - gera o par Ed25519 localmente (`augecoin-crypto`), a **chave privada nunca
     sai da máquina**;
   - calcula o `machine_id = BLAKE3(install_id || hardware)` — os dados brutos
     de hardware **nunca são persistidos**;
   - chama `POST /activate` no backend operacional (só chave pública + hash).
2. **Sincronização** — o app inicia o `augecoin-node` (sidecar) com a seed.
3. **Heartbeat** — a cada 30s o app consulta o node e envia
   `POST /validator/heartbeat`.
4. **Dashboard** — status, sincronização, último bloco, ganhos (hoje/total) e
   indicador "Você é o Validador Chefe" quando produz blocos.
5. **OTA** — `GET /updates` + verificação de assinatura Ed25519 sobre o hash do
   artefato; **nunca instala binário sem assinatura válida**.

## Estrutura

```
src-tauri/src/
  machine.rs    Machine ID (BLAKE3 de hardware, sem persistir dados brutos)
  ops.rs        Cliente dos endpoints públicos do augecoin-ops
  state.rs      Estado local (state.json + validator.key 0600)
  commands.rs   Comandos Tauri: activate, get_dashboard, node_status, check_update
src/            Frontend React (ativação + dashboard)
```

## Configuração (env)

| Variável | Padrão | Descrição |
|---|---|---|
| `AUGECOIN_OPS_API_URL` | `https://operacional.augeco.in/api` | Backend operacional (TLS obrigatório) |
| `AUGECOIN_NODE_BIN` | sidecar ao lado do executável | Caminho do binário `augecoin-node` |
| `AUGECOIN_NODE_RPC_PORT` | `9005` | Porta RPC local do node |

Configuração extra do node (gênesis, bootnodes, chain-id, portas) é fornecida
pelo operador em `~/.local/share/augecoin-validator/node.env` (`KEY=VALUE`).

## Segurança

- Chave privada gerada e armazenada localmente (arquivo `0600`), passada ao node
  apenas via variável de ambiente.
- Machine Binding anti-clonagem: uma licença só ativa numa máquina.
- TLS obrigatório na API; OTA com verificação de assinatura em duas pontas.
- A chave de release (`AUGECOIN_RELEASE_PUBLIC_KEY_HEX`, hex Ed25519 de 64 chars)
  deve ser definida no ambiente de build (ou via secret no CI) para habilitar a
  instalação OTA. Sem ela, o OTA fica desabilitado.

## Build

```bash
./BUILD.sh
```

Gera `.deb` e `.AppImage` (Linux). O `.msi` (Windows) é cross-compilado via CI ou
host Windows.
