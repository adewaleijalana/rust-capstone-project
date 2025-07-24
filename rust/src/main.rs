#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::json::AddressType;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info

    /*
    I was getting JSON related exception, reason for commenting this out and trying another method of
    using rpc.call("getblockchaininfo", &[])?;

    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);
    */

    let raw: serde_json::Value = rpc.call("getblockchaininfo", &[])?;
    println!("{}", serde_json::to_string_pretty(&raw)?);

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    let miner_rpc = create_load_wallet(&rpc, "Miner")?;
    let trader_rpc = create_load_wallet(&rpc, "Trader")?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    let miner_address = miner_rpc.get_new_address(None, None)?;
    let miner_address = miner_address
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        .unwrap();
    let _ = miner_rpc.generate_to_address(101, &miner_address)?;

    // Load Trader wallet and generate a new address
    let trader_address = trader_rpc.get_new_address(None, None)?;
    let trader_address = trader_address
        .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
        .unwrap();

    // Send 20 BTC from Miner to Trader
    let txid = miner_rpc.send_to_address(
        &trader_address,
        Amount::from_btc(20.0)?,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    // Check transaction in mempool
    let mempool = rpc.get_raw_mempool()?;
    assert!(mempool.contains(&txid), "TX not in mempool!");

    // Mine 1 block to confirm the transaction
    let _ = miner_rpc.generate_to_address(1, &miner_address)?;

    // Extract all required transaction details
    let tx = rpc.get_raw_transaction_info(&txid, None)?;

    //Getting the previous transaction details
    let vin = &tx.vin[0];
    let prev_tx =
        rpc.get_raw_transaction_info(vin.txid.as_ref().expect("vin.txid is None"), None)?;

    let input_vout = &prev_tx.vout[vin.vout.expect("vin.vout is None") as usize];

    //Getting the Miners input address as a string
    let miner_input_address = input_vout
        .script_pub_key
        .address
        .clone()
        .as_ref()
        .map(|addr| {
            addr.clone()
                .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
                .unwrap()
                .to_string()
        })
        .expect("No address found in miner input script_pub_key");

    let miner_input_amount: Amount = input_vout.value;

    let outputs = &tx.vout;

    let (trader_output, change_output) = if outputs[0].value == Amount::from_btc(20.0)? {
        (&outputs[0], &outputs[1])
    } else {
        (&outputs[1], &outputs[0])
    };

    //Getting the Trader output address as a string
    let trader_output_address = trader_output
        .script_pub_key
        .address
        .clone()
        .as_ref()
        .map(|addr| {
            addr.clone()
                .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
                .unwrap()
                .to_string()
        })
        .expect("No address found in miner input script_pub_key");

    let trader_output_amount = trader_output.value;

    //Getting the Miners change address as a string
    let miner_change_address = change_output
        .script_pub_key
        .address
        .clone()
        .as_ref()
        .map(|addr| {
            addr.clone()
                .require_network(bitcoincore_rpc::bitcoin::Network::Regtest)
                .unwrap()
                .to_string()
        })
        .expect("No address found in miner input script_pub_key");

    let miner_change_amount = change_output.value;

    let total_output: f64 = outputs.iter().map(|v| v.value.to_btc()).sum();
    let rounded = (total_output * 100_000_000.0).round() / 100_000_000.0;
    let total_output = Amount::from_btc(rounded)?;

    let fees = miner_input_amount - total_output;

    let block_hash = tx.blockhash.unwrap();
    let block = rpc.get_block_info(&block_hash)?;
    let block_height = block.height;

    // Write the data to ../out.txt in the specified format given in readme.md
    let mut file = File::create("../out.txt")?;
    writeln!(file, "{}", tx.txid)?;
    writeln!(file, "{miner_input_address}")?;
    writeln!(file, "{}", miner_input_amount.to_btc())?;
    writeln!(file, "{trader_output_address}")?;
    writeln!(file, "{}", trader_output_amount.to_btc())?;
    writeln!(file, "{miner_change_address}")?;
    writeln!(file, "{}", miner_change_amount.to_btc())?;
    writeln!(file, "{:.2e}", -fees.to_btc())?;
    writeln!(file, "{block_height}")?;
    writeln!(file, "{block_hash}")?;

    Ok(())
}

//I created this method for creating or loading wallet and return the RPC client to interact with the wallet
fn create_load_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<Client> {
    let _ = rpc.load_wallet(wallet_name);

    let wallet_path = format!("{}/wallet/{}", std::env::var("HOME").unwrap(), wallet_name);
    if !Path::new(&wallet_path).exists() {
        let _ = rpc.create_wallet(wallet_name, None, None, None, None);
    }

    //This is to return a client to interact with the wallet
    let wallet_url = format!("{RPC_URL}/wallet/{wallet_name}");
    Client::new(
        &wallet_url,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )
}
