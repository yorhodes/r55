mod exec;
use exec::{deploy_contract, run_tx, run_tx_prolog};

use std::fs::File;
use std::io::Read;
use std::process::Command;

use alloy_sol_types::SolValue;
use revm::{
    primitives::{address, keccak256, ruint::Uint, AccountInfo, Address, Bytecode, Bytes},
    InMemoryDB,
};

fn compile_runtime(path: &str) -> Result<Vec<u8>, ()> {
    println!("Compiling runtime: {}", path);
    let status = Command::new("cargo")
        .arg("+nightly-2024-02-01")
        .arg("build")
        .arg("-r")
        .arg("--lib")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("--target")
        .arg("riscv64imac-unknown-none-elf")
        .arg("--bin")
        .arg("runtime")
        .current_dir(path)
        .status()
        .expect("Failed to execute cargo command");

    if !status.success() {
        eprintln!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        println!("Cargo command completed successfully");
    }

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/runtime",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            return Err(());
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eprintln!("Failed to read file: {}", e);
        return Err(());
    }

    Ok(bytecode)
}

fn compile_deploy(path: &str) -> Result<Vec<u8>, ()> {
    compile_runtime(path)?;
    println!("Compiling deploy: {}", path);
    let status = Command::new("cargo")
        .arg("+nightly-2024-02-01")
        .arg("build")
        .arg("-r")
        .arg("--lib")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("--target")
        .arg("riscv64imac-unknown-none-elf")
        .arg("--bin")
        .arg("deploy")
        .current_dir(path)
        .status()
        .expect("Failed to execute cargo command");

    if !status.success() {
        eprintln!("Cargo command failed with status: {}", status);
        std::process::exit(1);
    } else {
        println!("Cargo command completed successfully");
    }

    let path = format!(
        "{}/target/riscv64imac-unknown-none-elf/release/deploy",
        path
    );
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to open file: {}", e);
            return Err(());
        }
    };

    // Read the file contents into a vector.
    let mut bytecode = Vec::new();
    if let Err(e) = file.read_to_end(&mut bytecode) {
        eprintln!("Failed to read file: {}", e);
        return Err(());
    }

    Ok(bytecode)
}

fn add_contract_to_db(db: &mut InMemoryDB, addr: Address, bytecode: Bytes) {
    let account = AccountInfo::new(
        Uint::from(0),
        0,
        keccak256(&bytecode),
        Bytecode::new_raw(bytecode),
    );
    db.insert_account_info(addr, account);
}

fn test_prolog_contract() {
    println!("\n\n============ Running Prolog tests ============\n\n");
    let prolog_bytes = include_bytes!("../erc20.pl");

    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xf7];
    bytecode.extend_from_slice(prolog_bytes);

    let bytecode = Bytes::from(bytecode);

    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let from: u64 = 0xcafebeef;
    let to: u64 = 0xdeadbeef;
    let value_mint: u64 = 42;
    let value_transfer: u64 = 7;

    let calldata_balance_from = format!("balanceOf({from})");
    let calldata_balance_to = format!("balanceOf({to})");
    let calldata_mint = format!("mint({from}, {value_mint})");
    let calldata_transfer = format!("transfer({from}, {to}, {value_transfer})");

    println!("=== Balance of address {from}:");
    run_tx_prolog(
        &mut db,
        &CONTRACT_ADDR,
        calldata_balance_from.clone().into(),
    );
    println!("=== Minting {value_mint} tokens to address {from}:");
    run_tx_prolog(&mut db, &CONTRACT_ADDR, calldata_mint.clone().into());
    println!("=== Balance of address {from}:");
    run_tx_prolog(
        &mut db,
        &CONTRACT_ADDR,
        calldata_balance_from.clone().into(),
    );
    println!("=== Transfer {value_transfer} from {from} to {to}:");
    run_tx_prolog(&mut db, &CONTRACT_ADDR, calldata_transfer.clone().into());
    println!("=== Balance of address {from}:");
    run_tx_prolog(
        &mut db,
        &CONTRACT_ADDR,
        calldata_balance_from.clone().into(),
    );
    println!("=== Balance of address {to}:");
    run_tx_prolog(&mut db, &CONTRACT_ADDR, calldata_balance_to.clone().into());

    /*
    let account_db = &evm.db().accounts[&CONTRACT_ADDR];
    println!("Account storage: {:?}", account_db.storage);
    let slot_42 = account_db.storage[&U256::from(42)];
    assert_eq!(slot_42.as_limbs()[0], 0xdeadbeef);
    */
}
fn test_runtime_from_binary() {
    let rv_bytecode = compile_runtime("erc20").unwrap();

    const CONTRACT_ADDR: Address = address!("0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    add_contract_to_db(&mut db, CONTRACT_ADDR, bytecode);

    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let to: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = to.abi_encode();
    let mut calldata_mint = (to, value_mint).abi_encode();

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_mint.clone());
    run_tx(&mut db, &CONTRACT_ADDR, complete_calldata_balance.clone());

    /*
    let account_db = &evm.db().accounts[&CONTRACT_ADDR];
    println!("Account storage: {:?}", account_db.storage);
    let slot_42 = account_db.storage[&U256::from(42)];
    assert_eq!(slot_42.as_limbs()[0], 0xdeadbeef);
    */
}

fn test_runtime(addr: &Address, db: &mut InMemoryDB) {
    let selector_balance = &keccak256("balance_of")[0..4];
    let selector_mint = &keccak256("mint")[0..4];
    let to: Address = address!("0000000000000000000000000000000000000001");
    let value_mint: u64 = 42;
    let mut calldata_balance = to.abi_encode();
    let mut calldata_mint = (to, value_mint).abi_encode();

    let mut complete_calldata_balance = selector_balance.to_vec();
    complete_calldata_balance.append(&mut calldata_balance);

    let mut complete_calldata_mint = selector_mint.to_vec();
    complete_calldata_mint.append(&mut calldata_mint);

    run_tx(db, addr, complete_calldata_mint.clone());
    run_tx(db, addr, complete_calldata_balance.clone());
}

fn test_deploy() {
    let rv_bytecode = compile_deploy("erc20").unwrap();
    let mut db = InMemoryDB::default();

    let mut bytecode = vec![0xff];
    bytecode.extend_from_slice(&rv_bytecode);

    let bytecode = Bytes::from(bytecode);

    let addr = deploy_contract(&mut db, bytecode);

    test_runtime(&addr, &mut db);
}

fn main() {
    test_runtime_from_binary();
    test_deploy();
    test_prolog_contract();
}
