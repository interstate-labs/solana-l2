use std::path::Path;

use sequencer::Sequencer;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::{EncodableKey, Signer};
use solana_sdk::transaction::Transaction;

pub mod sequencer;

fn main() {
    println!("--x-- start ---x--");

    // initialize sequencer
    let sequencer = Sequencer::new();

    // L2 txn
    let sender = Keypair::read_from_file("../keypairs/mint.json").unwrap();
    let receiver = Keypair::read_from_file("../keypairs/receiver.json").unwrap();


    // getting balances
    let sender_balance = sequencer.validator.bank_forks.read().unwrap().working_bank().get_balance(&sender.pubkey());
    let receiver_balance = sequencer.validator.bank_forks.read().unwrap().working_bank().get_balance(&receiver.pubkey());
    println!("Before Balances: \n sender: {}, receiver: {}", sender_balance/(10u64.pow(9)), receiver_balance/(10u64.pow(9)));


    let ix = solana_program::system_instruction::transfer(
        &sender.pubkey(),
        &receiver.pubkey(),
        LAMPORTS_PER_SOL,
    );

    let recent_blockhash = sequencer
        .validator
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

    sequencer.add_transaction(tx).unwrap();

    // getting balances
    let sender_balance = sequencer.validator.bank_forks.read().unwrap().working_bank().get_balance(&sender.pubkey());
    let receiver_balance = sequencer.validator.bank_forks.read().unwrap().working_bank().get_balance(&receiver.pubkey());
    println!("After Balances: \n sender: {}, receiver: {}", sender_balance/(10u64.pow(9)), receiver_balance/(10u64.pow(9)));


    println!("--x--- end ---x--");
}
