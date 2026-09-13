//! Worker-local authority, independent of database lifetime. No spending-key export.
use std::cell::RefCell;
use serde_json::json;
use secrecy::SecretVec;
use wasm_bindgen::prelude::*;
use zcash_client_backend::data_api::{Account,AccountPurpose,WalletRead};
use zcash_client_sqlite::AccountUuid;
use zcash_keys::keys::UnifiedSpendingKey;

const MAX_SIGNERS: usize = 1024;
struct Signer { key: UnifiedSpendingKey, parameters: Vec<u8>, genesis: Vec<u8>, account_index: u32,
    bindings: Vec<(u32,AccountUuid)> }
// Tokens never recycle within this native instance; exhausted instances fail closed.
thread_local! { static SIGNERS: RefCell<Vec<Option<Signer>>> = const { RefCell::new(Vec::new()) }; }

#[wasm_bindgen]
pub fn signer_create_account(generation:u32, operation:&str, input:&str, mnemonic:Vec<u8>, passphrase:Vec<u8>) -> Result<String,String> {
    let mnemonic=SecretVec::new(mnemonic);
    let passphrase=SecretVec::new(passphrase);
    // Reserve allocation and identity before the account transaction can commit.
    SIGNERS.with(|table| {
        let mut table=table.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
        if table.len()>=MAX_SIGNERS { return Err("RESOURCE_LIMIT".into()); }
        table.try_reserve(1).map_err(|_|"RESOURCE_LIMIT")?;
        let (parameters,genesis)=super::DOMAIN.with(|domain| {
            let mut domain=domain.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
            let active=domain.active.as_mut().filter(|a|a.generation==generation).ok_or("STALE_HANDLE")?;
            let genesis=active.wallet.transactionally_with_extension(|_,ext| -> super::accounts::Result<Vec<u8>> {
                Ok(ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?)
            }).map_err(|e|e.0)?;
            Ok::<_,String>((active.bytes.clone(),genesis))
        })?;
        let (account,key)=super::accounts::mnemonic_account(generation,operation,input,mnemonic,passphrase).map_err(|e|e.0)?;
        // The account index is returned by the native allocator/import operation.
        let account_index=account["accountIndex"].as_u64().expect("native derived account index") as u32;
        let token=(table.len()+1) as u32;
        table.push(Some(Signer {key,parameters,genesis,account_index,bindings:Vec::new()}));
        Ok(json!({"account":account,"signerToken":token}).to_string())
    })
}
fn index(token:u32)->Result<usize,String> { token.checked_sub(1).map(|n|n as usize).ok_or("STALE_HANDLE".into()) }
#[wasm_bindgen]
pub fn signer_describe(token:u32)->Result<String,String> {
    SIGNERS.with(|table| {
        let table=table.try_borrow().map_err(|_|"STORAGE_BUSY")?;
        let signer=table.get(index(token)?).and_then(Option::as_ref).ok_or("STALE_HANDLE")?;
        let p=crate::Document::parse(&signer.parameters).map_err(|_|"DOMAIN_INVALID")?;
        Ok(json!({"parameters":hex::encode(&signer.parameters),"genesis":hex::encode(&signer.genesis),
            "accountIndex":signer.account_index,"viewingKey":signer.key.to_unified_full_viewing_key().encode(&p)}).to_string())
    })
}
#[wasm_bindgen]
pub fn signer_release(token:u32)->Result<(),String> {
    SIGNERS.with(|table| {
        let mut table=table.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
        table.get_mut(index(token)?).and_then(Option::take).ok_or("STALE_HANDLE")?;
        Ok(())
    })
}
pub(super) fn detach_wallet(generation:u32) {
    SIGNERS.with(|table| { for signer in table.borrow_mut().iter_mut().flatten() { signer.bindings.retain(|(g,_)|*g!=generation); } });
}

#[wasm_bindgen]
pub fn signer_bind(token:u32,generation:u32,account_id:&str)->Result<String,String> {
    let account_id=AccountUuid::from_uuid(uuid::Uuid::parse_str(account_id).map_err(|_|"INVALID_ARGUMENT")?);
    SIGNERS.with(|table| {
        let mut table=table.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
        let signer=table.get_mut(index(token)?).and_then(Option::as_mut).ok_or("STALE_HANDLE")?;
        let present=signer.bindings.contains(&(generation,account_id));
        if !present {
            if signer.bindings.len()>=1024 {return Err("RESOURCE_LIMIT".into());}
            signer.bindings.try_reserve(1).map_err(|_|"RESOURCE_LIMIT")?;
        }
        let state=super::DOMAIN.with(|domain| {
            let mut domain=domain.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
            let active=domain.active.as_mut().filter(|a|a.generation==generation).ok_or("STALE_HANDLE")?;
            if active.bytes!=signer.parameters {return Err("NETWORK_MISMATCH".to_string());}
            active.wallet.transactionally_with_extension(|db,ext| -> super::accounts::Result<&'static str> {
                let genesis:Vec<u8>=ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?;
                if genesis!=signer.genesis {return Err("NETWORK_MISMATCH".into());}
                let account=db.get_account(account_id)?.ok_or(super::accounts::Failure::from("ACCOUNT_NOT_FOUND"))?;
                if !account.ufvk().is_some_and(|key|signer.key.to_unified_full_viewing_key().subsumes_ufvk(key)) {
                    return Err("SIGNER_MISMATCH".into());
                }
                Ok(if account.purpose()==AccountPurpose::ViewOnly {"recovery-required"} else {"ready"})
            }).map_err(|e|e.0)
        })?;
        if !present { signer.bindings.push((generation,account_id)); }
        Ok(state.into())
    })
}
#[wasm_bindgen]
pub fn signer_unbind(token:u32,generation:u32,account_id:&str)->Result<(),String> {
    let account_id=AccountUuid::from_uuid(uuid::Uuid::parse_str(account_id).map_err(|_|"INVALID_ARGUMENT")?);
    SIGNERS.with(|table| {
        let mut table=table.try_borrow_mut().map_err(|_|"STORAGE_BUSY")?;
        let signer=table.get_mut(index(token)?).and_then(Option::as_mut).ok_or("STALE_HANDLE")?;
        signer.bindings.retain(|pair|*pair!=(generation,account_id)); Ok(())
    })
}
