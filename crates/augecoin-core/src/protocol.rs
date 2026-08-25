use crate::constants::{
    CT_BUILD_PROTOCOL, CT_MAX_PROTOCOL, CT_PROTOCOL_2_ACTIVATION, CT_PROTOCOL_3_ACTIVATION,
    CT_PROTOCOL_4_ACTIVATION, CT_PROTOCOL_5_ACTIVATION, CT_PROTOCOL_6_ACTIVATION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolInfo {
    pub protocol_version: u16,
    pub protocol_available: u16,
    pub activation_block: u64,
}

impl ProtocolInfo {
    pub fn new() -> Self {
        ProtocolInfo {
            protocol_version: CT_BUILD_PROTOCOL,
            protocol_available: CT_MAX_PROTOCOL,
            activation_block: 0,
        }
    }

    #[allow(clippy::absurd_extreme_comparisons)]
    pub fn is_protocol_active(&self, protocol: u16, at_block: u64) -> bool {
        match protocol {
            1 => true,
            2 => at_block >= CT_PROTOCOL_2_ACTIVATION,
            3 => at_block >= CT_PROTOCOL_3_ACTIVATION,
            4 => at_block >= CT_PROTOCOL_4_ACTIVATION,
            5 => at_block >= CT_PROTOCOL_5_ACTIVATION,
            6 => at_block >= CT_PROTOCOL_6_ACTIVATION,
            _ => false,
        }
    }

    pub fn available_protocol(&self) -> u16 {
        self.protocol_available
    }

    pub fn current_protocol(&self) -> u16 {
        self.protocol_version
    }
}

impl Default for ProtocolInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_1_always_active() {
        let p = ProtocolInfo::new();
        assert!(p.is_protocol_active(1, 0));
        assert!(p.is_protocol_active(1, 999999999));
    }

    #[test]
    fn protocol_upgrades_per_activation_block() {
        let p = ProtocolInfo::new();
        assert_eq!(p.current_protocol(), CT_BUILD_PROTOCOL);
        assert!(p.is_protocol_active(5, 0));
        assert!(!p.is_protocol_active(6, 0));
    }
}
