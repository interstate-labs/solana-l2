use sequencer::Sequencer;
use anchor_client::solana_client::nonblocking::rpc_client::RpcClient;
use anchor_client::solana_client::rpc_config::RpcRequestAirdropConfig;
use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use anchor_client::solana_sdk::native_token::LAMPORTS_PER_SOL;
use anchor_client::solana_sdk::signature::Keypair;
use anchor_client::solana_sdk::signer::Signer;
use anchor_client::solana_sdk::transaction::Transaction;

pub mod sequencer;

fn main() {
    // getting balances
    // let sender_balance = sequencer.client.get_balance(&sender.pubkey()).unwrap();
    // let receiver_balance = sequencer.client.get_balance(&receiver.pubkey()).unwrap();

    // println!(
    //     "Before Balances: \n sender: {}, receiver: {}",
    //     sender_balance,
    //     receiver_balance
    // );

    // // getting balances
    // let sender_balance = sequencer.client.get_balance(&sender.pubkey()).unwrap();
    // let receiver_balance = sequencer.client.get_balance(&receiver.pubkey()).unwrap();

    // println!(
    //     "Before Balances: \n sender: {}, receiver: {}",
    //     sender_balance,
    //     receiver_balance
    // );

    println!("--x--- end ---x--");
}

use tokio::time::{sleep, Duration};
#[tokio::test]
async fn test_main() {
    println!("--x-- start ---x--");

    // setup
    let rpc_url = "http://localhost:8899";
    let sequencer = Sequencer::new(rpc_url, rpc_url);
    // let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // keypairs
    let sender = Keypair::new();
    let receiver = Keypair::new();

    // airdrop for sender
    let airdrop_signature = sequencer
        .l2_client
        .request_airdrop(&sender.pubkey(), 5 * LAMPORTS_PER_SOL)
        .await
        .expect("Airdrop request failed");

    // Confirm the airdrop transaction
    sequencer
        .l2_client
        .confirm_transaction(&airdrop_signature)
        .await
        .expect("Transaction confirmation failed");

    // Sleep for a few seconds to ensure the transaction is processed
    sleep(Duration::from_secs(5)).await;

    // getting balances
    let sender_balance = sequencer.get_balance(&sender.pubkey()).await;
    let receiver_balance = sequencer.get_balance(&receiver.pubkey()).await;

    println!(
        "Before Balances: \n sender: {} SOL, receiver: {} SOL",
        sender_balance, receiver_balance
    );

    // Transaction
    let ix = solana_program::system_instruction::transfer(
        &sender.pubkey(),
        &receiver.pubkey(),
        LAMPORTS_PER_SOL,
    );

    let recent_blockhash = sequencer.l2_client.get_latest_blockhash().await.unwrap();
    println!("Block hash: {}", recent_blockhash);

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&sender.pubkey()),
        &[&sender],
        recent_blockhash,
    );

    sequencer.add_transaction(tx).await.unwrap();

    // getting balances
    let sender_balance = sequencer.get_balance(&sender.pubkey()).await;
    let receiver_balance = sequencer.get_balance(&receiver.pubkey()).await;

    println!(
        "After Balances: \n sender: {} SOL, receiver: {} SOL",
        sender_balance, receiver_balance
    );
}
