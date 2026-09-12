//! Wallet-wide scan reads and native rewind; no network scheduler or history reconstruction.
use super::accounts::{fields, height, string, Failure, Result};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use zcash_client_backend::data_api::{WalletRead, WalletWrite, wallet::ConfirmationsPolicy};
use zcash_client_sqlite::error::SqliteClientError;

fn execute(generation: u32, operation: &str, input: &Value) -> Result<Value> {
    super::DOMAIN.with(|domain| {
        let mut domain=domain.try_borrow_mut().map_err(|_|Failure::from("STORAGE_BUSY"))?;
        let active=domain.active.as_mut().filter(|a|a.generation==generation).ok_or(Failure::from("STALE_HANDLE"))?;
        match operation {
            "scan_state" => {
                fields(input,&[])?;
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    // Scan completeness is upstream-owned and independent of per-account amount projection.
                    let summary=db.get_wallet_summary_in_transaction(ConfirmationsPolicy::MIN)?;
                    Ok(json!({"revision":super::revision::read(ext)?,
                        "tipHeight":db.chain_height()?.map(u32::from),
                        "fullyScannedHeight":db.block_fully_scanned()?.map(|b|u32::from(b.block_height())),
                        "maxScannedHeight":db.block_max_scanned()?.map(|b|u32::from(b.block_height())),
                        "scanComplete":summary.as_ref().map(|s|s.is_synced())}))
                })
            }
            "scan_block_hash" => {
                fields(input,&["height"])?;
                let height=height(input,"height")?;
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    Ok(json!({"revision":super::revision::read(ext)?,
                        "point":db.get_block_hash(height.into())?.map(|hash|json!({"height":height,"hash":hex::encode(hash.0)}))}))
                })
            }
            "scan_rewind" => {
                fields(input,&["revision","requestedPoint"])?;
                let revision=string(input,"revision")?;
                if revision.len()>128 {return Err("INVALID_ARGUMENT".into());}
                let point=input.get("requestedPoint").ok_or(Failure::from("INVALID_ARGUMENT"))?;
                fields(point,&["height","hash"])?;
                let requested=height(point,"height")?;
                if requested==u32::MAX {return Err("INVALID_ARGUMENT".into());}
                let hash=string(point,"hash")?;
                let bytes=hex::decode(hash).map_err(|_|Failure::from("INVALID_ARGUMENT"))?;
                if bytes.len()!=32 || hex::encode(&bytes)!=hash {return Err("INVALID_ARGUMENT".into());}
                let result=active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    if super::revision::read(ext)?!=revision {return Err("STALE_REVISION".into());}
                    let known=db.get_block_hash(requested.into())?.ok_or(Failure::from("RECOVERY_REQUIRED"))?;
                    if known.0.as_slice()!=bytes {return Err("CHAIN_MISMATCH".into());}
                    let actual=db.truncate_to_height(requested.into()).map_err(|error|match error {
                        SqliteClientError::RequestedRewindInvalid {..} | SqliteClientError::TruncateCommitmentTree {..} => Failure::from("RECOVERY_REQUIRED"),
                        other=>Failure::from(other),
                    })?;
                    let hash=db.get_block_hash(actual)?.ok_or(Failure::from("RECOVERY_REQUIRED"))?;
                    super::revision::advance(ext)?;
                    Ok(json!({"revision":super::revision::read(ext)?,"point":{"height":u32::from(actual),"hash":hex::encode(hash.0)}}))
                })?;
                active.scan_plan=None;
                Ok(result)
            }
            _=>Err("INVALID_ARGUMENT".into()),
        }
    })
}

#[wasm_bindgen]
pub fn sync_call(generation:u32, operation:&str, input:&str)->std::result::Result<String,String> {
    if input.len()>1024 {return Err("RESOURCE_LIMIT".into());}
    let value=serde_json::from_str(input).map_err(|_|"INVALID_ARGUMENT".to_string())?;
    execute(generation,operation,&value).map(|v|v.to_string()).map_err(|e|e.0)
}
