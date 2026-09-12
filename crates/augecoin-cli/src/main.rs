use clap::{Parser, Subcommand, ValueHint};

mod commands;
mod output;
mod rpc;

#[derive(Parser)]
#[command(
    name = "augecoin-cli",
    about = "AUGECOIN command-line interface",
    version,
    long_about = "CLI for the AUGECOIN blockchain. Manage validators, query accounts, blocks, and network status."
)]
pub struct Cli {
    #[arg(
        long,
        default_value = "https://localhost:9443",
        value_hint = ValueHint::Url,
        global = true
    )]
    pub endpoint: String,

    #[arg(long, global = true)]
    pub mnemonic: Option<String>,

    #[arg(long, value_hint = ValueHint::FilePath, global = true)]
    pub key_file: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Validator {
        #[command(subcommand)]
        action: ValidatorAction,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Security {
        #[command(subcommand)]
        action: SecurityAction,
    },
    GetAccount {
        account_number: u64,
        #[arg(long)]
        json: bool,
    },
    GetBlock {
        block_number: u64,
        #[arg(long)]
        json: bool,
    },
    SendOperation {
        hex: String,
    },
    SendTransfer {
        /// Sender account number (must be owned by the provided key)
        #[arg(long)]
        from: u64,
        /// Destination account number
        #[arg(long, conflicts_with = "to_address")]
        to: Option<u64>,
        /// Destination AUGE address (self-describing addresses auto-activate
        /// a new on-chain account on first receive)
        #[arg(long)]
        to_address: Option<String>,
        /// Amount in augesat (1 AUGE = 100000000 augesat)
        #[arg(long)]
        amount: u64,
        /// Fee in augesat (default: minimum 1000)
        #[arg(long)]
        fee: Option<u64>,
        /// n_operation to use (default: query the node)
        #[arg(long)]
        n_operation: Option<u64>,
        /// Target chain id (default: 1 = mainnet)
        #[arg(long)]
        chain_id: Option<u64>,
        /// File with the raw private key hex (first 32 bytes = seed).
        /// Accepts plain hex or AUGECOIN_VALIDATOR_KEY_HEX=<hex> style files.
        #[arg(long)]
        key_hex_file: String,
    },
    /// Gift a Reserved AUGEID to a recipient Ed25519 public key. The recipient
    /// activates it by signing an AcceptGift with the recipient key.
    GiftAccount {
        /// Reserved AUGEID to gift away
        #[arg(long)]
        account: u64,
        /// Recipient Ed25519 public key hex (32 bytes)
        #[arg(long)]
        recipient_public_key: String,
        /// Fee in augesat (default: 0 — Reserved accounts hold no balance)
        #[arg(long)]
        fee: Option<u64>,
        /// n_operation to use (default: query the node)
        #[arg(long)]
        n_operation: Option<u64>,
        /// Target chain id (default: 1 = mainnet)
        #[arg(long)]
        chain_id: Option<u64>,
        /// File with the raw private key hex of the AUGEID owner (first
        /// 32 bytes = seed). Accepts plain hex or KEY=<hex> style files.
        #[arg(long)]
        key_hex_file: String,
    },
    GetPendings {
        #[arg(long)]
        json: bool,
    },
    FindAccounts {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        r#type: Option<u16>,
        #[arg(long)]
        min_balance: Option<u64>,
        #[arg(long)]
        max_balance: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Contract {
        #[command(subcommand)]
        action: ContractAction,
    },
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
}

#[derive(Subcommand)]
pub enum ValidatorAction {
    List {
        #[arg(long)]
        json: bool,
    },
    Add {
        #[arg(long)]
        ed25519_key: String,
        #[arg(long)]
        activation_height: u64,
    },
    Remove {
        #[arg(long)]
        id: u64,
        #[arg(long)]
        activation_height: u64,
    },
    Activate {
        #[arg(long)]
        id: u64,
        #[arg(long)]
        activation_height: u64,
    },
    Deactivate {
        #[arg(long)]
        id: u64,
        #[arg(long)]
        activation_height: u64,
    },
    Earnings {
        #[arg(long)]
        id: Option<u64>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// SafeBox inventory (Reserved/ForSale/Owned AUGEIDs) of a validator key.
    Inventory {
        /// Ed25519 public key hex of the validator/leader account
        #[arg(long)]
        public_key: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum SecurityAction {
    Equivocations {
        #[arg(long)]
        json: bool,
    },
    BannedPeers {
        #[arg(long)]
        json: bool,
    },
    Alerts {
        #[arg(long)]
        tail: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ContractAction {
    Store {
        /// WASM bytecode hex (e.g. the auge20.wasm contents)
        #[arg(long)]
        wasm_hex: String,
        /// Admin API key (must match node AUGECOIN_ADMIN_API_KEYS)
        #[arg(long)]
        api_key: String,
        /// Gas limit hex (optional; defaults to chain DEFAULT_GAS_LIMIT)
        #[arg(long)]
        gas_limit_hex: Option<String>,
        /// Emit raw JSON instead of pretty output
        #[arg(long)]
        json: bool,
    },
    Create {
        /// Code id (u64, big-endian). AUGE20 = 1
        #[arg(long, default_value_t = 1)]
        code_id: u64,
        /// Token name
        #[arg(long)]
        name: String,
        /// Token symbol
        #[arg(long)]
        symbol: String,
        /// Token decimals
        #[arg(long, default_value_t = 8)]
        decimals: u8,
        /// Initial supply (whole tokens)
        #[arg(long)]
        initial_supply: u64,
        /// Max supply (whole tokens)
        #[arg(long)]
        max_supply: u64,
        /// Enable minting
        #[arg(long, default_value_t = true)]
        mint_enabled: bool,
        /// Enable burning
        #[arg(long, default_value_t = true)]
        burn_enabled: bool,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        gas_limit_hex: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Info {
        /// Contract id hex (32 bytes)
        #[arg(long)]
        contract_id_hex: String,
        #[arg(long)]
        json: bool,
    },
    Balance {
        /// Contract id hex (32 bytes)
        #[arg(long)]
        contract_id_hex: String,
        /// Owner account number
        #[arg(long)]
        address: u64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum TokenAction {
    Send {
        /// Contract id hex (32 bytes)
        #[arg(long)]
        contract_id_hex: String,
        /// Recipient account number
        #[arg(long)]
        to: u64,
        /// Amount (whole tokens)
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        gas_limit_hex: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Mint {
        #[arg(long)]
        contract_id_hex: String,
        #[arg(long)]
        to: u64,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        gas_limit_hex: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Burn {
        #[arg(long)]
        contract_id_hex: String,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        gas_limit_hex: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Info {
        /// Contract id hex (32 bytes)
        #[arg(long)]
        contract_id_hex: String,
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new().unwrap();

    let result = rt.block_on(async {
        match &cli.command {
            Commands::Status { json } => commands::handle_status(&cli, *json).await,
            Commands::Validator { action } => match action {
                ValidatorAction::List { json } => {
                    commands::handle_validator_list(&cli, *json).await
                }
                ValidatorAction::Add {
                    ed25519_key,
                    activation_height,
                } => commands::handle_validator_add(&cli, ed25519_key, *activation_height).await,
                ValidatorAction::Remove {
                    id,
                    activation_height,
                } => commands::handle_validator_remove(&cli, *id, *activation_height).await,
                ValidatorAction::Activate {
                    id,
                    activation_height,
                } => commands::handle_validator_activate(&cli, *id, *activation_height).await,
                ValidatorAction::Deactivate {
                    id,
                    activation_height,
                } => commands::handle_validator_deactivate(&cli, *id, *activation_height).await,
                ValidatorAction::Earnings { id, all, json } => {
                    commands::handle_validator_earnings(&cli, *id, *all, *json).await
                }
                ValidatorAction::Inventory { public_key, json } => {
                    commands::handle_validator_inventory(&cli, public_key, *json).await
                }
            },
            Commands::Security { action } => match action {
                SecurityAction::Equivocations { json } => {
                    commands::handle_security_equivocations(&cli, *json).await
                }
                SecurityAction::BannedPeers { json } => {
                    commands::handle_security_banned_peers(&cli, *json).await
                }
                SecurityAction::Alerts { tail, json } => {
                    commands::handle_security_alerts(&cli, *tail, *json).await
                }
            },
            Commands::GetAccount {
                account_number,
                json,
            } => commands::handle_get_account(&cli, *account_number, *json).await,
            Commands::GetBlock { block_number, json } => {
                commands::handle_get_block(&cli, *block_number, *json).await
            }
            Commands::SendOperation { hex } => commands::handle_send_operation(&cli, hex).await,
            Commands::GiftAccount {
                account,
                recipient_public_key,
                fee,
                n_operation,
                chain_id,
                key_hex_file,
            } => {
                commands::handle_gift_account(
                    &cli,
                    commands::GiftAccountArgs {
                        account: *account,
                        recipient_public_key: recipient_public_key.clone(),
                        fee: *fee,
                        n_operation: *n_operation,
                        chain_id: *chain_id,
                        key_hex_file: key_hex_file.clone(),
                    },
                )
                .await
            }
            Commands::SendTransfer {
                from,
                to,
                to_address,
                amount,
                fee,
                n_operation,
                chain_id,
                key_hex_file,
            } => {
                commands::handle_send_transfer(
                    &cli,
                    commands::TransferArgs {
                        from: *from,
                        to: *to,
                        to_address: to_address.clone(),
                        amount: *amount,
                        fee: *fee,
                        n_operation: *n_operation,
                        chain_id: *chain_id,
                        key_hex_file: key_hex_file.clone(),
                    },
                )
                .await
            }
            Commands::GetPendings { json } => commands::handle_get_pendings(&cli, *json).await,
            Commands::FindAccounts {
                name,
                r#type,
                min_balance,
                max_balance,
                json,
            } => {
                commands::handle_find_accounts(
                    &cli,
                    name.clone(),
                    *r#type,
                    *min_balance,
                    *max_balance,
                    *json,
                )
                .await
            }
            Commands::Contract { action } => match action {
                ContractAction::Store {
                    wasm_hex,
                    api_key,
                    gas_limit_hex,
                    json,
                } => {
                    commands::handle_contract_store(
                        &cli,
                        wasm_hex.clone(),
                        api_key.clone(),
                        gas_limit_hex.clone(),
                        *json,
                    )
                    .await
                }
                ContractAction::Create {
                    code_id,
                    name,
                    symbol,
                    decimals,
                    initial_supply,
                    max_supply,
                    mint_enabled,
                    burn_enabled,
                    api_key,
                    gas_limit_hex,
                    json,
                } => {
                    commands::handle_contract_create(
                        &cli,
                        *code_id,
                        name.clone(),
                        symbol.clone(),
                        *decimals,
                        *initial_supply,
                        *max_supply,
                        *mint_enabled,
                        *burn_enabled,
                        api_key.clone(),
                        gas_limit_hex.clone(),
                        *json,
                    )
                    .await
                }
                ContractAction::Info {
                    contract_id_hex,
                    json,
                } => commands::handle_contract_info(&cli, contract_id_hex.clone(), *json).await,
                ContractAction::Balance {
                    contract_id_hex,
                    address,
                    json,
                } => {
                    commands::handle_contract_balance(
                        &cli,
                        contract_id_hex.clone(),
                        *address,
                        *json,
                    )
                    .await
                }
            },
            Commands::Token { action } => match action {
                TokenAction::Send {
                    contract_id_hex,
                    to,
                    amount,
                    api_key,
                    gas_limit_hex,
                    json,
                } => {
                    commands::handle_token_send(
                        &cli,
                        contract_id_hex.clone(),
                        *to,
                        *amount,
                        api_key.clone(),
                        gas_limit_hex.clone(),
                        *json,
                    )
                    .await
                }
                TokenAction::Mint {
                    contract_id_hex,
                    to,
                    amount,
                    api_key,
                    gas_limit_hex,
                    json,
                } => {
                    commands::handle_token_mint(
                        &cli,
                        contract_id_hex.clone(),
                        *to,
                        *amount,
                        api_key.clone(),
                        gas_limit_hex.clone(),
                        *json,
                    )
                    .await
                }
                TokenAction::Burn {
                    contract_id_hex,
                    amount,
                    api_key,
                    gas_limit_hex,
                    json,
                } => {
                    commands::handle_token_burn(
                        &cli,
                        contract_id_hex.clone(),
                        *amount,
                        api_key.clone(),
                        gas_limit_hex.clone(),
                        *json,
                    )
                    .await
                }
                TokenAction::Info {
                    contract_id_hex,
                    json,
                } => commands::handle_token_info(&cli, contract_id_hex.clone(), *json).await,
            },
        }
    });

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod cli_parsing_tests {
    use super::*;

    #[test]
    fn validator_list_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "validator", "list"]).unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::List { json } => assert!(!json),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn validator_list_json_flag_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "validator", "list", "--json"]).unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::List { json } => assert!(json),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn validator_add_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "validator",
            "add",
            "--ed25519-key",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--activation-height",
            "100",
        ])
        .unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Add {
                ed25519_key,
                activation_height,
            } => {
                assert_eq!(
                    ed25519_key,
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                );
                assert_eq!(activation_height, 100);
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn validator_remove_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "validator",
            "remove",
            "--id",
            "3",
            "--activation-height",
            "500",
        ])
        .unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Remove {
                id,
                activation_height,
            } => {
                assert_eq!(id, 3);
                assert_eq!(activation_height, 500);
            }
            _ => panic!("expected Remove"),
        }
    }

    #[test]
    fn validator_activate_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "validator",
            "activate",
            "--id",
            "2",
            "--activation-height",
            "600",
        ])
        .unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Activate {
                id,
                activation_height,
            } => {
                assert_eq!(id, 2);
                assert_eq!(activation_height, 600);
            }
            _ => panic!("expected Activate"),
        }
    }

    #[test]
    fn validator_deactivate_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "validator",
            "deactivate",
            "--id",
            "1",
            "--activation-height",
            "700",
        ])
        .unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Deactivate {
                id,
                activation_height,
            } => {
                assert_eq!(id, 1);
                assert_eq!(activation_height, 700);
            }
            _ => panic!("expected Deactivate"),
        }
    }

    #[test]
    fn endpoint_flag_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "--endpoint",
            "https://my-node:9443",
            "validator",
            "list",
        ])
        .unwrap();
        assert_eq!(cli.endpoint, "https://my-node:9443");
    }

    #[test]
    fn mnemonic_flag_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "--mnemonic",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "validator",
            "list",
        ])
        .unwrap();
        assert!(cli.mnemonic.is_some());
    }

    #[test]
    fn help_output_contains_commands() {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        let help = cmd.render_help().to_string();
        assert!(help.contains("validator"));
        assert!(help.contains("status"));
        assert!(help.contains("security"));
        assert!(help.contains("get-account"));
        assert!(help.contains("get-block"));
        assert!(help.contains("send-operation"));
        assert!(help.contains("get-pendings"));
        assert!(help.contains("find-accounts"));
        assert!(help.contains("contract"));
        assert!(help.contains("token"));
        assert!(help.contains("AUGECOIN"));
    }

    #[test]
    fn default_endpoint_is_localhost_tls() {
        let cli = Cli::try_parse_from(["augecoin-cli", "validator", "list"]).unwrap();
        assert!(cli.endpoint.contains("localhost"));
        assert!(cli.endpoint.starts_with("https://"));
    }

    #[test]
    fn status_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "status"]).unwrap();
        let Commands::Status { json } = cli.command else {
            panic!()
        };
        assert!(!json);
    }

    #[test]
    fn status_command_with_json_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "status", "--json"]).unwrap();
        let Commands::Status { json } = cli.command else {
            panic!()
        };
        assert!(json);
    }

    #[test]
    fn validator_earnings_all_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "validator", "earnings", "--all"]).unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Earnings { id, all, json } => {
                assert!(all);
                assert!(id.is_none());
                assert!(!json);
            }
            _ => panic!("expected Earnings"),
        }
    }

    #[test]
    fn validator_earnings_by_id_parses() {
        let cli =
            Cli::try_parse_from(["augecoin-cli", "validator", "earnings", "--id", "5"]).unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Earnings { id, all, json } => {
                assert_eq!(id, Some(5));
                assert!(!all);
                assert!(!json);
            }
            _ => panic!("expected Earnings"),
        }
    }

    #[test]
    fn validator_earnings_with_json_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "validator", "earnings", "--all", "--json"])
            .unwrap();
        let Commands::Validator { action } = cli.command else {
            panic!()
        };
        match action {
            ValidatorAction::Earnings { id, all, json } => {
                assert!(all);
                assert!(json);
                assert!(id.is_none());
            }
            _ => panic!("expected Earnings"),
        }
    }

    #[test]
    fn security_equivocations_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "security", "equivocations"]).unwrap();
        let Commands::Security { action } = cli.command else {
            panic!()
        };
        match action {
            SecurityAction::Equivocations { json } => assert!(!json),
            _ => panic!("expected Equivocations"),
        }
    }

    #[test]
    fn security_banned_peers_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "security", "banned-peers"]).unwrap();
        let Commands::Security { action } = cli.command else {
            panic!()
        };
        match action {
            SecurityAction::BannedPeers { json } => assert!(!json),
            _ => panic!("expected BannedPeers"),
        }
    }

    #[test]
    fn security_alerts_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "security", "alerts"]).unwrap();
        let Commands::Security { action } = cli.command else {
            panic!()
        };
        match action {
            SecurityAction::Alerts { tail, json } => {
                assert!(!tail);
                assert!(!json);
            }
            _ => panic!("expected Alerts"),
        }
    }

    #[test]
    fn security_alerts_tail_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "security", "alerts", "--tail"]).unwrap();
        let Commands::Security { action } = cli.command else {
            panic!()
        };
        match action {
            SecurityAction::Alerts { tail, json } => {
                assert!(tail);
                assert!(!json);
            }
            _ => panic!("expected Alerts"),
        }
    }

    #[test]
    fn security_equivocations_json_parses() {
        let cli =
            Cli::try_parse_from(["augecoin-cli", "security", "equivocations", "--json"]).unwrap();
        let Commands::Security { action } = cli.command else {
            panic!()
        };
        match action {
            SecurityAction::Equivocations { json } => assert!(json),
            _ => panic!("expected Equivocations"),
        }
    }

    #[test]
    fn get_account_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-account", "42"]).unwrap();
        let Commands::GetAccount {
            account_number,
            json,
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(account_number, 42);
        assert!(!json);
    }

    #[test]
    fn get_account_json_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-account", "7", "--json"]).unwrap();
        let Commands::GetAccount {
            account_number,
            json,
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(account_number, 7);
        assert!(json);
    }

    #[test]
    fn get_block_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-block", "100"]).unwrap();
        let Commands::GetBlock { block_number, json } = cli.command else {
            panic!()
        };
        assert_eq!(block_number, 100);
        assert!(!json);
    }

    #[test]
    fn get_block_json_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-block", "200", "--json"]).unwrap();
        let Commands::GetBlock { block_number, json } = cli.command else {
            panic!()
        };
        assert_eq!(block_number, 200);
        assert!(json);
    }

    #[test]
    fn send_operation_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "send-operation", "aabbccdd"]).unwrap();
        let Commands::SendOperation { hex } = cli.command else {
            panic!()
        };
        assert_eq!(hex, "aabbccdd");
    }

    #[test]
    fn get_pendings_command_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-pendings"]).unwrap();
        let Commands::GetPendings { json } = cli.command else {
            panic!()
        };
        assert!(!json);
    }

    #[test]
    fn get_pendings_json_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "get-pendings", "--json"]).unwrap();
        let Commands::GetPendings { json } = cli.command else {
            panic!()
        };
        assert!(json);
    }

    #[test]
    fn find_accounts_no_filters_parses() {
        let cli = Cli::try_parse_from(["augecoin-cli", "find-accounts"]).unwrap();
        let Commands::FindAccounts {
            name,
            r#type,
            min_balance,
            max_balance,
            json,
        } = cli.command
        else {
            panic!()
        };
        assert!(name.is_none());
        assert!(r#type.is_none());
        assert!(min_balance.is_none());
        assert!(max_balance.is_none());
        assert!(!json);
    }

    #[test]
    fn find_accounts_all_filters_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "find-accounts",
            "--name",
            "alice",
            "--type",
            "5",
            "--min-balance",
            "1000",
            "--max-balance",
            "999999",
            "--json",
        ])
        .unwrap();
        let Commands::FindAccounts {
            name,
            r#type,
            min_balance,
            max_balance,
            json,
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(name, Some("alice".to_string()));
        assert_eq!(r#type, Some(5));
        assert_eq!(min_balance, Some(1000));
        assert_eq!(max_balance, Some(999999));
        assert!(json);
    }

    #[test]
    fn contract_store_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "contract",
            "store",
            "--wasm-hex",
            "0061736d",
            "--api-key",
            "admin-key",
        ])
        .unwrap();
        let Commands::Contract { action } = cli.command else {
            panic!()
        };
        match action {
            ContractAction::Store {
                wasm_hex,
                api_key,
                gas_limit_hex,
                json,
            } => {
                assert_eq!(wasm_hex, "0061736d");
                assert_eq!(api_key, "admin-key");
                assert!(gas_limit_hex.is_none());
                assert!(!json);
            }
            _ => panic!("expected Store"),
        }
    }

    #[test]
    fn contract_create_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "contract",
            "create",
            "--code-id",
            "1",
            "--name",
            "Auge",
            "--symbol",
            "AUG",
            "--decimals",
            "8",
            "--initial-supply",
            "1000000",
            "--max-supply",
            "2000000",
            "--api-key",
            "admin-key",
        ])
        .unwrap();
        let Commands::Contract { action } = cli.command else {
            panic!()
        };
        match action {
            ContractAction::Create {
                code_id,
                name,
                symbol,
                decimals,
                initial_supply,
                max_supply,
                mint_enabled,
                burn_enabled,
                api_key,
                gas_limit_hex,
                json,
            } => {
                assert_eq!(code_id, 1);
                assert_eq!(name, "Auge");
                assert_eq!(symbol, "AUG");
                assert_eq!(decimals, 8);
                assert_eq!(initial_supply, 1_000_000);
                assert_eq!(max_supply, 2_000_000);
                assert!(mint_enabled);
                assert!(burn_enabled);
                assert_eq!(api_key, "admin-key");
                assert!(gas_limit_hex.is_none());
                assert!(!json);
            }
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn contract_info_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "contract",
            "info",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--json",
        ])
        .unwrap();
        let Commands::Contract { action } = cli.command else {
            panic!()
        };
        match action {
            ContractAction::Info {
                contract_id_hex,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert!(json);
            }
            _ => panic!("expected Info"),
        }
    }

    #[test]
    fn contract_balance_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "contract",
            "balance",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--address",
            "42",
        ])
        .unwrap();
        let Commands::Contract { action } = cli.command else {
            panic!()
        };
        match action {
            ContractAction::Balance {
                contract_id_hex,
                address,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert_eq!(address, 42);
                assert!(!json);
            }
            _ => panic!("expected Balance"),
        }
    }

    #[test]
    fn token_send_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "token",
            "send",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--to",
            "7",
            "--amount",
            "500",
            "--api-key",
            "admin-key",
        ])
        .unwrap();
        let Commands::Token { action } = cli.command else {
            panic!()
        };
        match action {
            TokenAction::Send {
                contract_id_hex,
                to,
                amount,
                api_key,
                gas_limit_hex,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert_eq!(to, 7);
                assert_eq!(amount, 500);
                assert_eq!(api_key, "admin-key");
                assert!(gas_limit_hex.is_none());
                assert!(!json);
            }
            _ => panic!("expected Send"),
        }
    }

    #[test]
    fn token_mint_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "token",
            "mint",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--to",
            "7",
            "--amount",
            "1000",
            "--api-key",
            "admin-key",
        ])
        .unwrap();
        let Commands::Token { action } = cli.command else {
            panic!()
        };
        match action {
            TokenAction::Mint {
                contract_id_hex,
                to,
                amount,
                api_key,
                gas_limit_hex,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert_eq!(to, 7);
                assert_eq!(amount, 1000);
                assert_eq!(api_key, "admin-key");
                assert!(gas_limit_hex.is_none());
                assert!(!json);
            }
            _ => panic!("expected Mint"),
        }
    }

    #[test]
    fn token_burn_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "token",
            "burn",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--amount",
            "250",
            "--api-key",
            "admin-key",
        ])
        .unwrap();
        let Commands::Token { action } = cli.command else {
            panic!()
        };
        match action {
            TokenAction::Burn {
                contract_id_hex,
                amount,
                api_key,
                gas_limit_hex,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert_eq!(amount, 250);
                assert_eq!(api_key, "admin-key");
                assert!(gas_limit_hex.is_none());
                assert!(!json);
            }
            _ => panic!("expected Burn"),
        }
    }

    #[test]
    fn token_info_command_parses() {
        let cli = Cli::try_parse_from([
            "augecoin-cli",
            "token",
            "info",
            "--contract-id-hex",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--json",
        ])
        .unwrap();
        let Commands::Token { action } = cli.command else {
            panic!()
        };
        match action {
            TokenAction::Info {
                contract_id_hex,
                json,
            } => {
                assert_eq!(
                    contract_id_hex,
                    "0000000000000000000000000000000000000000000000000000000000000001"
                );
                assert!(json);
            }
            _ => panic!("expected Info"),
        }
    }
}
