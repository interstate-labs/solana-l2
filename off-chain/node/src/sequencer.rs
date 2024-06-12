use crossbeam_channel::unbounded;
use solana_accounts_db::accounts_db::AccountShrinkThreshold;
use solana_accounts_db::accounts_db::AccountsDb;
use solana_accounts_db::accounts_db::AccountsDbConfig;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_accounts_db::accounts_index::AccountSecondaryIndexes;
use solana_accounts_db::accounts_index::AccountsIndexConfig;
use solana_accounts_db::accounts_index::IndexLimitMb;
use solana_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
use solana_accounts_db::partitioned_rewards::TestPartitionedEpochRewards;
use solana_accounts_db::utils::create_all_accounts_run_and_snapshot_dirs;
use solana_accounts_db::utils::create_and_canonicalize_directories;
use solana_core::banking_trace::BANKING_TRACE_DIR_DEFAULT_BYTE_LIMIT;
use solana_core::consensus::tower_storage::FileTowerStorage;
use solana_core::validator::is_snapshot_config_valid;
use solana_core::validator::BlockProductionMethod;
use solana_core::validator::BlockVerificationMethod;
use solana_ledger::blockstore_options::BlockstoreCompressionType;
use solana_ledger::blockstore_options::BlockstoreRecoveryMode;
use solana_ledger::blockstore_options::LedgerColumnOptions;
use solana_ledger::blockstore_options::ShredStorageType;
use solana_ledger::genesis_utils::create_genesis_config;
use solana_ledger::genesis_utils::GenesisConfigInfo;
use solana_ledger::use_snapshot_archives_at_startup::UseSnapshotArchivesAtStartup;
use solana_program_runtime::log_collector::log::info;
use solana_rpc::rpc::JsonRpcConfig;
use solana_rpc::rpc_pubsub_service::PubSubConfig;
use solana_runtime::bank::Bank;
use solana_runtime::snapshot_bank_utils::DEFAULT_INCREMENTAL_SNAPSHOT_ARCHIVE_INTERVAL_SLOTS;
use solana_runtime::snapshot_bank_utils::DISABLED_SNAPSHOT_ARCHIVE_INTERVAL;
use solana_runtime::snapshot_config::SnapshotConfig;
use solana_runtime::snapshot_config::SnapshotUsage;
use solana_runtime::snapshot_utils;
use solana_runtime::snapshot_utils::ArchiveFormat;
use solana_runtime::snapshot_utils::SnapshotVersion;
use solana_runtime::snapshot_utils::DEFAULT_ARCHIVE_COMPRESSION;
use solana_sdk::genesis_config::GenesisConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;
use solana_validator::admin_rpc_service;
use solana_validator::admin_rpc_service::StakedNodesOverrides;
use solana_validator::bootstrap;
use solana_validator::ledger_lockfile;
use solana_validator::lock_ledger;
use solana_validator::redirect_stderr_to_file;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fmt;
use std::fs;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

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
}

// Sequencer txn ordering and execution
impl Sequencer {
    pub fn new() -> Self {
        Sequencer {
            inbox: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn add_transaction(
        &self,
        validator: &Validator,
        tx: Transaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Adding txn to inbox .....");
        // adding txns to the inbox
        // TODO: Add pre transaction validation/ run preflights here
        // validate txns before adding to the inbox (maybe simulate?)
        let _res = self.execute_transaction(validator, &tx).unwrap();

        // processing result and adding to the inbox
        let txn_info = TxInfo {
            txn: tx,
            receipt: "HOLDER".to_string(),
        };

        let mut inbox = self.inbox.lock().unwrap();
        inbox.push_back(txn_info);
        Ok(())
    }

    fn execute_transaction(
        &self,
        validator: &Validator,
        tx: &Transaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Executing txn ....");
        // getting bank
        let bank = validator.bank_forks.read().unwrap().working_bank();

        if bank.is_frozen() {
            // create new bank and insert to the bank_forks
            let new_bank =
                Bank::new_from_parent(bank.clone(), &bank.collector_id(), bank.slot() + 1);
            validator.bank_forks.write().unwrap().insert(new_bank);
        }

        // getting bank again and execute the transaction
        let bank = validator.bank_forks.read().unwrap().working_bank();

        bank.process_transaction(tx).unwrap_or_else(|e| {
            eprintln!("Transaction Execution Failed: {}", e);
        });

        Ok(())
    }
}

pub fn setting_l2_env() -> Validator {
    // paths
    let ledger_path = Path::new("./ledger");
    let validator_keypair_path = Path::new("./keypairs/identity.json");
    let faucet_keypair_path = Path::new("./keypairs/faucet.json");
    let vote_keypair_path = Path::new("./keypairs/vote.json");

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

    // setting up logs
    let logfile = format!("validator-log-{}.log", identity_keypair.pubkey());
    let _logger_thread = redirect_stderr_to_file(Some(logfile));

    // start logs
    info!("{} {}", "Sequencer", "0.0.0");
    info!("Starting sequencer with: {:#?}", std::env::args_os());

    // reporting features to the logs
    solana_core::validator::report_target_features();

    // auth keypairs
    let authorized_voter_keypairs = vec![identity_keypair.clone()];
    let authorized_voter_keypairs = Arc::new(RwLock::new(authorized_voter_keypairs));

    // staked nodes
    let staked_nodes_overrides =
        Arc::new(RwLock::new(StakedNodesOverrides::default().staked_map_id));

    //skiping init_complete_file

    // rpc bootstrap config
    let rpc_bootstrap_config = bootstrap::RpcBootstrapConfig {
        no_genesis_fetch: false,
        no_snapshot_fetch: false,
        check_vote_account: None,
        only_known_rpc: false,
        max_genesis_archive_unpacked_size: 10 * 1024 * 1024, //10 MiB
        incremental_snapshot_fetch: true,
    };

    let private_rpc = false;
    let do_port_check = true;
    let tpu_coalesce = Duration::from_millis(5);
    let wal_recovery_mode = Some(BlockstoreRecoveryMode::AbsoluteConsistency);

    // checking and creating ledger
    if !ledger_path.exists() {
        println!("Creating new ledger");
        let GenesisConfigInfo {
            genesis_config,
            mint_keypair,
            ..
        } = create_genesis_config(1_000_000_000_000);
        save_keypair(&mint_keypair, Path::new("./keypairs/mint.json"));
        save_keypair(&Keypair::new(), Path::new("./keypairs/receiver.json"));
        create_new_ledger(
            &ledger_path,
            &genesis_config,
            MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
            LedgerColumnOptions::default(),
        )
        .expect("Failed to create ledger");
        println!("new ledger created");
    } else {
        println!("Using existing ledger");
    }

    // Canonicalize ledger path to avoid issues with symlink creation
    let ledger_path = create_and_canonicalize_directories([&ledger_path])
        .unwrap_or_else(|err| {
            eprintln!(
                "Unable to access ledger path '{}': {err}",
                ledger_path.display(),
            );
            exit(1);
        })
        .pop()
        .unwrap();

    // Accounts hash cache path
    let accounts_hash_cache_path = ledger_path.join(AccountsDb::DEFAULT_ACCOUNTS_HASH_CACHE_DIR);
    let accounts_hash_cache_path = create_and_canonicalize_directories([&accounts_hash_cache_path])
        .unwrap_or_else(|err| {
            eprintln!(
                "Unable to access accounts hash cache path '{}': {err}",
                accounts_hash_cache_path.display(),
            );
            exit(1);
        })
        .pop()
        .unwrap();

    // skiping debug_keys, known_validator, repair_validators, repair_whitelist, gosip_whitelist

    let bind_address = solana_net_utils::parse_host("0.0.0.0").expect("invalid bind_address");
    let rpc_bind_address = if private_rpc {
        solana_net_utils::parse_host("127.0.0.1").unwrap()
    } else {
        bind_address
    };

    // skip contact_debug_interval

    // process account indexes
    let account_indexes = {
        let account_indexes: HashSet<AccountIndex> =
            ["program-id", "spl-token-owner", "spl-token-mint"]
                .map(|value| match value {
                    "program-id" => AccountIndex::ProgramId,
                    "spl-token-mint" => AccountIndex::SplTokenMint,
                    "spl-token-owner" => AccountIndex::SplTokenOwner,
                    _ => unreachable!(),
                })
                .into();

        AccountSecondaryIndexes {
            keys: None,
            indexes: account_indexes,
        }
    };

    let restricted_repair_only_mode = true;
    let accounts_shrink_optimize_total_space = true;
    let tpu_use_quic = false;
    let tpu_enable_udp = false;
    let tpu_connection_pool_size = 4; // default
    let shrink_ratio = 0.8;
    let accounts_shrink_ratio = if accounts_shrink_optimize_total_space {
        AccountShrinkThreshold::TotalSpace { shrink_ratio }
    } else {
        AccountShrinkThreshold::IndividualStore { shrink_ratio }
    };

    // skip entrypoint_addrs, expected_shred_version.

    let tower_storage = Arc::new(FileTowerStorage::new(ledger_path.clone()));

    let mut accounts_index_config = AccountsIndexConfig {
        started_from_validator: true, // this is the only place this is set
        ..AccountsIndexConfig::default()
    };

    let test_partitioned_epoch_rewards = TestPartitionedEpochRewards::None;
    accounts_index_config.index_limit_mb = IndexLimitMb::Unspecified;

    {
        let mut accounts_index_paths: Vec<PathBuf> = vec![];
        if accounts_index_paths.is_empty() {
            accounts_index_paths = vec![ledger_path.join("accounts_index")];
        }
        accounts_index_config.drives = Some(accounts_index_paths);
    }

    let account_shrink_paths: Option<Vec<PathBuf>> = Some(vec![]);
    let account_shrink_paths = account_shrink_paths.as_ref().map(|paths| {
        create_and_canonicalize_directories(paths).unwrap_or_else(|err| {
            eprintln!("Unable to access account shrink path: {err}");
            exit(1);
        })
    });

    let (account_shrink_run_paths, account_shrink_snapshot_paths) = account_shrink_paths
        .map(|paths| {
            create_all_accounts_run_and_snapshot_dirs(&paths).unwrap_or_else(|err| {
                eprintln!("Error: {err}");
                exit(1);
            })
        })
        .unzip();

    let accounts_db_config = AccountsDbConfig {
        index: Some(accounts_index_config),
        base_working_path: Some(ledger_path.clone()),
        accounts_hash_cache_path: Some(accounts_hash_cache_path),
        // shrink_paths: account_shrink_run_paths,
        // write_cache_limit_bytes: value_t!(matches, "accounts_db_cache_limit_mb", u64)
        //     .ok()
        //     .map(|mb| mb * MB as u64),
        // ancient_append_vec_offset: value_t!(matches, "accounts_db_ancient_append_vecs", i64).ok(),
        // exhaustively_verify_refcounts: matches.is_present("accounts_db_verify_refcounts"),
        // create_ancient_storage: matches
        //     .is_present("accounts_db_create_ancient_storage_packed")
        //     .then_some(CreateAncientStorage::Pack)
        //     .unwrap_or_default(),
        test_partitioned_epoch_rewards,
        test_skip_rewrites_but_include_in_bank_hash: false,
        ..AccountsDbConfig::default()
    };

    let accounts_db_config = Some(accounts_db_config);

    // skip on_start_geyser_plugin_config_files

    let starting_with_geyser_plugins: bool = false;

    // skip rpc_bigtable_config
    let rpc_send_retry_rate_ms = 2_000u64;
    let rpc_send_batch_size = 1 as usize;
    let rpc_send_batch_send_rate_ms = 1u64;

    const MILLIS_PER_SECOND: u64 = 1000;

    let tps = rpc_send_batch_size as u64 * MILLIS_PER_SECOND / rpc_send_batch_send_rate_ms;

    // skip rpc_send_transaction_tpu_peers,
    let rpc_send_transaction_also_leader = false;
    let leader_forward_count = 2u64;
    let full_api = false;

    // validator config
    let mut validator_config = ValidatorConfig {
        require_tower: false,
        tower_storage,
        rpc_config: JsonRpcConfig {
            enable_rpc_transaction_history: true,
            full_api,
            disable_health_check: false,
            account_indexes: account_indexes.clone(),
            ..JsonRpcConfig::default()
        },
        pubsub_config: PubSubConfig {
            enable_block_subscription: false,
            enable_vote_subscription: false,
            max_active_subscriptions: 1_000_000,
            queue_capacity_items: 10_000_000,
            queue_capacity_bytes: 256 * 1024 * 1024,
            worker_threads: 4,
            notification_threads: Some(1),
        },
        voting_disabled: true,
        wal_recovery_mode,
        run_verification: false,
        poh_hashes_per_batch: 64,
        account_indexes,
        accounts_db_config,
        accounts_db_skip_shrink: true,
        tpu_coalesce,
        accounts_shrink_ratio,
        staked_nodes_overrides: staked_nodes_overrides.clone(),
        use_snapshot_archives_at_startup: UseSnapshotArchivesAtStartup::default().into(),
        ..ValidatorConfig::default()
    };

    let default_port_range = (1024u16, 65535u16);

    let account_paths: Vec<PathBuf> = vec![ledger_path.join("accounts")];

    let account_paths = create_and_canonicalize_directories(account_paths).unwrap_or_else(|err| {
        eprintln!("Unable to access account path: {err}");
        exit(1);
    });

    let (account_run_paths, account_snapshot_paths) =
        create_all_accounts_run_and_snapshot_dirs(&account_paths).unwrap_or_else(|err| {
            eprintln!("Error: {err}");
            exit(1);
        });

    // From now on, use run/ paths in the same way as the previous account_paths.
    validator_config.account_paths = account_run_paths;

    // These snapshot paths are only used for initial clean up, add in shrink paths if they exist.
    validator_config.account_snapshot_paths =
        if let Some(account_shrink_snapshot_paths) = account_shrink_snapshot_paths {
            account_snapshot_paths
                .into_iter()
                .chain(account_shrink_snapshot_paths)
                .collect()
        } else {
            account_snapshot_paths
        };

    let maximum_local_snapshot_age = 2500u64;
    let maximum_full_snapshot_archives_to_retain = unsafe { NonZeroUsize::new_unchecked(2) };
    let maximum_incremental_snapshot_archives_to_retain = unsafe { NonZeroUsize::new_unchecked(4) };
    let snapshot_packager_niceness_adj = 0;
    let minimal_snapshot_download_speed = 10485760f32;
    let maximum_snapshot_download_abort = 5u64;
    let full_snapshot_archives_dir = ledger_path.clone();
    let incremental_snapshot_archives_dir = full_snapshot_archives_dir.clone();
    let bank_snapshots_dir = incremental_snapshot_archives_dir.join("snapshot");
    fs::create_dir_all(&bank_snapshots_dir).unwrap_or_else(|err| {
        eprintln!(
            "Failed to create snapshots directory {:?}: {}",
            bank_snapshots_dir.display(),
            err
        );
        exit(1);
    });

    let archive_format = {
        let archive_format_str = DEFAULT_ARCHIVE_COMPRESSION;
        ArchiveFormat::from_cli_arg(&archive_format_str)
            .unwrap_or_else(|| panic!("Archive format not recognized: {archive_format_str}"))
    };

    let snapshot_version = SnapshotVersion::default();
    let incremental_snapshot_interval_slots = DEFAULT_INCREMENTAL_SNAPSHOT_ARCHIVE_INTERVAL_SLOTS;

    let (full_snapshot_archive_interval_slots, incremental_snapshot_archive_interval_slots) =
        if incremental_snapshot_interval_slots > 0 {
            (
                incremental_snapshot_interval_slots,
                DISABLED_SNAPSHOT_ARCHIVE_INTERVAL,
            )
        } else {
            (
                DISABLED_SNAPSHOT_ARCHIVE_INTERVAL,
                DISABLED_SNAPSHOT_ARCHIVE_INTERVAL,
            )
        };

    validator_config.snapshot_config = SnapshotConfig {
        usage: if full_snapshot_archive_interval_slots == DISABLED_SNAPSHOT_ARCHIVE_INTERVAL {
            SnapshotUsage::LoadOnly
        } else {
            SnapshotUsage::LoadAndGenerate
        },
        full_snapshot_archive_interval_slots,
        incremental_snapshot_archive_interval_slots,
        bank_snapshots_dir,
        full_snapshot_archives_dir: full_snapshot_archives_dir.clone(),
        incremental_snapshot_archives_dir: incremental_snapshot_archives_dir.clone(),
        archive_format,
        snapshot_version,
        maximum_full_snapshot_archives_to_retain,
        maximum_incremental_snapshot_archives_to_retain,
        accounts_hash_debug_verify: validator_config.accounts_db_test_hash_calculation,
        packager_thread_niceness_adj: snapshot_packager_niceness_adj,
    };

    // The accounts hash interval shall match the snapshot interval
    validator_config.accounts_hash_interval_slots = std::cmp::min(
        full_snapshot_archive_interval_slots,
        incremental_snapshot_archive_interval_slots,
    );

    if !is_snapshot_config_valid(
        &validator_config.snapshot_config,
        validator_config.accounts_hash_interval_slots,
    ) {
        eprintln!(
            "Invalid snapshot configuration provided: snapshot intervals are incompatible. \
             \n\t- full snapshot interval MUST be a multiple of incremental snapshot interval (if \
             enabled)\
             \n\t- full snapshot interval MUST be larger than incremental snapshot \
             interval (if enabled)\
             \nSnapshot configuration values:\
             \n\tfull snapshot interval: {}\
             \n\tincremental snapshot interval: {}",
            if full_snapshot_archive_interval_slots == DISABLED_SNAPSHOT_ARCHIVE_INTERVAL {
                "disabled".to_string()
            } else {
                full_snapshot_archive_interval_slots.to_string()
            },
            if incremental_snapshot_archive_interval_slots == DISABLED_SNAPSHOT_ARCHIVE_INTERVAL {
                "disabled".to_string()
            } else {
                incremental_snapshot_archive_interval_slots.to_string()
            },
        );
        exit(1);
    }

    validator_config.max_ledger_shreds = Some(10_000);
    validator_config.banking_trace_dir_byte_limit = BANKING_TRACE_DIR_DEFAULT_BYTE_LIMIT;
    validator_config.block_verification_method = BlockVerificationMethod::BlockstoreProcessor;
    validator_config.block_production_method = BlockProductionMethod::CentralScheduler;

    validator_config.ledger_column_options = LedgerColumnOptions {
        compression_type: BlockstoreCompressionType::default(),
        shred_storage_type: ShredStorageType::RocksLevel,
        rocks_perf_sample_interval: 0,
    };

    // skip public_rpc_addr

    let mut ledger_lock = ledger_lockfile(&ledger_path);
    let _ledger_write_guard = lock_ledger(&ledger_path, &mut ledger_lock);

    let start_progress = Arc::new(RwLock::new(ValidatorStartProgress::default()));
    let admin_service_post_init = Arc::new(RwLock::new(None));

    let (rpc_to_plugin_manager_sender, rpc_to_plugin_manager_receiver) =
        if starting_with_geyser_plugins {
            let (sender, receiver) = unbounded();
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };

    admin_rpc_service::run(
        &ledger_path,
        admin_rpc_service::AdminRpcRequestMetadata {
            rpc_addr: validator_config.rpc_addrs.map(|(rpc_addr, _)| rpc_addr),
            start_time: std::time::SystemTime::now(),
            validator_exit: validator_config.validator_exit.clone(),
            start_progress: start_progress.clone(),
            authorized_voter_keypairs: authorized_voter_keypairs.clone(),
            post_init: admin_service_post_init.clone(),
            tower_storage: validator_config.tower_storage.clone(),
            staked_nodes_overrides,
            rpc_to_plugin_manager_sender,
        },
    );

    let gossip_host = IpAddr::V4(Ipv4Addr::LOCALHOST);

    let gossip_addr = SocketAddr::new(
        gossip_host,
        solana_net_utils::find_available_port_in_range(bind_address, (0, 1)).unwrap_or_else(
            |err| {
                eprintln!("Unable to find an available gossip port: {err}");
                exit(1);
            },
        ),
    );

    // skip public_tpu_addr, public_tpu_forwards_addr, cluster_entrypoints

    let mut node = Node::new_localhost_with_pubkey(&identity_keypair.pubkey());

    if !private_rpc {
        macro_rules! set_socket {
            ($method:ident, $addr:expr, $name:literal) => {
                node.info.$method($addr).expect(&format!(
                    "Operator must spin up node with valid {} address",
                    $name
                ))
            };
        }
        if let Some((rpc_addr, rpc_pubsub_addr)) = validator_config.rpc_addrs {
            let addr = node
                .info
                .gossip()
                .expect("Operator must spin up node with valid gossip address")
                .ip();
            set_socket!(set_rpc, (addr, rpc_addr.port()), "RPC");
            set_socket!(set_rpc_pubsub, (addr, rpc_pubsub_addr.port()), "RPC-pubsub");
        }
    }

    solana_metrics::set_host_id(identity_keypair.pubkey().to_string());
    solana_metrics::set_panic_hook("validator", Some(String::from(solana_version::version!())));
    solana_entry::entry::init_poh();
    snapshot_utils::remove_tmp_snapshot_archives(&full_snapshot_archives_dir);
    snapshot_utils::remove_tmp_snapshot_archives(&incremental_snapshot_archives_dir);

    let should_check_duplicate_instance = true;

    let socket_addr_space = SocketAddrSpace::new(false);
    node.info
        .tvu(solana_gossip::contact_info::Protocol::QUIC)
        .unwrap_or_else(|e| {
            eprintln!("The error: {}", e);
            exit(1);
        });

    let validator = Validator::new(
        node,
        identity_keypair,
        &ledger_path,
        &vote_keypair.pubkey(),
        authorized_voter_keypairs,
        vec![],
        &validator_config,
        should_check_duplicate_instance,
        rpc_to_plugin_manager_receiver,
        start_progress,
        socket_addr_space,
        tpu_use_quic,
        tpu_connection_pool_size,
        tpu_enable_udp,
        admin_service_post_init,
    )
    .unwrap_or_else(|e| {
        eprintln!("Failed to start validator: {:?}", e);
        exit(1);
    });

    println!("Validator started.");

    validator
}
