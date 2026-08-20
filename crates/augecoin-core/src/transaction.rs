use augecoin_crypto::signature::Ed25519Signature;
use thiserror::Error;

const ED25519_PK_LEN: usize = 32;
const ED25519_SIG_LEN: usize = 64;

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
pub enum Operation {
    Transfer {
        to: u64,
        amount: u64,
        fee: u64,
    },
    ChangeKey {
        new_ed25519_public_key: [u8; ED25519_PK_LEN],
    },
    ClaimAccount {
        fee: u64,
    },
    SetAccountName {
        name: String,
    },
    AttachData {
        data: Vec<u8>,
    },
    ValidatorAdmin(ValidatorAdminOp),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub sender: u64,
    pub op_sequence: u64,
    pub sig_scheme_version: u8,
    pub operation: Operation,
    pub signature: Ed25519Signature,
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("invalid serialized data: {0}")]
    InvalidSerialization(String),
    #[error("unknown operation tag: {0}")]
    UnknownOperationTag(u8),
    #[error("unknown validator admin operation tag: {0}")]
    UnknownValidatorAdminTag(u8),
}

const OP_TAG_TRANSFER: u8 = 0;
const OP_TAG_CHANGE_KEY: u8 = 1;
const OP_TAG_CLAIM_ACCOUNT: u8 = 2;
const OP_TAG_SET_ACCOUNT_NAME: u8 = 3;
const OP_TAG_ATTACH_DATA: u8 = 4;
const OP_TAG_VALIDATOR_ADMIN: u8 = 5;

const VA_TAG_ADD: u8 = 0;
const VA_TAG_REMOVE: u8 = 1;
const VA_TAG_ACTIVATE: u8 = 2;
const VA_TAG_DEACTIVATE: u8 = 3;

fn write_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

fn read_u64(data: &[u8], pos: &mut usize) -> Result<u64, TransactionError> {
    if *pos + 8 > data.len() {
        return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes: [u8; 8] = data[*pos..*pos + 8].try_into().unwrap();
    *pos += 8;
    Ok(u64::from_be_bytes(bytes))
}

fn write_u8(buf: &mut Vec<u8>, value: u8) {
    buf.push(value);
}

fn read_u8(data: &[u8], pos: &mut usize) -> Result<u8, TransactionError> {
    if *pos >= data.len() {
        return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
    }
    let value = data[*pos];
    *pos += 1;
    Ok(value)
}

fn write_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn read_bytes(data: &[u8], pos: &mut usize) -> Result<Vec<u8>, TransactionError> {
    if *pos + 4 > data.len() {
        return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
    }
    let len = u32::from_be_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]) as usize;
    *pos += 4;
    if *pos + len > data.len() {
        return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
    }
    let bytes = data[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(bytes)
}

fn write_fixed_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
}

fn read_fixed_bytes<const N: usize>(
    data: &[u8],
    pos: &mut usize,
) -> Result<[u8; N], TransactionError> {
    if *pos + N > data.len() {
        return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&data[*pos..*pos + N]);
    *pos += N;
    Ok(arr)
}

impl Operation {
    pub fn tag(&self) -> u8 {
        match self {
            Operation::Transfer { .. } => OP_TAG_TRANSFER,
            Operation::ChangeKey { .. } => OP_TAG_CHANGE_KEY,
            Operation::ClaimAccount { .. } => OP_TAG_CLAIM_ACCOUNT,
            Operation::SetAccountName { .. } => OP_TAG_SET_ACCOUNT_NAME,
            Operation::AttachData { .. } => OP_TAG_ATTACH_DATA,
            Operation::ValidatorAdmin(_) => OP_TAG_VALIDATOR_ADMIN,
        }
    }

    fn serialize_body(&self, buf: &mut Vec<u8>) {
        match self {
            Operation::Transfer { to, amount, fee } => {
                write_u64(buf, *to);
                write_u64(buf, *amount);
                write_u64(buf, *fee);
            }
            Operation::ChangeKey {
                new_ed25519_public_key,
            } => {
                write_fixed_bytes(buf, new_ed25519_public_key);
            }
            Operation::ClaimAccount { fee } => {
                write_u64(buf, *fee);
            }
            Operation::SetAccountName { name } => {
                let name_bytes = name.as_bytes();
                buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
                buf.extend_from_slice(name_bytes);
            }
            Operation::AttachData { data } => {
                write_bytes(buf, data);
            }
            Operation::ValidatorAdmin(op) => op.serialize_body(buf),
        }
    }

    fn deserialize_body(
        tag: u8,
        data: &[u8],
        pos: &mut usize,
    ) -> Result<Operation, TransactionError> {
        match tag {
            OP_TAG_TRANSFER => {
                let to = read_u64(data, pos)?;
                let amount = read_u64(data, pos)?;
                let fee = read_u64(data, pos)?;
                Ok(Operation::Transfer { to, amount, fee })
            }
            OP_TAG_CHANGE_KEY => {
                let new_ed25519_public_key = read_fixed_bytes(data, pos)?;
                Ok(Operation::ChangeKey {
                    new_ed25519_public_key,
                })
            }
            OP_TAG_CLAIM_ACCOUNT => {
                let fee = read_u64(data, pos)?;
                Ok(Operation::ClaimAccount { fee })
            }
            OP_TAG_SET_ACCOUNT_NAME => {
                if *pos + 2 > data.len() {
                    return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
                }
                let name_len = u16::from_be_bytes([data[*pos], data[*pos + 1]]) as usize;
                *pos += 2;
                if *pos + name_len > data.len() {
                    return Err(TransactionError::InvalidSerialization("unexpected EOF".into()));
                }
                let name =
                    String::from_utf8(data[*pos..*pos + name_len].to_vec()).map_err(|_| {
                        TransactionError::InvalidSerialization("invalid UTF-8 in name".into())
                    })?;
                *pos += name_len;
                Ok(Operation::SetAccountName { name })
            }
            OP_TAG_ATTACH_DATA => {
                let data = read_bytes(data, pos)?;
                Ok(Operation::AttachData { data })
            }
            OP_TAG_VALIDATOR_ADMIN => ValidatorAdminOp::deserialize(data, pos),
            _ => Err(TransactionError::UnknownOperationTag(tag)),
        }
    }
}

impl ValidatorAdminOp {
    pub fn tag(&self) -> u8 {
        match self {
            ValidatorAdminOp::Add { .. } => VA_TAG_ADD,
            ValidatorAdminOp::Remove { .. } => VA_TAG_REMOVE,
            ValidatorAdminOp::Activate { .. } => VA_TAG_ACTIVATE,
            ValidatorAdminOp::Deactivate { .. } => VA_TAG_DEACTIVATE,
        }
    }

    fn serialize_body(&self, buf: &mut Vec<u8>) {
        write_u8(buf, self.tag());
        match self {
            ValidatorAdminOp::Add {
                ed25519_public_key,
                activation_height,
            } => {
                write_fixed_bytes(buf, ed25519_public_key);
                write_u64(buf, *activation_height);
            }
            ValidatorAdminOp::Remove {
                validator_id,
                activation_height,
            } => {
                write_u64(buf, *validator_id);
                write_u64(buf, *activation_height);
            }
            ValidatorAdminOp::Activate {
                validator_id,
                activation_height,
            } => {
                write_u64(buf, *validator_id);
                write_u64(buf, *activation_height);
            }
            ValidatorAdminOp::Deactivate {
                validator_id,
                activation_height,
            } => {
                write_u64(buf, *validator_id);
                write_u64(buf, *activation_height);
            }
        }
    }

    fn deserialize(
        data: &[u8],
        pos: &mut usize,
    ) -> Result<Operation, TransactionError> {
        let va_tag = read_u8(data, pos)?;
        match va_tag {
            VA_TAG_ADD => {
                let ed25519_public_key = read_fixed_bytes(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(Operation::ValidatorAdmin(ValidatorAdminOp::Add {
                    ed25519_public_key,
                    activation_height,
                }))
            }
            VA_TAG_REMOVE => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(Operation::ValidatorAdmin(ValidatorAdminOp::Remove {
                    validator_id,
                    activation_height,
                }))
            }
            VA_TAG_ACTIVATE => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(Operation::ValidatorAdmin(ValidatorAdminOp::Activate {
                    validator_id,
                    activation_height,
                }))
            }
            VA_TAG_DEACTIVATE => {
                let validator_id = read_u64(data, pos)?;
                let activation_height = read_u64(data, pos)?;
                Ok(Operation::ValidatorAdmin(ValidatorAdminOp::Deactivate {
                    validator_id,
                    activation_height,
                }))
            }
            _ => Err(TransactionError::UnknownValidatorAdminTag(va_tag)),
        }
    }
}

impl Transaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.sender);
        write_u64(&mut buf, self.op_sequence);
        write_u8(&mut buf, self.sig_scheme_version);
        write_u8(&mut buf, self.operation.tag());
        self.operation.serialize_body(&mut buf);
        write_fixed_bytes(&mut buf, &self.signature.bytes);
        buf
    }

    pub fn to_bytes_stripped(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u64(&mut buf, self.sender);
        write_u64(&mut buf, self.op_sequence);
        write_u8(&mut buf, self.sig_scheme_version);
        write_u8(&mut buf, self.operation.tag());
        self.operation.serialize_body(&mut buf);
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, TransactionError> {
        let mut pos = 0;
        let sender = read_u64(data, &mut pos)?;
        let op_sequence = read_u64(data, &mut pos)?;
        let sig_scheme_version = read_u8(data, &mut pos)?;
        let op_tag = read_u8(data, &mut pos)?;
        let operation = Operation::deserialize_body(op_tag, data, &mut pos)?;

        let sig_bytes: [u8; ED25519_SIG_LEN] = read_fixed_bytes(data, &mut pos)?;
        let signature = Ed25519Signature { bytes: sig_bytes };

        Ok(Transaction {
            sender,
            op_sequence,
            sig_scheme_version,
            operation,
            signature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_signature() -> Ed25519Signature {
        Ed25519Signature {
            bytes: [7u8; ED25519_SIG_LEN],
        }
    }

    fn roundtrip(tx: &Transaction) {
        let bytes = tx.to_bytes();
        let restored = Transaction::from_bytes(&bytes).unwrap();
        assert_eq!(tx.sender, restored.sender);
        assert_eq!(tx.op_sequence, restored.op_sequence);
        assert_eq!(tx.sig_scheme_version, restored.sig_scheme_version);
        assert_eq!(tx.operation, restored.operation);
        assert_eq!(tx.signature.bytes, restored.signature.bytes);
    }

    #[test]
    fn transfer_roundtrip() {
        let tx = Transaction {
            sender: 1,
            op_sequence: 5,
            sig_scheme_version: 1,
            operation: Operation::Transfer {
                to: 2,
                amount: 1000,
                fee: 10,
            },
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn change_key_roundtrip() {
        let ed = [0xAAu8; ED25519_PK_LEN];
        let tx = Transaction {
            sender: 1,
            op_sequence: 1,
            sig_scheme_version: 1,
            operation: Operation::ChangeKey {
                new_ed25519_public_key: ed,
            },
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn claim_account_roundtrip() {
        let tx = Transaction {
            sender: 0,
            op_sequence: 0,
            sig_scheme_version: 1,
            operation: Operation::ClaimAccount { fee: 100 },
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn set_account_name_roundtrip() {
        let tx = Transaction {
            sender: 42,
            op_sequence: 3,
            sig_scheme_version: 1,
            operation: Operation::SetAccountName {
                name: "joao.auge".to_string(),
            },
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn attach_data_roundtrip() {
        let tx = Transaction {
            sender: 7,
            op_sequence: 2,
            sig_scheme_version: 1,
            operation: Operation::AttachData {
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
            },
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn validator_admin_add_roundtrip() {
        let ed = [0x11u8; ED25519_PK_LEN];
        let tx = Transaction {
            sender: 0,
            op_sequence: 0,
            sig_scheme_version: 1,
            operation: Operation::ValidatorAdmin(ValidatorAdminOp::Add {
                ed25519_public_key: ed,
                activation_height: 500,
            }),
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn validator_admin_remove_roundtrip() {
        let tx = Transaction {
            sender: 0,
            op_sequence: 0,
            sig_scheme_version: 1,
            operation: Operation::ValidatorAdmin(ValidatorAdminOp::Remove {
                validator_id: 3,
                activation_height: 600,
            }),
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn validator_admin_activate_roundtrip() {
        let tx = Transaction {
            sender: 0,
            op_sequence: 0,
            sig_scheme_version: 1,
            operation: Operation::ValidatorAdmin(ValidatorAdminOp::Activate {
                validator_id: 2,
                activation_height: 700,
            }),
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn validator_admin_deactivate_roundtrip() {
        let tx = Transaction {
            sender: 0,
            op_sequence: 0,
            sig_scheme_version: 1,
            operation: Operation::ValidatorAdmin(ValidatorAdminOp::Deactivate {
                validator_id: 1,
                activation_height: 800,
            }),
            signature: dummy_signature(),
        };
        roundtrip(&tx);
    }

    #[test]
    fn malformed_transaction_rejected() {
        let result = Transaction::from_bytes(&[0u8; 3]);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_operation_tag_rejected() {
        let mut bytes = Transaction {
            sender: 1,
            op_sequence: 1,
            sig_scheme_version: 1,
            operation: Operation::Transfer {
                to: 2,
                amount: 100,
                fee: 1,
            },
            signature: dummy_signature(),
        }
        .to_bytes();
        let op_tag_offset = 8 + 8 + 1;
        bytes[op_tag_offset] = 99;
        let result = Transaction::from_bytes(&bytes);
        assert!(result.is_err());
    }
}
