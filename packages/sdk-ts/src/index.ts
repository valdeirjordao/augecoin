export { HdWallet, WasmKeyPair } from './wallet';
export type { KeyPair } from './wallet';

export {
  createTransfer,
  serializeOperation,
  serializeOperationStripped,
  serializeSignedOperation,
} from './transaction';
export type {
  Operation,
  OperationPayload,
  SenderInfo,
  ReceiverInfo,
  ChangerInfo,
  ValidatorAdminOp,
} from './transaction';

export { RpcClient } from './rpc';
/** High-level RPC wrapper entry point (Fase 16). Alias of {@link RpcClient}. */
export { RpcClient as AugeCoinClient } from './rpc';
export type {
  RpcClientConfig,
  AccountInfo,
  SendOperationResult,
  NodeStatus,
  ContractStoreCodeResult,
  ContractCreateResult,
  ContractExecuteResult,
  ContractGetResult,
  ContractBalanceResult,
  ContractTokenInfoResult,
  TokenInfoView,
} from './rpc';

export {
  ContractClient,
  TokenClient,
  encodeTokenInit,
  encodeTokenCall,
  type TokenInit,
  type TokenCall,
  type ContractOp,
  type Address,
  type CodeId,
} from './contracts';

export {
  bytesToHex,
  hexToBytes,
  u64ToBeHex,
  beHexToU64,
  addressToBeHex,
  beHexToAddress,
  codeIdToBeHex,
  beHexToCodeId,
} from './hex';

export {
  getAddress,
  validateAddress,
  addressFromPublicKeyHex,
} from './address';

export {
  augesatToAuge,
  augeToAugesat,
  formatAuge,
  DECIMALS,
  AUGESAT_PER_AUGE,
  TOTAL_SUPPLY_AUGE,
  TOTAL_SUPPLY_AUGESAT,
} from './utils';
