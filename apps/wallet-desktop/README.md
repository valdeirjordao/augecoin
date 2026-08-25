# AUGECOIN Wallet Desktop (Tauri)

Carteira desktop para Windows, Linux e macOS. Núcleo Rust + interface Tauri.

## Funcionalidades

- **Criar carteira** — gera um mnemônico BIP-39 localmente (`create_wallet`).
- **Importar carteira** — restaura a partir de um mnemônico (`import_wallet`).
- **Derivação de chaves** — deriva pares Ed25519 por índice (`derive_keypair`).
- **Assinatura** — assina mensagens localmente (`sign_message`).
- **Keystore** — armazena o mnemônico cifrado (XOR + senha) no cofre do sistema
  (`keystore_save` / `keystore_load` / `keystore_delete`), via `keyring`.

## Estrutura

```
src-tauri/src/
  commands.rs  Comandos Tauri: create_wallet, import_wallet, derive_keypair,
               sign_message, keystore_save/load/delete
src/           Frontend React
```

## Segurança

- O mnemônico só existe em memória durante a criação/importação; nunca é
  persistido em texto claro.
- A keystore cifra o mnemônico antes de armazenar no cofre do sistema operacional.
- Chaves são derivadas localmente com `augecoin-crypto`.

## Build

```bash
./BUILD.sh
```

Gera `.deb` e `.AppImage` (Linux) ou `.msi`/`.nsis` (Windows) de acordo com o
host. O `.msi` (Windows) também é produzido via CI.
