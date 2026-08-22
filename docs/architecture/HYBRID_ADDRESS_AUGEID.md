# Modelo Hibrido de Enderecos e AUGEID

## Fluxo

```text
Wallet Ed25519
     |
     v
Endereco Base58 + checksum BLAKE3
     |
     v
address_index (O(1))
     | existe                    | ausente
     v                           v
 AUGEID existente       validar public key -> Reserved deterministico
     |                           |
     +-------------> SafeBox: Reserved -> Owned + transferencia
                         (uma unica transicao atomica)
```

## Protocolo

- Operacoes legadas continuam usando `OperationPayload::Transaction` e
  `ReceiverInfo { account: u64, ... }` sem mudanca de wire format.
- `OperationPayload::AddressTransaction` usa a tag `0x0F` e transporta o hash
  do endereco, a chave publica Ed25519, valor e payload.
- Enderecos novos sao auto-descritivos: carregam a chave publica Ed25519 e um
  checksum BLAKE3 domain-separated de 4 bytes.
- Enderecos legados continuam sendo `BLAKE3(public_key)[0..24]` mais checksum.
  Eles continuam resolviveis para contas ja indexadas, mas nao podem ativar
  uma conta nova sem um binding adicional, pois um hash nao revela a chave.
- AUGEIDs numerados, `Reserved`, `Owned`, `pubkey_index` e `CreateAccount`
  permanecem existentes e continuam sendo a fonte de consenso.
- A wallet nunca cria ou minera AUGEID. O primeiro recebimento apenas consome
  um AUGEID `Reserved` previamente emitido como recompensa de validador e o
  ativa para a chave pública do destinatário.

## RPC

`resolve_address` e seu alias `resolveaddress` aceitam `{ "address": "..." }`.

Resposta para destino conhecido:

```json
{"exists":true,"account":845621,"public_key":null}
```

Resposta para endereco valido ainda nao ativado:

```json
{"exists":false,"account":null,"public_key":null}
```

`sendoperation` continua sendo o endpoint de submissao assinado. A wallet usa
um campo unico e detecta `AUGE-<numero>` ou endereco antes de construir a
operacao correspondente.

## Storage e banco da wallet

- O SafeBox mantem `address_index: HashMap<AddressHash, u64>` residente.
- O storage mantem o mesmo indice incrementalmente junto de `pubkey_index` e
  grava o mapeamento hash -> conta na column family de indices em `WriteBatch`.
- A tabela `platform_users` possui `address`, `public_key_hex`, `augeid`,
  `activation_tx` e `first_receive`. Nenhuma chave privada e persistida no
  backend.

## Consenso

O PoA, quorum, assinatura do bloco, hash de operacoes, emissao e SafeBox hash
nao foram removidos. A ativacao e a transferencia entram no mesmo conjunto
`modified` e chegam ao mesmo `commit_block_atomic`, portanto nao existe estado
intermediario observavel ou operacao parcialmente aplicada. A escolha do
primeiro `Reserved` e feita pela ordem residente do SafeBox, deterministica
para todos os validadores.

## Testes

- Roundtrip de serializacao para transferencia por endereco.
- Indice de endereco residente e resolucao O(1) no SafeBox.
- Os testes legados de serializacao e SafeBox continuam cobrindo o formato
  anterior.

## Checklist de seguranca

- [x] Ed25519 para autorizacao.
- [x] BLAKE3 para derivacao/checksum.
- [x] Validacao de hash contra chave publica.
- [x] Nonce `n_operation` e `chain_id` para replay.
- [x] `WriteBatch` atomico no commit de bloco.
- [x] TLS existente no RPC.
- [x] Chave privada permanece somente na wallet.

## Checklist de compatibilidade

- [x] Operacoes numeradas antigas preservadas.
- [x] `Reserved`, `Owned`, `pubkey_index` e `CreateAccount` preservados.
- [x] PoA e SafeBox hash preservados.
- [x] Sem uso de `iter_accounts()` nos caminhos de resolucao de endereco.
- [x] Clientes novos constroem `AddressTransaction` sem campo de chave publica
  separado; a chave vem do endereco auto-descritivo.
