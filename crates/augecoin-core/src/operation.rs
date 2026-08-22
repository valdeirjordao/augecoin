use augecoin_crypto::address::AddressHash;
use augecoin_crypto::signature::Ed25519Signature;
use augecoin_crypto::signature::HybridSignature;
use thiserror::Error;

pub const ED25519_SIG_LEN: usize = 64;
const ED25519_PK_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Transaction = 0x01,
    AddressTransaction = 0x0F,
    ChangeKey = 0x02,
    RecoverFounds = 0x03,
    ListAccountForSale = 0x04,
    DelistAccount = 0x05,
    BuyAccount = 0x06,
    ChangeKeySigned = 0x07,
    ChangeAccountInfo = 0x08,
    MultiOperation = 0x09,
    Data = 0x0A,
    ValidatorAdminOp = 0x0B,
    CreateAccount = 0x0C,
    GiftAccount = 0x0D,
    AcceptGift = 0x0E,
}

impl OperationType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(OperationType::Transaction),
            0x0F => Some(OperationType::AddressTransaction),
            0x02 => Some(OperationType::ChangeKey),
            0x03 => Some(OperationType::RecoverFounds),
            0x04 => Some(OperationType::ListAccountForSale),
            0x05 => Some(OperationType::DelistAccount),
            0x06 => Some(OperationType::BuyAccount),
            0x07 => Some(OperationType::ChangeKeySigned),
            0x08 => Some(OperationType::ChangeAccountInfo),
            0x09 => Some(OperationType::MultiOperation),
            0x0A => Some(OperationType::Data),
            0x0B => Some(OperationType::ValidatorAdminOp),
            0x0C => Some(OperationType::CreateAccount),
            0x0D => Some(OperationType::GiftAccount),
            0x0E => Some(OperationType::AcceptGift),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderInfo {
    pub account: u64,
    pub n_operation: u64,
    pub amount: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiverInfo {
    pub account: u64,
    pub amount: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressReceiverInfo {
    pub address: AddressHash,
    pub amount: u64,
    pub payload: Vec<u8>,
}

/// Destination abstraction used by address-aware callers. Legacy operations
/// continue to use `ReceiverInfo { account, ... }` unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Receiver {
    Account(u64),
    Address(AddressHash),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangerInfo {
    pub account: u64,
    pub n_operation: u64,
    pub new_ed25519_public_key: [u8; ED25519_PK_LEN],
    pub new_name: Option<String>,
    pub new_type: u16,
    pub new_account_data: Vec<u8>,
    pub new_account_seal: Vec<u8>,
    pub fee: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorAdminOp {
    Add {
        ed25519_public_key: [u8; ED25519_PK_LEN],
        activation_height: u64,
    },
    Remove {
        validator_id: u64,
        activation_height: u64,
    },
    Activate {
        validator_id: u64,
        activation_height: u64,
    },
    Deactivate {
        validator_id: u64,
        activation_height: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationPayload {
    Transaction {
        senders: Vec<SenderInfo>,
        receivers: Vec<ReceiverInfo>,
        changers: Vec<ChangerInfo>,
        fee: u64,
    },
    AddressTransaction {
        senders: Vec<SenderInfo>,
        receivers: Vec<AddressReceiverInfo>,
        fee: u64,
    },
    ChangeKey {
        account: u64,
        n_operation: u64,
        fee: u64,
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
    },
    RecoverFounds {
        account: u64,
    },
    ListAccountForSale {
        account: u64,
        n_operation: u64,
        sale_price: u64,
        account_to_pay: u64,
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
        locked_until_block: u64,
        fee: u64,
    },
    DelistAccount {
        account: u64,
        n_operation: u64,
        fee: u64,
    },
    BuyAccount {
        buyer_account: u64,
        n_operation: u64,
        account_to_purchase: u64,
        amount: u64,
        fee: u64,
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
        seller_account: u64,
    },
    ChangeKeySigned {
        account: u64,
        n_operation: u64,
        fee: u64,
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
        new_signature: HybridSignature,
    },
    ChangeAccountInfo {
        account: u64,
        n_operation: u64,
        fee: u64,
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
        new_name: Option<String>,
        new_type: u16,
        new_account_data: Vec<u8>,
        new_account_seal: Vec<u8>,
    },
    MultiOperation {
        senders: Vec<SenderInfo>,
        receivers: Vec<ReceiverInfo>,
        changers: Vec<ChangerInfo>,
        fee: u64,
    },
    Data {
        account: u64,
        n_operation: u64,
        fee: u64,
        data: Vec<u8>,
        senders: Vec<SenderInfo>,
        receivers: Vec<ReceiverInfo>,
        changers: Vec<ChangerInfo>,
    },
    ValidatorAdmin(ValidatorAdminOp),
    CreateAccount {
        /// The AUGEID number to activate. Must reference an existing
        /// `Reserved` AUGEID owned by the signer (the block leader). Never
        /// creates a new number.
        account_number: u64,
        pubkey: [u8; ED25519_PK_LEN],
        initial_metadata: Vec<u8>,
    },
    GiftAccount {
        account: u64,
        n_operation: u64,
        recipient_public_key: [u8; ED25519_PK_LEN],
        fee: u64,
    },
    AcceptGift {
        account: u64,
        n_operation: u64,
        fee: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub op_type: OperationType,
    pub payload: OperationPayload,
    pub chain_id: u64,
    pub signatures: Vec<HybridSignature>,
}

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("invalid serialized data: {0}")]
    InvalidSerialization(String),
    #[error("unknown operation type: {0}")]
    UnknownOperationType(u8),
    #[error("unknown validator admin tag: {0}")]
    UnknownValidatorAdminTag(u8),
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, OperationError> {
    if *pos + 8 > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let bytes: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(bytes))
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, OperationError> {
    if *pos + 4 > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let bytes: [u8; 4] = data[*pos..*pos + 4].try_into().unwrap();
    *pos += 4;
    Ok(u32::from_be_bytes(bytes))
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn read_u16(data: &[u8], pos: &mut usize) -> Result<u16, OperationError> {
    if *pos + 2 > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let bytes: [u8; 2] = data[*pos..*pos + 2].try_into().unwrap();
    *pos += 2;
    Ok(u16::from_be_bytes(bytes))
}

fn write_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8, OperationError> {
    if *pos >= data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let v = data[*pos];
    *pos += 1;
    Ok(v)
}

fn write_fixed(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn read_fixed<const N: usize>(data: &[u8], pos: &mut usize) -> Result<[u8; N], OperationError> {
    if *pos + N > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&data[*pos..*pos + N]);
    *pos += N;
    Ok(arr)
}

fn write_bytes_with_len(buf: &mut Vec<u8>, bytes: &[u8]) {
    write_u16(buf, bytes.len() as u16);
    buf.extend_from_slice(bytes);
}

fn read_bytes_with_len(data: &[u8], pos: &mut usize) -> Result<Vec<u8>, OperationError> {
    let len = read_u16(data, pos)? as usize;
    if *pos + len > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let bytes = data[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(bytes)
}

fn write_optional_str(buf: &mut Vec<u8>, s: &Option<String>) {
    match s {
        Some(s) => {
            write_u16(buf, s.len() as u16);
            buf.extend_from_slice(s.as_bytes());
        }
        None => write_u16(buf, 0xFFFF),
    }
}

fn read_optional_str(data: &[u8], pos: &mut usize) -> Result<Option<String>, OperationError> {
    let len = read_u16(data, pos)? as usize;
    if len == 0xFFFF {
        return Ok(None);
    }
    if *pos + len > data.len() {
        return Err(OperationError::InvalidSerialization(
            "unexpected EOF".into(),
        ));
    }
    let s = String::from_utf8(data[*pos..*pos + len].to_vec())
        .map_err(|_| OperationError::InvalidSerialization("invalid UTF-8".into()))?;
    *pos += len;
    Ok(Some(s))
}

fn write_sender(buf: &mut Vec<u8>, s: &SenderInfo) {
    write_u64(buf, s.account);
    write_u64(buf, s.n_operation);
    write_u64(buf, s.amount);
    write_bytes_with_len(buf, &s.payload);
}

fn read_sender(data: &[u8], pos: &mut usize) -> Result<SenderInfo, OperationError> {
    let account = read_u64(data, pos)?;
    let n_operation = read_u64(data, pos)?;
    let amount = read_u64(data, pos)?;
    let payload = read_bytes_with_len(data, pos)?;
    Ok(SenderInfo {
        account,
        n_operation,
        amount,
        payload,
    })
}

fn write_receiver(buf: &mut Vec<u8>, r: &ReceiverInfo) {
    write_u64(buf, r.account);
    write_u64(buf, r.amount);
    write_bytes_with_len(buf, &r.payload);
}

fn read_receiver(data: &[u8], pos: &mut usize) -> Result<ReceiverInfo, OperationError> {
    let account = read_u64(data, pos)?;
    let amount = read_u64(data, pos)?;
    let payload = read_bytes_with_len(data, pos)?;
    Ok(ReceiverInfo {
        account,
        amount,
        payload,
    })
}

fn write_changer(buf: &mut Vec<u8>, c: &ChangerInfo) {
    write_u64(buf, c.account);
    write_u64(buf, c.n_operation);
    write_fixed(buf, &c.new_ed25519_public_key);
    write_optional_str(buf, &c.new_name);
    write_u16(buf, c.new_type);
    write_bytes_with_len(buf, &c.new_account_data);
    write_bytes_with_len(buf, &c.new_account_seal);
    write_u64(buf, c.fee);
}

fn read_changer(data: &[u8], pos: &mut usize) -> Result<ChangerInfo, OperationError> {
    let account = read_u64(data, pos)?;
    let n_operation = read_u64(data, pos)?;
    let new_ed25519_public_key = read_fixed(data, pos)?;
    let new_name = read_optional_str(data, pos)?;
    let new_type = read_u16(data, pos)?;
    let new_account_data = read_bytes_with_len(data, pos)?;
    let new_account_seal = read_bytes_with_len(data, pos)?;
    let fee = read_u64(data, pos)?;
    Ok(ChangerInfo {
        account,
        n_operation,
        new_ed25519_public_key,
        new_name,
        new_type,
        new_account_data,
        new_account_seal,
        fee,
    })
}

fn write_sig(buf: &mut Vec<u8>, sig: &HybridSignature) {
    write_fixed(buf, &sig.bytes);
}

fn read_sig(data: &[u8], pos: &mut usize) -> Result<HybridSignature, OperationError> {
    let bytes: [u8; ED25519_SIG_LEN] = read_fixed(data, pos)?;
    Ok(Ed25519Signature { bytes })
}

impl ValidatorAdminOp {
    pub fn tag(&self) -> u8 {
        match self {
            ValidatorAdminOp::Add { .. } => 0,
            ValidatorAdminOp::Remove { .. } => 1,
            ValidatorAdminOp::Activate { .. } => 2,
            ValidatorAdminOp::Deactivate { .. } => 3,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u8(&mut buf, self.tag());
        match self {
            ValidatorAdminOp::Add {
                ed25519_public_key,
                activation_height,
            } => {
                write_fixed(&mut buf, ed25519_public_key);
                write_u64(&mut buf, *activation_height);
            }
            ValidatorAdminOp::Remove {
                validator_id,
                activation_height,
            } => {
                write_u64(&mut buf, *validator_id);
                write_u64(&mut buf, *activation_height);
            }
            ValidatorAdminOp::Activate {
                validator_id,
                activation_height,
            } => {
                write_u64(&mut buf, *validator_id);
                write_u64(&mut buf, *activation_height);
            }
            ValidatorAdminOp::Deactivate {
                validator_id,
                activation_height,
            } => {
                write_u64(&mut buf, *validator_id);
                write_u64(&mut buf, *activation_height);
            }
        }
        buf
    }

    fn from_bytes(data: &[u8], pos: &mut usize) -> Result<Self, OperationError> {
        let va_tag = read_u8(data, pos)?;
        match va_tag {
            0 => {
                let ed25519_public_key = read_fixed(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(ValidatorAdminOp::Add {
                    ed25519_public_key,
                    activation_height,
                })
            }
            1 => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(ValidatorAdminOp::Remove {
                    validator_id,
                    activation_height,
                })
            }
            2 => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(ValidatorAdminOp::Activate {
                    validator_id,
                    activation_height,
                })
            }
            3 => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(ValidatorAdminOp::Deactivate {
                    validator_id,
                    activation_height,
                })
            }
            _ => Err(OperationError::UnknownValidatorAdminTag(va_tag)),
        }
    }
}

impl OperationPayload {
    fn op_type(&self) -> OperationType {
        match self {
            OperationPayload::Transaction { .. } => OperationType::Transaction,
            OperationPayload::AddressTransaction { .. } => OperationType::AddressTransaction,
            OperationPayload::ChangeKey { .. } => OperationType::ChangeKey,
            OperationPayload::RecoverFounds { .. } => OperationType::RecoverFounds,
            OperationPayload::ListAccountForSale { .. } => OperationType::ListAccountForSale,
            OperationPayload::DelistAccount { .. } => OperationType::DelistAccount,
            OperationPayload::BuyAccount { .. } => OperationType::BuyAccount,
            OperationPayload::ChangeKeySigned { .. } => OperationType::ChangeKeySigned,
            OperationPayload::ChangeAccountInfo { .. } => OperationType::ChangeAccountInfo,
            OperationPayload::MultiOperation { .. } => OperationType::MultiOperation,
            OperationPayload::Data { .. } => OperationType::Data,
            OperationPayload::ValidatorAdmin(_) => OperationType::ValidatorAdminOp,
            OperationPayload::CreateAccount { .. } => OperationType::CreateAccount,
            OperationPayload::GiftAccount { .. } => OperationType::GiftAccount,
            OperationPayload::AcceptGift { .. } => OperationType::AcceptGift,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u8(&mut buf, self.op_type() as u8);
        match self {
            OperationPayload::Transaction {
                senders,
                receivers,
                changers,
                fee,
            } => {
                write_u32(&mut buf, senders.len() as u32);
                for s in senders {
                    write_sender(&mut buf, s);
                }
                write_u32(&mut buf, receivers.len() as u32);
                for r in receivers {
                    write_receiver(&mut buf, r);
                }
                write_u32(&mut buf, changers.len() as u32);
                for c in changers {
                    write_changer(&mut buf, c);
                }
                write_u64(&mut buf, *fee);
            }
            OperationPayload::AddressTransaction {
                senders,
                receivers,
                fee,
            } => {
                write_u32(&mut buf, senders.len() as u32);
                for sender in senders {
                    write_sender(&mut buf, sender);
                }
                write_u32(&mut buf, receivers.len() as u32);
                for receiver in receivers {
                    write_fixed(&mut buf, &receiver.address.hash);
                    match receiver.address.public_key {
                        Some(public_key) => {
                            write_u8(&mut buf, 1);
                            write_fixed(&mut buf, &public_key);
                        }
                        None => write_u8(&mut buf, 0),
                    }
                    write_u64(&mut buf, receiver.amount);
                    write_bytes_with_len(&mut buf, &receiver.payload);
                }
                write_u64(&mut buf, *fee);
            }
            OperationPayload::ChangeKey {
                account,
                n_operation,
                fee,
                new_ed25519_public_key,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
                write_fixed(&mut buf, new_ed25519_public_key);
            }
            OperationPayload::RecoverFounds { account } => {
                write_u64(&mut buf, *account);
            }
            OperationPayload::ListAccountForSale {
                account,
                n_operation,
                sale_price,
                account_to_pay,
                new_ed25519_public_key,
                locked_until_block,
                fee,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *sale_price);
                write_u64(&mut buf, *account_to_pay);
                write_fixed(&mut buf, new_ed25519_public_key);
                write_u64(&mut buf, *locked_until_block);
                write_u64(&mut buf, *fee);
            }
            OperationPayload::DelistAccount {
                account,
                n_operation,
                fee,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
            }
            OperationPayload::BuyAccount {
                buyer_account,
                n_operation,
                account_to_purchase,
                amount,
                fee,
                new_ed25519_public_key,
                seller_account,
            } => {
                write_u64(&mut buf, *buyer_account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *account_to_purchase);
                write_u64(&mut buf, *amount);
                write_u64(&mut buf, *fee);
                write_fixed(&mut buf, new_ed25519_public_key);
                write_u64(&mut buf, *seller_account);
            }
            OperationPayload::ChangeKeySigned {
                account,
                n_operation,
                fee,
                new_ed25519_public_key,
                new_signature,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
                write_fixed(&mut buf, new_ed25519_public_key);
                write_sig(&mut buf, new_signature);
            }
            OperationPayload::ChangeAccountInfo {
                account,
                n_operation,
                fee,
                new_ed25519_public_key,
                new_name,
                new_type,
                new_account_data,
                new_account_seal,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
                write_fixed(&mut buf, new_ed25519_public_key);
                write_optional_str(&mut buf, new_name);
                write_u16(&mut buf, *new_type);
                write_bytes_with_len(&mut buf, new_account_data);
                write_bytes_with_len(&mut buf, new_account_seal);
            }
            OperationPayload::MultiOperation {
                senders,
                receivers,
                changers,
                fee,
            } => {
                write_u32(&mut buf, senders.len() as u32);
                for s in senders {
                    write_sender(&mut buf, s);
                }
                write_u32(&mut buf, receivers.len() as u32);
                for r in receivers {
                    write_receiver(&mut buf, r);
                }
                write_u32(&mut buf, changers.len() as u32);
                for c in changers {
                    write_changer(&mut buf, c);
                }
                write_u64(&mut buf, *fee);
            }
            OperationPayload::Data {
                account,
                n_operation,
                fee,
                data,
                senders,
                receivers,
                changers,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
                write_bytes_with_len(&mut buf, data);
                write_u32(&mut buf, senders.len() as u32);
                for s in senders {
                    write_sender(&mut buf, s);
                }
                write_u32(&mut buf, receivers.len() as u32);
                for r in receivers {
                    write_receiver(&mut buf, r);
                }
                write_u32(&mut buf, changers.len() as u32);
                for c in changers {
                    write_changer(&mut buf, c);
                }
            }
            OperationPayload::ValidatorAdmin(op) => buf.extend_from_slice(&op.to_bytes()),
            OperationPayload::CreateAccount {
                account_number,
                pubkey,
                initial_metadata,
            } => {
                write_u64(&mut buf, *account_number);
                write_fixed(&mut buf, pubkey);
                write_bytes_with_len(&mut buf, initial_metadata);
            }
            OperationPayload::GiftAccount {
                account,
                n_operation,
                recipient_public_key,
                fee,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_fixed(&mut buf, recipient_public_key);
                write_u64(&mut buf, *fee);
            }
            OperationPayload::AcceptGift {
                account,
                n_operation,
                fee,
            } => {
                write_u64(&mut buf, *account);
                write_u64(&mut buf, *n_operation);
                write_u64(&mut buf, *fee);
            }
        }
        buf
    }

    fn from_bytes(data: &[u8], pos: &mut usize) -> Result<Self, OperationError> {
        let op_tag = read_u8(data, pos)?;
        match op_tag {
            0x01 => {
                let sender_count = read_u32(data, pos)? as usize;
                let mut senders = Vec::with_capacity(sender_count);
                for _ in 0..sender_count {
                    senders.push(read_sender(data, pos)?);
                }
                let receiver_count = read_u32(data, pos)? as usize;
                let mut receivers = Vec::with_capacity(receiver_count);
                for _ in 0..receiver_count {
                    receivers.push(read_receiver(data, pos)?);
                }
                let changer_count = read_u32(data, pos)? as usize;
                let mut changers = Vec::with_capacity(changer_count);
                for _ in 0..changer_count {
                    changers.push(read_changer(data, pos)?);
                }
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::Transaction {
                    senders,
                    receivers,
                    changers,
                    fee,
                })
            }
            0x0F => {
                let sender_count = read_u32(data, pos)? as usize;
                let mut senders = Vec::with_capacity(sender_count);
                for _ in 0..sender_count {
                    senders.push(read_sender(data, pos)?);
                }
                let receiver_count = read_u32(data, pos)? as usize;
                let mut receivers = Vec::with_capacity(receiver_count);
                for _ in 0..receiver_count {
                    let hash = read_fixed(data, pos)?;
                    let public_key = if read_u8(data, pos)? == 1 {
                        Some(read_fixed(data, pos)?)
                    } else {
                        None
                    };
                    let amount = read_u64(data, pos)?;
                    let payload = read_bytes_with_len(data, pos)?;
                    receivers.push(AddressReceiverInfo {
                        address: AddressHash { hash, public_key },
                        amount,
                        payload,
                    });
                }
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::AddressTransaction {
                    senders,
                    receivers,
                    fee,
                })
            }
            0x02 => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                let new_ed25519_public_key = read_fixed(data, pos)?;
                Ok(OperationPayload::ChangeKey {
                    account,
                    n_operation,
                    fee,
                    new_ed25519_public_key,
                })
            }
            0x03 => {
                let account = read_u64(data, pos)?;
                Ok(OperationPayload::RecoverFounds { account })
            }
            0x04 => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let sale_price = read_u64(data, pos)?;
                let account_to_pay = read_u64(data, pos)?;
                let new_ed25519_public_key = read_fixed(data, pos)?;
                let locked_until_block = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::ListAccountForSale {
                    account,
                    n_operation,
                    sale_price,
                    account_to_pay,
                    new_ed25519_public_key,
                    locked_until_block,
                    fee,
                })
            }
            0x05 => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::DelistAccount {
                    account,
                    n_operation,
                    fee,
                })
            }
            0x06 => {
                let buyer_account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let account_to_purchase = read_u64(data, pos)?;
                let amount = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                let new_ed25519_public_key = read_fixed(data, pos)?;
                let seller_account = read_u64(data, pos)?;
                Ok(OperationPayload::BuyAccount {
                    buyer_account,
                    n_operation,
                    account_to_purchase,
                    amount,
                    fee,
                    new_ed25519_public_key,
                    seller_account,
                })
            }
            0x07 => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                let new_ed25519_public_key = read_fixed(data, pos)?;
                let new_signature = read_sig(data, pos)?;
                Ok(OperationPayload::ChangeKeySigned {
                    account,
                    n_operation,
                    fee,
                    new_ed25519_public_key,
                    new_signature,
                })
            }
            0x08 => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                let new_ed25519_public_key = read_fixed(data, pos)?;
                let new_name = read_optional_str(data, pos)?;
                let new_type = read_u16(data, pos)?;
                let new_account_data = read_bytes_with_len(data, pos)?;
                let new_account_seal = read_bytes_with_len(data, pos)?;
                Ok(OperationPayload::ChangeAccountInfo {
                    account,
                    n_operation,
                    fee,
                    new_ed25519_public_key,
                    new_name,
                    new_type,
                    new_account_data,
                    new_account_seal,
                })
            }
            0x09 => {
                let sender_count = read_u32(data, pos)? as usize;
                let mut senders = Vec::with_capacity(sender_count);
                for _ in 0..sender_count {
                    senders.push(read_sender(data, pos)?);
                }
                let receiver_count = read_u32(data, pos)? as usize;
                let mut receivers = Vec::with_capacity(receiver_count);
                for _ in 0..receiver_count {
                    receivers.push(read_receiver(data, pos)?);
                }
                let changer_count = read_u32(data, pos)? as usize;
                let mut changers = Vec::with_capacity(changer_count);
                for _ in 0..changer_count {
                    changers.push(read_changer(data, pos)?);
                }
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::MultiOperation {
                    senders,
                    receivers,
                    changers,
                    fee,
                })
            }
            0x0A => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                let op_data = read_bytes_with_len(data, pos)?;
                let sender_count = read_u32(data, pos)? as usize;
                let mut senders = Vec::with_capacity(sender_count);
                for _ in 0..sender_count {
                    senders.push(read_sender(data, pos)?);
                }
                let receiver_count = read_u32(data, pos)? as usize;
                let mut receivers = Vec::with_capacity(receiver_count);
                for _ in 0..receiver_count {
                    receivers.push(read_receiver(data, pos)?);
                }
                let changer_count = read_u32(data, pos)? as usize;
                let mut changers = Vec::with_capacity(changer_count);
                for _ in 0..changer_count {
                    changers.push(read_changer(data, pos)?);
                }
                Ok(OperationPayload::Data {
                    account,
                    n_operation,
                    fee,
                    data: op_data,
                    senders,
                    receivers,
                    changers,
                })
            }
            0x0B => {
                let op = ValidatorAdminOp::from_bytes(data, pos)?;
                Ok(OperationPayload::ValidatorAdmin(op))
            }
            0x0C => {
                let account_number = read_u64(data, pos)?;
                let pubkey = read_fixed(data, pos)?;
                let initial_metadata = read_bytes_with_len(data, pos)?;
                Ok(OperationPayload::CreateAccount {
                    account_number,
                    pubkey,
                    initial_metadata,
                })
            }
            0x0D => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let recipient_public_key = read_fixed(data, pos)?;
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::GiftAccount {
                    account,
                    n_operation,
                    recipient_public_key,
                    fee,
                })
            }
            0x0E => {
                let account = read_u64(data, pos)?;
                let n_operation = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                Ok(OperationPayload::AcceptGift {
                    account,
                    n_operation,
                    fee,
                })
            }
            _ => Err(OperationError::UnknownOperationType(op_tag)),
        }
    }
}

impl Operation {
    pub fn op_type(&self) -> OperationType {
        self.payload.op_type()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = self.to_bytes_stripped();
        write_u32(&mut buf, self.signatures.len() as u32);
        for sig in &self.signatures {
            write_sig(&mut buf, sig);
        }
        buf
    }

    pub fn to_bytes_stripped(&self) -> Vec<u8> {
        let mut buf = self.payload.to_bytes();
        write_u64(&mut buf, self.chain_id);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, OperationError> {
        let mut pos = 0;
        let payload = OperationPayload::from_bytes(data, &mut pos)?;
        let chain_id = read_u64(data, &mut pos)?;
        let sig_count = read_u32(data, &mut pos)? as usize;
        let mut signatures = Vec::with_capacity(sig_count);
        for _ in 0..sig_count {
            signatures.push(read_sig(data, &mut pos)?);
        }
        Ok(Operation {
            op_type: payload.op_type(),
            payload,
            chain_id,
            signatures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_sig() -> HybridSignature {
        Ed25519Signature {
            bytes: [7u8; ED25519_SIG_LEN],
        }
    }

    fn roundtrip(op: &Operation) {
        let bytes = op.to_bytes();
        let restored = Operation::from_bytes(&bytes).unwrap();
        assert_eq!(op.payload, restored.payload);
        assert_eq!(op.signatures.len(), restored.signatures.len());
    }

    #[test]
    fn transaction_roundtrip() {
        let op = Operation {
            payload: OperationPayload::Transaction {
                senders: vec![SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![1, 2, 3],
                }],
                receivers: vec![ReceiverInfo {
                    account: 2,
                    amount: 90,
                    payload: vec![],
                }],
                changers: vec![],
                fee: 10,
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::Transaction,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn address_transaction_roundtrip() {
        let public_key = [9u8; 32];
        let op = Operation {
            payload: OperationPayload::AddressTransaction {
                senders: vec![SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 100,
                    payload: vec![],
                }],
                receivers: vec![AddressReceiverInfo {
                    address: augecoin_crypto::address::AddressHash::from_public_key(public_key),
                    amount: 90,
                    payload: vec![1, 2],
                }],
                fee: 10,
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::AddressTransaction,
            chain_id: 1,
        };
        roundtrip(&op);
        assert_eq!(
            Operation::from_bytes(&op.to_bytes()).unwrap().op_type,
            OperationType::AddressTransaction
        );
    }

    #[test]
    fn change_key_roundtrip() {
        let op = Operation {
            payload: OperationPayload::ChangeKey {
                account: 1,
                n_operation: 0,
                fee: 10,
                new_ed25519_public_key: [1u8; 32],
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::ChangeKey,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn list_account_for_sale_roundtrip() {
        let op = Operation {
            payload: OperationPayload::ListAccountForSale {
                account: 1,
                n_operation: 0,
                sale_price: 5000,
                account_to_pay: 10,
                new_ed25519_public_key: [1u8; 32],
                locked_until_block: 1000,
                fee: 10,
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::ListAccountForSale,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn buy_account_roundtrip() {
        let op = Operation {
            payload: OperationPayload::BuyAccount {
                buyer_account: 2,
                n_operation: 0,
                account_to_purchase: 1,
                amount: 5000,
                fee: 10,
                new_ed25519_public_key: [3u8; 32],
                seller_account: 10,
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::BuyAccount,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn multi_operation_roundtrip() {
        let op = Operation {
            payload: OperationPayload::MultiOperation {
                senders: vec![SenderInfo {
                    account: 1,
                    n_operation: 0,
                    amount: 200,
                    payload: vec![],
                }],
                receivers: vec![
                    ReceiverInfo {
                        account: 2,
                        amount: 100,
                        payload: vec![],
                    },
                    ReceiverInfo {
                        account: 3,
                        amount: 80,
                        payload: vec![5, 6],
                    },
                ],
                changers: vec![ChangerInfo {
                    account: 1,
                    n_operation: 0,
                    new_ed25519_public_key: [1u8; 32],
                    new_name: None,
                    new_type: 0,
                    new_account_data: vec![],
                    new_account_seal: vec![],
                    fee: 0,
                }],
                fee: 20,
            },
            signatures: vec![dummy_sig(), dummy_sig()],
            op_type: OperationType::MultiOperation,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn data_operation_roundtrip() {
        let op = Operation {
            payload: OperationPayload::Data {
                account: 1,
                n_operation: 0,
                fee: 10,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                senders: vec![],
                receivers: vec![],
                changers: vec![],
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::Data,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn create_account_roundtrip() {
        let op = Operation {
            payload: OperationPayload::CreateAccount {
                account_number: 154650,
                pubkey: [9u8; 32],
                initial_metadata: vec![1, 2, 3, 4],
            },
            signatures: vec![dummy_sig()],
            op_type: OperationType::CreateAccount,
            chain_id: 1,
        };
        roundtrip(&op);
    }

    #[test]
    fn malformed_operation_rejected() {
        let result = Operation::from_bytes(&[0u8; 3]);
        assert!(result.is_err());
    }
}
