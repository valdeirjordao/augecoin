// AUGECOIN Wallet Web — shared types (mirrors crates/augecoin-rpc responses).

export type AccountState =
  | 'Unknown'
  | 'Normal'
  | 'ForSale'
  | 'ForAtomicAccountSwap'
  | 'ForAtomicCoinSwap'
  | 'Reserved'
  | 'Owned'
  | 'GiftPending';

/** `getaccount` response. */
export interface AccountInfo {
  account_number: number;
  balance: number;
  n_operation: number;
  name: string | null;
  account_type: number;
  account_data_hex: string;
  account_seal_hex: string;
  state: AccountState;
  updated_on_block_passive_mode: number;
  updated_on_block_active_mode: number;
  locked_until_block: number;
  price: number;
  account_to_pay: number;
  account_key_ed_hex: string;
}

/** `listaccountsforsale` entry. */
export interface MarketplaceEntry {
  account_number: number;
  price: number;
  seller_public_key_hex: string;
  state: 'ForSale';
  listed_at_block: number;
}

/** `listvalidatorinventory` response. */
export interface InventoryResult {
  reserved: number[];
  for_sale: number[];
  owned: number[];
}

/** `listpendinggifts` entry. */
export interface PendingGiftEntry {
  account_number: number;
  recipient_public_key_hex: string;
  from_public_key_hex: string;
  gifted_at_block: number;
  name: string | null;
}

/** Lifecycle op result (buy/sell/gift/accept/cancel). */
export interface LifecycleOpResult {
  accepted: boolean;
  op_hash_hex: string | null;
  error: string | null;
}

export interface NodeStatus {
  current_height: number;
  latest_block_hash_hex: string;
  peers_connected: number;
  peers_gossipsub_consensus: number;
  peers_kademlia_total: number;
  syncing: boolean;
  sync_target_height: number;
  total_accounts: number;
  total_blocks: number;
  mempool_size: number;
  current_round: number;
  current_view: number;
  validator_id: number;
  chain_id: number;
  uptime_seconds: number;
  last_consensus_error: string | null;
}

// ── Camada 1: Conta da Plataforma ─────────────────────────────────────
//
// Exists only in the wallet-web. Login / cadastro / perfil / sessão /
// preferências. Has NO balance and never creates an AUGEID. The `public_key_hex`
// is the user's blockchain identity key (public only — the private key never
// leaves the client and is never stored by the backend).

export interface PlatformUser {
  id: string;
  email: string;
  display_name: string;
  public_key_hex: string;
  created_at: string;
  last_login: string | null;
}

export interface UserPreferences {
  theme: string;
  language: string;
  notifications: boolean;
}

// ── Wallet Registry: ponte usuário <-> AUGEID ─────────────────────────
//
// Does NOT hold balance, state or keys. Maps a platform user to the on-chain
// account numbers they own. On-chain truth comes from `getaccount`.

export interface LinkedWallet {
  user_id: string;
  account_number: number;
  linked_at: string;
}

/** A linked wallet enriched with on-chain info (`getaccount`). */
export interface LinkedWalletInfo extends LinkedWallet {
  account: AccountInfo | null;
}

/** A directory entry (for gift/transfer recipient resolution). */
export interface DirectoryMember {
  id: string;
  email: string;
  display_name: string;
  public_key_hex: string;
}

/** A resolved AUGEID for the send/receive flows. */
export interface AugeIdSummary {
  account_number: number;
  name: string | null;
  balance: number;
  state: AccountState;
}

// ── Camada 2: SaaS de Validação (licenças de validador) ───────────────
//
// Lives in the operational backend (augecoin-ops); the wallet-web server proxies
// the parts owned by the authenticated user. Monetary amounts (augesat) arrive
// as strings to avoid precision loss; parse with BigInt.

export type PlanId = 'monthly' | 'semiannual' | 'annual';
export type OrderMethod = 'pix' | 'usdt' | 'auge';
export type OrderStatus = 'pending' | 'paid' | 'issued' | 'cancelled';

export interface Plan {
  id: PlanId;
  label: string;
  usd: number;
  discount: string | null;
}

export interface ValidatorOrder {
  id: string;
  user_id: string;
  plan: PlanId;
  method: OrderMethod;
  amount_usd: number;
  status: OrderStatus;
  license_id: string | null;
  created_at: string;
  paid_at: string | null;
  issued_at: string | null;
}

export type LicenseStatus = 'active' | 'expired' | 'suspended' | 'revoked';

export interface LicenseView {
  id: string;
  license_key_prefix: string;
  user_id: string;
  plan: PlanId;
  augeid: string | null;
  machine_hash: string | null;
  public_key: string | null;
  ip: string | null;
  expires_at: string;
  status: LicenseStatus;
  created_at: string;
}

export interface RewardBucket {
  auge: string;
  augeids: number;
  fees: string;
  blocks: number;
}

export interface RewardSummary {
  hour: RewardBucket;
  day: RewardBucket;
  week: RewardBucket;
  month: RewardBucket;
  year: RewardBucket;
  total: RewardBucket;
}

export type ValidatorStatus = 'pending' | 'active' | 'suspended' | 'revoked';

export interface ValidatorView {
  id: string;
  license_id: string;
  public_key: string;
  augeid: string | null;
  ip: string | null;
  machine_hash: string | null;
  node_validator_id: number | null;
  os: string | null;
  cpu: number | null;
  ram: number | null;
  version: string | null;
  uptime: number;
  blocks: number;
  blocks_lost: number;
  leadership: number;
  total_rewards: string;
  last_seen: string | null;
  status: ValidatorStatus;
  online: boolean;
  license_status: LicenseStatus;
  license_expires_at: string;
  created_at: string;
}

export interface ValidatorDetail {
  validator: ValidatorView;
  rewards: RewardSummary;
}

export interface ValidatorOverview {
  licenses: LicenseView[];
  validators: ValidatorDetail[];
}
