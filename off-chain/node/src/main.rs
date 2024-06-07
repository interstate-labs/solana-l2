use sequencer::Sequencer;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;

pub mod sequencer;

fn main() {
    println!("--x-- start ---x--");

    // initialize sequencer
    let sequencer = Sequencer::new();

    // // L2 txn
    let sender = Keypair::new();
    let reciever = Keypair::new();
    let ix = solana_program::system_instruction::transfer(
        &sender.pubkey(),
        &reciever.pubkey(),
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

    println!("--x--- end ---x--");
}
