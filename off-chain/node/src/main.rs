use std::path::Path;

use sequencer::{setting_l2_env, Sequencer};
use solana_client::rpc_client::RpcClient;
use solana_program_runtime::log_collector::log::{debug, info};
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::{EncodableKey, Signer};
use solana_sdk::transaction::Transaction;

pub mod sequencer;

fn main() {
    println!("--x-- start ---x--");
    debug!("From debug");

    // initialize sequencer
    let sequencer = Sequencer::new();
    let validator = setting_l2_env();

    // L2 txn
    let sender = Keypair::read_from_file("./keypairs/mint.json").unwrap();
    let receiver = Keypair::read_from_file("./keypairs/receiver.json").unwrap();

    // getting balances
    let sender_balance = validator
        .bank_forks
        .read()
        .unwrap()
        .working_bank()
        .get_balance(&sender.pubkey());
    let receiver_balance = validator
        .bank_forks
        .read()
        .unwrap()
        .working_bank()
        .get_balance(&receiver.pubkey());
    println!(
        "Before Balances: \n sender: {}, receiver: {}",
        sender_balance / (10u64.pow(9)),
        receiver_balance / (10u64.pow(9))
    );

    let ix = solana_program::system_instruction::transfer(
        &sender.pubkey(),
        &receiver.pubkey(),
        LAMPORTS_PER_SOL,
    );

    let recent_blockhash = validator
        .bank_forks
        .read()
        .unwrap()
        .working_bank()
        .last_blockhash();

    println!("Block hash: {}", recent_blockhash);

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&sender.pubkey()),
        &[&sender],
        recent_blockhash,
    );

    sequencer.add_transaction(&validator, tx).unwrap();

    info!(" ---------------------------x---------------- Transaction Processeddddddd ---------------------x--------------- ");

    // getting balances
    let sender_balance = validator
        .bank_forks
        .read()
        .unwrap()
        .working_bank()
        .get_balance(&sender.pubkey());
    let receiver_balance = validator
        .bank_forks
        .read()
        .unwrap()
        .working_bank()
        .get_balance(&receiver.pubkey());
    println!(
        "After Balances: \n sender: {}, receiver: {}",
        sender_balance / (10u64.pow(9)),
        receiver_balance / (10u64.pow(9))
    );

    validator.close();
    println!("--x--- end ---x--");
}

#[test]
pub fn test_main() {
    debug!("---x----");
    main();
    debug!("---x----");
}
