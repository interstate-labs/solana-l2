use anchor_client::solana_sdk::clock::Clock;
use anchor_client::solana_sdk::system_program;
use anchor_client::solana_sdk::sysvar::SysvarId;
use anchor_client::Client;
use anchor_client::solana_client::nonblocking::rpc_client::RpcClient;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::{EncodableKey, Signer};
use anchor_client::solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, transaction::Transaction};
use anchor_client::solana_sdk::instruction::{Instruction, AccountMeta};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{collections::VecDeque, fmt::Error};

// just wrapping a native solana transaction to add other fields
pub struct TxInfo {
    _txn: Transaction,
    _receipt: String,
}

pub struct Sequencer {
    inbox: Arc<Mutex<VecDeque<TxInfo>>>,
    pub l2_client: RpcClient,

    l1_rpc_url: String,
    pub blockroot_program: Pubkey,
    pub blockroot_pda: (Pubkey, u8),

}

// Sequencer txn ordering and execution
impl Sequencer {
    pub fn new(rpc_url: &str, l1_rpc: &str) -> Self {
        let rpc_client =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());
        let blockroot_program = blockroot::id();
        let (blockroot_pda, bump) =
            Pubkey::find_program_address(&[blockroot::da::PREFIX.as_bytes()], &blockroot_program);

        Sequencer {
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            l2_client: rpc_client,
            l1_rpc_url: l1_rpc.to_string(),
            blockroot_program,
            blockroot_pda
        }
    }

    pub fn update_l1_rpc(&mut self, new_rpc_str: &str) {
        self.l1_rpc_url = new_rpc_str.to_string();
    }

    pub async fn add_transaction(&self, tx: Transaction) -> Result<(), Box<dyn std::error::Error>> {
        println!("Adding txn to inbox .....");
        // adding txns to the inbox
        // TODO: Add pre transaction validation/ run preflights here
        // validate txns before adding to the inbox (maybe simulate?)
        self.execute_transaction(&tx).await.unwrap();

        // processing result and adding to the inbox
        let txn_info = TxInfo {
            _txn: tx,
            _receipt: "HOLDER".to_string(),
        };

        let mut inbox = self.inbox.lock().unwrap();
        inbox.push_back(txn_info);
        Ok(())
    }

    async fn execute_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Executing txn ....");
        let _result = self
            .l2_client
            .send_and_confirm_transaction(tx)
            .await
            .unwrap();
        Ok(())
    }

    pub async fn post_blockroot_hash(&self) {
        let hash = self.l2_client.get_latest_blockhash().await.unwrap();
        let sequencer_kp = Keypair::read_from_file("./keypair.json").unwrap();

        let l1_client = RpcClient::new(self.l1_rpc_url.clone());
        let anchor_client = Client::new_with_options(
            anchor_client::Cluster::Custom(self.l1_rpc_url.clone(), String::new()),
            &sequencer_kp,
            CommitmentConfig::confirmed(),
        );
        let program = anchor_client.program(blockroot::ID).unwrap();

        let signature = program
            .request()
            .accounts(blockroot::accounts::ProcessChunk {
                creator: sequencer_kp.pubkey(),
                chunk_accumulator: chunks_keypair.pubkey(),
                blocks_root: self.blockroot_pda.0,
                clock: Clock::id(),
                system_program: system_program::id(),
            })
            .args(blockroot::instruction::ProcessChunk {
                bump: self.blockroot_pda.1,
                chunk,
            })
            .options(CommitmentConfig {
                commitment: CommitmentLevel::Processed,
            })
            .signer(chunks_keypair)
            .send()?;
        Ok(signature)
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> u64 {
        self.l2_client.get_balance(pubkey).await.unwrap()
    }
}
