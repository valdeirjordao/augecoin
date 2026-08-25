use augecoin_core::operation::{Operation, OperationPayload, ValidatorAdminOp};
use augecoin_crypto::signature::HybridPublicKey;
use thiserror::Error;

const ED25519_PK_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorStatus {
    PendingActivation { activation_height: u64 },
    Active,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorInfo {
    pub id: u64,
    pub ed25519_public_key: [u8; ED25519_PK_LEN],
    pub status: ValidatorStatus,
}

#[derive(Debug, Clone)]
pub struct ValidatorSet {
    admin_public_key: HybridPublicKey,
    validators: Vec<ValidatorInfo>,
    next_validator_id: u64,
}

#[derive(Debug, Error)]
pub enum ValidatorSetError {
    #[error("invalid admin signature")]
    InvalidAdminSignature,
    #[error("validator already exists")]
    ValidatorAlreadyExists,
    #[error("validator not found")]
    ValidatorNotFound,
    #[error("no active validators")]
    NoActiveValidators,
    #[error("cannot remove last active validator")]
    CannotRemoveLastActive,
    #[error("{0}")]
    InvalidActivation(String),
}

impl ValidatorSet {
    pub fn new(admin_public_key: HybridPublicKey, initial_validators: Vec<ValidatorInfo>) -> Self {
        let mut next_id = 0u64;
        for v in &initial_validators {
            if v.id >= next_id {
                next_id = v.id + 1;
            }
        }
        ValidatorSet {
            admin_public_key,
            validators: initial_validators,
            next_validator_id: next_id,
        }
    }

    pub fn admin_public_key(&self) -> &HybridPublicKey {
        &self.admin_public_key
    }

    pub fn validators(&self) -> &[ValidatorInfo] {
        &self.validators
    }

    pub fn active_validators(&self) -> Vec<&ValidatorInfo> {
        self.validators
            .iter()
            .filter(|v| v.status == ValidatorStatus::Active)
            .collect()
    }

    pub fn leader_for_height(&self, height: u64) -> Result<&ValidatorInfo, ValidatorSetError> {
        let active = self.active_validators();
        if active.is_empty() {
            return Err(ValidatorSetError::NoActiveValidators);
        }
        let index = (height as usize) % active.len();
        Ok(active[index])
    }

    pub fn apply_validator_admin(
        &mut self,
        op: &Operation,
        current_height: u64,
    ) -> Result<(), ValidatorSetError> {
        let va_op = match &op.payload {
            OperationPayload::ValidatorAdmin(op) => op,
            _ => return Err(ValidatorSetError::InvalidAdminSignature),
        };

        let message = op.to_bytes_stripped();
        let admin_pk = &self.admin_public_key;

        let all_ok = op
            .signatures
            .iter()
            .any(|sig| sig.verify(admin_pk, &message));
        if !all_ok {
            return Err(ValidatorSetError::InvalidAdminSignature);
        }

        match va_op {
            ValidatorAdminOp::Add {
                ed25519_public_key,
                activation_height,
                ..
            } => {
                let id = self.next_validator_id;
                self.next_validator_id += 1;

                let status = if *activation_height <= current_height {
                    ValidatorStatus::Active
                } else {
                    ValidatorStatus::PendingActivation {
                        activation_height: *activation_height,
                    }
                };

                self.validators.push(ValidatorInfo {
                    id,
                    ed25519_public_key: *ed25519_public_key,
                    status,
                });
            }
            ValidatorAdminOp::Remove {
                validator_id,
                activation_height,
                ..
            } => {
                if *activation_height > current_height {
                    return Err(ValidatorSetError::InvalidActivation(
                        "Remove operation requires activation_height <= current_height".to_string(),
                    ));
                }
                if !self.validators.iter().any(|v| v.id == *validator_id) {
                    return Err(ValidatorSetError::ValidatorNotFound);
                }
                if self.active_validators().len() <= 1
                    && self
                        .validators
                        .iter()
                        .any(|v| v.id == *validator_id && v.status == ValidatorStatus::Active)
                {
                    return Err(ValidatorSetError::CannotRemoveLastActive);
                }
                self.validators.retain(|v| v.id != *validator_id);
            }
            ValidatorAdminOp::Activate {
                validator_id,
                activation_height,
                ..
            } => {
                let v = self
                    .validators
                    .iter_mut()
                    .find(|v| v.id == *validator_id)
                    .ok_or(ValidatorSetError::ValidatorNotFound)?;
                if *activation_height <= current_height {
                    v.status = ValidatorStatus::Active;
                } else {
                    v.status = ValidatorStatus::PendingActivation {
                        activation_height: *activation_height,
                    };
                }
            }
            ValidatorAdminOp::Deactivate {
                validator_id,
                activation_height,
                ..
            } => {
                if *activation_height > current_height {
                    return Err(ValidatorSetError::InvalidActivation(
                        "Deactivate operation requires activation_height <= current_height"
                            .to_string(),
                    ));
                }
                let active_count = self.active_validators().len();
                let v = self
                    .validators
                    .iter_mut()
                    .find(|v| v.id == *validator_id)
                    .ok_or(ValidatorSetError::ValidatorNotFound)?;
                let is_last_active = v.status == ValidatorStatus::Active && active_count <= 1;
                if is_last_active {
                    return Err(ValidatorSetError::CannotRemoveLastActive);
                }
                v.status = ValidatorStatus::Inactive;
            }
        }

        Ok(())
    }

    pub fn process_pending_activations(&mut self, current_height: u64) {
        for v in &mut self.validators {
            if let ValidatorStatus::PendingActivation { activation_height } = v.status {
                if current_height >= activation_height {
                    v.status = ValidatorStatus::Active;
                }
            }
        }
    }

    pub fn add_validator_direct(
        &mut self,
        ed25519_public_key: [u8; ED25519_PK_LEN],
        activation_height: u64,
        current_height: u64,
    ) -> u64 {
        let id = self.next_validator_id;
        self.next_validator_id += 1;

        let status = if activation_height <= current_height {
            ValidatorStatus::Active
        } else {
            ValidatorStatus::PendingActivation { activation_height }
        };

        self.validators.push(ValidatorInfo {
            id,
            ed25519_public_key,
            status,
        });

        id
    }

    pub fn remove_validator_direct(&mut self, validator_id: u64) -> Result<(), ValidatorSetError> {
        if self.active_validators().len() <= 1
            && self
                .validators
                .iter()
                .any(|v| v.id == validator_id && v.status == ValidatorStatus::Active)
        {
            return Err(ValidatorSetError::CannotRemoveLastActive);
        }
        let before = self.validators.len();
        self.validators.retain(|v| v.id != validator_id);
        if self.validators.len() == before {
            return Err(ValidatorSetError::ValidatorNotFound);
        }
        Ok(())
    }

    pub fn set_validator_status_direct(
        &mut self,
        validator_id: u64,
        status: ValidatorStatus,
    ) -> Result<(), ValidatorSetError> {
        let v = self
            .validators
            .iter_mut()
            .find(|v| v.id == validator_id)
            .ok_or(ValidatorSetError::ValidatorNotFound)?;
        v.status = status;
        Ok(())
    }
}

impl ValidatorInfo {
    pub fn new_active(id: u64, ed25519_public_key: [u8; ED25519_PK_LEN]) -> Self {
        ValidatorInfo {
            id,
            ed25519_public_key,
            status: ValidatorStatus::Active,
        }
    }

    fn status_tag(&self) -> u8 {
        match &self.status {
            ValidatorStatus::Active => 0,
            ValidatorStatus::Inactive => 1,
            ValidatorStatus::PendingActivation { .. } => 2,
        }
    }
}

impl ValidatorSet {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let admin_ed = self.admin_public_key.to_bytes();
        buf.extend_from_slice(&admin_ed);

        buf.extend_from_slice(&(self.validators.len() as u32).to_be_bytes());
        for v in &self.validators {
            buf.extend_from_slice(&v.id.to_be_bytes());
            buf.extend_from_slice(&v.ed25519_public_key);
            buf.push(v.status_tag());
            if let ValidatorStatus::PendingActivation { activation_height } = v.status {
                buf.extend_from_slice(&activation_height.to_be_bytes());
            }
        }
        buf.extend_from_slice(&self.next_validator_id.to_be_bytes());
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let min_len = 32 + 4 + 8;
        if data.len() < min_len {
            return Err("data too short".to_string());
        }
        let mut pos = 0;

        let ed_bytes: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
        pos += 32;

        let admin_public_key = ed25519_dalek::VerifyingKey::from_bytes(&ed_bytes)
            .map_err(|e| format!("invalid ed25519 key: {e}"))?;

        if pos + 4 > data.len() {
            return Err("data too short for validator count".to_string());
        }
        let vcount =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;

        let mut validators = Vec::with_capacity(vcount);
        for _ in 0..vcount {
            if pos + 8 + 32 + 1 > data.len() {
                return Err("data too short for validator".to_string());
            }
            let id = u64::from_be_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]);
            pos += 8;
            let ed: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
            pos += 32;
            if pos >= data.len() {
                return Err("data too short for status tag".to_string());
            }
            let status_tag = data[pos];
            pos += 1;
            let status = match status_tag {
                0 => ValidatorStatus::Active,
                1 => ValidatorStatus::Inactive,
                2 => {
                    if pos + 8 > data.len() {
                        return Err("data too short for activation_height".to_string());
                    }
                    let activation_height = u64::from_be_bytes([
                        data[pos],
                        data[pos + 1],
                        data[pos + 2],
                        data[pos + 3],
                        data[pos + 4],
                        data[pos + 5],
                        data[pos + 6],
                        data[pos + 7],
                    ]);
                    pos += 8;
                    ValidatorStatus::PendingActivation { activation_height }
                }
                _ => return Err(format!("unknown validator status tag: {status_tag}")),
            };
            validators.push(ValidatorInfo {
                id,
                ed25519_public_key: ed,
                status,
            });
        }

        if pos + 8 > data.len() {
            return Err("data too short for next_validator_id".to_string());
        }
        let next_validator_id = u64::from_be_bytes([
            data[pos],
            data[pos + 1],
            data[pos + 2],
            data[pos + 3],
            data[pos + 4],
            data[pos + 5],
            data[pos + 6],
            data[pos + 7],
        ]);

        Ok(ValidatorSet {
            admin_public_key,
            validators,
            next_validator_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use augecoin_core::operation::{Operation, OperationPayload, OperationType, ValidatorAdminOp};
    use augecoin_crypto::signature::{HybridKeyPair, HybridSignature};

    fn admin_keypair() -> HybridKeyPair {
        HybridKeyPair::generate()
    }

    fn dummy_sig() -> HybridSignature {
        HybridSignature { bytes: [0u8; 64] }
    }

    fn dummy_pubkey_bytes() -> [u8; 32] {
        [0xAAu8; 32]
    }

    fn make_validator(id: u64) -> ValidatorInfo {
        let ed = dummy_pubkey_bytes();
        ValidatorInfo::new_active(id, ed)
    }

    fn make_set(admin_kp: &HybridKeyPair) -> ValidatorSet {
        let validators: Vec<ValidatorInfo> = (0..4).map(make_validator).collect();
        ValidatorSet::new(admin_kp.verifying_key(), validators)
    }

    fn make_admin_op(kp: &HybridKeyPair, va_op: ValidatorAdminOp) -> Operation {
        let mut op = Operation {
            op_type: OperationType::ValidatorAdminOp,
            payload: OperationPayload::ValidatorAdmin(va_op),
            chain_id: 1,
            signatures: vec![],
        };
        let signature = kp.sign(&op.to_bytes_stripped());
        op.signatures.push(signature);
        op
    }

    #[test]
    fn leader_for_height_is_deterministic() {
        let admin = admin_keypair();
        let set = make_set(&admin);
        let l1 = set.leader_for_height(0).unwrap();
        let l2 = set.leader_for_height(4).unwrap();
        assert_eq!(l1.id, l2.id);
    }

    #[test]
    fn leader_rotates() {
        let admin = admin_keypair();
        let set = make_set(&admin);
        let l0 = set.leader_for_height(0).unwrap().id;
        let l1 = set.leader_for_height(1).unwrap().id;
        let l2 = set.leader_for_height(2).unwrap().id;
        let l3 = set.leader_for_height(3).unwrap().id;
        assert_ne!(l0, l1);
        assert_ne!(l1, l2);
        assert_ne!(l2, l3);
    }

    #[test]
    fn add_validator_with_future_activation() {
        let admin = admin_keypair();
        let mut set = make_set(&admin);
        let ed = dummy_pubkey_bytes();

        let va_op = ValidatorAdminOp::Add {
            ed25519_public_key: ed,
            activation_height: 500,
        };
        let op = make_admin_op(&admin, va_op);
        set.apply_validator_admin(&op, 100).unwrap();

        assert_eq!(set.validators().len(), 5);
        let new_v = set.validators().last().unwrap();
        assert!(matches!(
            new_v.status,
            ValidatorStatus::PendingActivation {
                activation_height: 500
            }
        ));

        set.process_pending_activations(200);
        assert!(matches!(
            set.validators().last().unwrap().status,
            ValidatorStatus::PendingActivation { .. }
        ));

        set.process_pending_activations(500);
        assert_eq!(
            set.validators().last().unwrap().status,
            ValidatorStatus::Active
        );
    }

    #[test]
    fn remove_validator() {
        let admin = admin_keypair();
        let mut set = make_set(&admin);

        let va_op = ValidatorAdminOp::Remove {
            validator_id: 0,
            activation_height: 0,
        };
        let op = make_admin_op(&admin, va_op);
        set.apply_validator_admin(&op, 100).unwrap();
        assert_eq!(set.validators().len(), 3);
    }

    #[test]
    fn deactivate_then_activate() {
        let admin = admin_keypair();
        let mut set = make_set(&admin);

        let deact_op = make_admin_op(
            &admin,
            ValidatorAdminOp::Deactivate {
                validator_id: 2,
                activation_height: 0,
            },
        );
        set.apply_validator_admin(&deact_op, 100).unwrap();
        let active = set.active_validators();
        assert_eq!(active.len(), 3);

        let act_op = make_admin_op(
            &admin,
            ValidatorAdminOp::Activate {
                validator_id: 2,
                activation_height: 0,
            },
        );
        set.apply_validator_admin(&act_op, 100).unwrap();
        let active = set.active_validators();
        assert_eq!(active.len(), 4);
    }

    #[test]
    fn invalid_admin_signature_rejected() {
        let admin = admin_keypair();
        let wrong_admin = admin_keypair();
        let mut set = make_set(&admin);

        let mut op = Operation {
            op_type: OperationType::ValidatorAdminOp,
            payload: OperationPayload::ValidatorAdmin(ValidatorAdminOp::Add {
                ed25519_public_key: [0u8; 32],
                activation_height: 0,
            }),
            chain_id: 1,
            signatures: vec![],
        };
        op.signatures
            .push(wrong_admin.sign(&op.to_bytes_stripped()));

        let result = set.apply_validator_admin(&op, 100);
        assert!(result.is_err());
    }

    #[test]
    fn non_validator_admin_op_rejected() {
        let admin = admin_keypair();
        let mut set = make_set(&admin);

        let op = Operation {
            op_type: OperationType::Transaction,
            payload: OperationPayload::Transaction {
                senders: vec![],
                receivers: vec![],
                changers: vec![],
                fee: 0,
            },
            chain_id: 1,
            signatures: vec![dummy_sig()],
        };

        let result = set.apply_validator_admin(&op, 100);
        assert!(result.is_err());
    }
}
