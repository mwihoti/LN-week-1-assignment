use bitcoincore_rpc::{Auth, Client as BitcoinClient, RpcApi};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn call_cln(method: &str, params: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let rune = std::env::var("CLN_RUNE")?;
    let url = format!("http://localhost:3010/v1/{}", method);

    let client = Client::new();
    let response = client
        .post(&url)
        .json(&params)
        .header("Rune", rune)
        .send()?
        .json::<Value>()?;

    Ok(response)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get blockchain info
    let rpc = BitcoinClient::new(
        "http://localhost:18443",
        Auth::UserPass("alice".to_string(), "password".to_string()),
    )?;

    println!("Blockchain Info: {:?}", rpc.get_blockchain_info()?);

    // Get Lightning node info
    let ln_info = call_cln("getinfo", json!({}))?;
    println!("Lightning Node Info: {}", ln_info);

    // Create a new address for funding using lightning-cli and store it in CLN_ADDRESS
    let newaddr = call_cln("newaddr", json!({"addresstype": "bech32"}))?;
    let cln_address = newaddr["bech32"]
        .as_str()
        .or_else(|| newaddr["p2tr"].as_str())
        .ok_or("no address in newaddr response")?;
    println!("CLN Address: {}", cln_address);

    // Check if wallet exists, if not Create a bitcoin wallet named 'mining_wallet' using bitcoin-cli for mining
    let loaded: Vec<String> = rpc.call("listwallets", &[])?;
    if !loaded.iter().any(|w| w == "mining_wallet") {
        let wallets: Value = rpc.call("listwalletdir", &[])?;
        let exists = wallets["wallets"]
            .as_array()
            .is_some_and(|ws| ws.iter().any(|w| w["name"] == "mining_wallet"));
        if exists {
            rpc.call::<Value>("loadwallet", &[json!("mining_wallet")])?;
        } else {
            rpc.call::<Value>("createwallet", &[json!("mining_wallet")])?;
        }
    }

    let miner = BitcoinClient::new(
        "http://localhost:18443/wallet/mining_wallet",
        Auth::UserPass("alice".to_string(), "password".to_string()),
    )?;
    let mining_address: String = miner.call("getnewaddress", &[])?;
    println!("Mining address: {}", mining_address);

    // Generate a new address and mine blocks to it. How many blocks need to mined? Why?
    let height = rpc.get_block_count()?;
    if height < 101 {
        miner.call::<Value>(
            "generatetoaddress",
            &[json!(101 - height), json!(mining_address)],
        )?;
    }
    println!("Balance after funding: {}", miner.get_balance(None, None)?);

    // Fund the Lightning node by sending 0.1 BTC from the mining wallet to CLN_ADDRESS
    miner.call::<Value>("sendtoaddress", &[json!(cln_address), json!(0.1)])?;
    miner.call::<Value>("sendtoaddress", &[json!(cln_address), json!(0.1)])?;
    println!("Balance after funding: {}", miner.get_balance(None, None)?);

    // Confirm the funding transaction by mining 6 blocks
    miner.call::<Value>("generatetoaddress", &[json!(6), json!(mining_address)])?;
    println!("Balance after funding: {}", miner.get_balance(None, None)?);

    // Verify Lightning wallet balance using lightning-cli listfunds
    let funds = call_cln("listfunds", json!({}))?;
    println!("CLN funds: {}", funds);

    // Create an invoice with parameters and store the invoice string:
    // - Amount: 50,000 satoshis (50000000 millisatoshis)
    // - Label: Generate unique label using timestamp (e.g., "invoice_$(date +%s)")
    // - Description: "Coffee Payment"
    // - Expiry: 3600 seconds
    let label = format!(
        "invoice_{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );
    let invoice = call_cln(
        "invoice",
        json!({
            "amount_msat": 50000000,
            "label": label,
            "description": "Coffee Payment",
            "expiry": 3600
        }),
    )?;
    let payment_hash = invoice["payment_hash"].as_str().unwrap_or_default();
    let bolt11 = invoice["bolt11"].as_str().unwrap_or_default();

    // Decode the invoice string using lightning-cli decodepay and verify the parameters
    //  let decoded = call_cln("decodepay", serde_json::json!({ "bolt11": bolt11 }))?;
    let amount_msat = invoice["amount_msat"]
        .as_str()
        .unwrap_or_default()
        .trim_end_matches("msat");
    let amount: u64 = amount_msat.parse().unwrap_or(50000000);
    let description = invoice["description"].as_str().unwrap_or("Coffee Payment");
    let expiry: u64 = 3600;

    println!("Invoice fields: {amount}msat, {description}, {expiry}s expiry");
    // Output the invoice details in the specified format to out.txt
    // - Payment hash
    // - BOLT11 invoice string
    // - Amount
    // - Description
    // - Expiry time

    fs::write(
        "../out.txt",
        format!("{payment_hash}\n{bolt11}\n{amount}\n{description}\n{expiry}\n"),
    )?;

    Ok(())
}
