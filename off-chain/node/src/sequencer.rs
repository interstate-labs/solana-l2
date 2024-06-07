use solana_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
use solana_ledger::blockstore_options::LedgerColumnOptions;
use solana_ledger::genesis_utils::create_genesis_config;
use solana_ledger::genesis_utils::GenesisConfigInfo;
use solana_runtime::bank::Bank;
use solana_sdk::genesis_config::GenesisConfig;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;
use std::collections::VecDeque;
use std::time::Duration;
use solana_rpc::rpc::JsonRpcConfig;


use solana_core::validator::Validator;
use solana_streamer::socket::SocketAddrSpace;
use std::process::exit;

use solana_core::validator::ValidatorConfig;
use solana_core::validator::ValidatorStartProgress;
use solana_gossip::{cluster_info::Node, legacy_contact_info::LegacyContactInfo as ContactInfo};
use solana_sdk::signer::EncodableKey;

use solana_ledger::blockstore::create_new_ledger;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

fn save_keypair(keypair: &Keypair, path: &Path) {
    keypair.write_to_file(path).unwrap();
}

fn load_keypair(path: &Path) -> Keypair {
    Keypair::read_from_file(path).unwrap()
}

// just wrapping a native solana transaction to add other fields
pub struct TxInfo {
    txn: Transaction,
    receipt: String,
}

pub struct Sequencer {
    inbox: Arc<Mutex<VecDeque<TxInfo>>>,

    // pub validator: Arc<Validator>,
    pub validator: Arc<Validator>,
}

// Sequencer txn ordering and execution
impl Sequencer {
    pub fn new() -> Self {
        let validator = setting_l2_env();
        Sequencer {
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            validator: validator,
        }
    }

    pub fn add_transaction(&self, tx: Transaction) -> Result<(), Box<dyn std::error::Error>> {
        println!("Adding txn to inbox .....");
        // adding txns to the inbox
        // TODO: Add pre transaction validation/ run preflights here
        // validate txns before adding to the inbox (maybe simulate?)
        let _res = self.execute_transaction(&tx).unwrap();

        // processing result and adding to the inbox
        let txn_info = TxInfo {
            txn: tx,
            receipt: "HOLDER".to_string(),
        };

        let mut inbox = self.inbox.lock().unwrap();
        inbox.push_back(txn_info);
        Ok(())
    }

    fn execute_transaction(&self, tx: &Transaction) -> Result<(), Box<dyn std::error::Error>> {
        println!("Executing txn ....");
        // getting bank
        let bank = self.validator.bank_forks.read().unwrap().working_bank();

        if bank.is_frozen() {
            // create new bank and insert to the bank_forks
            let new_bank = Bank::new_from_parent(bank.clone(), &bank.collector_id(), bank.slot() + 1);
            self.validator.bank_forks.write().unwrap().insert(new_bank);
        }
        
        // getting bank again and execute the transaction
        let bank = self.validator.bank_forks.read().unwrap().working_bank();

        bank.process_transaction(tx).unwrap_or_else(|e| {
            eprintln!("Transaction Execution Failed: {}", e);
        });

        Ok(())
    }
}

pub fn setting_l2_env() -> Arc<Validator> {
    // paths
    let ledger_path = Path::new("../ledger");
    let validator_keypair_path = Path::new("../keypairs/identity.json");
    let faucet_keypair_path = Path::new("../keypairs/faucet.json");
    let vote_keypair_path = Path::new("../keypairs/vote.json");

    // checking keypairs
    if !validator_keypair_path.exists() {
        let validator_keypair = Keypair::new();
        save_keypair(&validator_keypair, validator_keypair_path)
    }

    if !faucet_keypair_path.exists() {
        let faucet_keypair = Keypair::new();
        save_keypair(&faucet_keypair, faucet_keypair_path)
    }

    if !vote_keypair_path.exists() {
        let vote_keypair = Keypair::new();
        save_keypair(&vote_keypair, vote_keypair_path);
    }

    // reading keypairs
    let identity_keypair = Arc::new(load_keypair(validator_keypair_path));
    let faucet_keypair = load_keypair(faucet_keypair_path);
    let vote_keypair = load_keypair(vote_keypair_path);

    if !ledger_path.exists() {
        println!("Creating new ledger");
        let GenesisConfigInfo {
            genesis_config,
            mint_keypair,
            ..
        } = create_genesis_config(1_000_000_000_000);
        save_keypair(&mint_keypair, Path::new("../keypairs/mint.json"));
        create_new_ledger(
            ledger_path,
            &genesis_config,
            MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
            LedgerColumnOptions::default(),
        )
        .expect("Failed to create ledger");
    } else {
        println!("Using existing ledger");
    }

    // initializing node
    let node = Node::new_localhost_with_pubkey(&identity_keypair.pubkey());
    let start_progress = Arc::new(RwLock::new(ValidatorStartProgress::default()));

    let validator = Validator::new(
        node,
        identity_keypair.clone(),
        ledger_path,
        &vote_keypair.pubkey(),
        Arc::new(RwLock::new(vec![identity_keypair])),
        vec![],
        &ValidatorConfig {
            rpc_addrs: None, // Disable RPC services
            rpc_config: JsonRpcConfig {
                enable_rpc_transaction_history: false,
                ..JsonRpcConfig::default()
            },
            voting_disabled: true,
            ..Default::default()
        },
        true,
        None,
        start_progress.clone(),
        SocketAddrSpace::Unspecified,
        true,
        10,
        false,
        Arc::new(RwLock::new(None)),
    )
    .unwrap_or_else(|e| {
        eprintln!("Failed to start validator: {:?}", e);
        exit(1);
    });
    println!("Validator started.");

    Arc::new(validator)

}
