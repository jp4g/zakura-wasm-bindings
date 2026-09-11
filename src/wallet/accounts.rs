//! Private UFVK and durable address operations on the existing storage owner.
use wasm_bindgen::prelude::*;
use prost::Message;
use serde_json::{json, Value};
use zcash_client_backend::{data_api::{Account, AccountBirthday, AccountPurpose, WalletRead, WalletWrite}, proto::service::TreeState};
use zcash_client_sqlite::{AccountUuid, error::SqliteClientError};
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_protocol::consensus::{Parameters, NetworkType};

#[derive(Debug)]
struct Failure(String);
impl From<rusqlite::Error> for Failure { fn from(_: rusqlite::Error) -> Self { Self("STORAGE_ERROR".into()) } }
impl From<SqliteClientError> for Failure { fn from(_: SqliteClientError) -> Self { Self("BACKEND_ERROR".into()) } }
impl From<&str> for Failure { fn from(s: &str) -> Self { Self(s.into()) } }
type Result<T> = std::result::Result<T, Failure>;
fn string<'a>(v: &'a Value, name: &str) -> Result<&'a str> { v.get(name).and_then(Value::as_str).ok_or("INVALID_ARGUMENT".into()) }
fn height(v: &Value, name: &str) -> Result<u32> { v.get(name).and_then(Value::as_u64).and_then(|n| n.try_into().ok()).ok_or("INVALID_ARGUMENT".into()) }
fn fields(v: &Value, allowed: &[&str]) -> Result<()> {
    if !v.as_object().is_some_and(|m| m.keys().all(|k| allowed.contains(&k.as_str()))) { return Err("INVALID_ARGUMENT".into()); }
    Ok(())
}
fn id(v: &Value) -> Result<AccountUuid> {
    let text = string(v,"accountId")?;
    let uuid = uuid::Uuid::parse_str(text).map_err(|_| Failure::from("INVALID_ARGUMENT"))?;
    if uuid.to_string() != text { return Err("INVALID_ARGUMENT".into()); }
    Ok(AccountUuid::from_uuid(uuid))
}
fn record(account: &impl Account<AccountId=AccountUuid>) -> Value {
    json!({"id":account.id().expose_uuid().to_string(),"name":account.name().filter(|n| !n.is_empty()),
        "birthdayHeight":u32::from(account.birthday_height()),"accountIndex":null,
        "viewOnly":account.purpose()==AccountPurpose::ViewOnly,"signerAttached":false})
}
fn birthday(v: &Value, parameters: &[u8], genesis: &[u8], p: &crate::Document) -> Result<AccountBirthday> {
    fields(v,&["parameters","genesis","firstScanHeight","priorTreeState","recoverUntilExclusive","source"])?;
    if string(v,"parameters")? != hex::encode(parameters) || string(v,"genesis")? != hex::encode(genesis) { return Err("NETWORK_MISMATCH".into()); }
    if !matches!(string(v,"source")?,"checkpoint"|"light-client") { return Err("INVALID_BIRTHDAY".into()); }
    let first = height(v,"firstScanHeight")?;
    if first == 0 { return Err("INVALID_BIRTHDAY".into()); }
    let recover = if v.get("recoverUntilExclusive").is_some() { Some(height(v,"recoverUntilExclusive")?) } else { None };
    if recover.is_some_and(|h| h < first) { return Err("INVALID_BIRTHDAY".into()); }
    let raw = hex::decode(string(v,"priorTreeState")?).map_err(|_| Failure::from("INVALID_BIRTHDAY"))?;
    if raw.is_empty() || raw.len()>65536 { return Err("INVALID_BIRTHDAY".into()); }
    let state = TreeState::decode(raw.as_slice()).map_err(|_| Failure::from("INVALID_BIRTHDAY"))?;
    let network = match p.network_type() { NetworkType::Main=>"main",NetworkType::Test=>"test",NetworkType::Regtest=>"regtest" };
    if state.network != network || state.height != u64::from(first-1) { return Err("INVALID_BIRTHDAY".into()); }
    AccountBirthday::from_treestate(state,recover.map(Into::into)).map_err(|_| "INVALID_BIRTHDAY".into())
}
fn execute(generation: u32, operation: &str, v: &Value) -> Result<Value> {
    super::DOMAIN.with(|domain| {
        let mut domain = domain.try_borrow_mut().map_err(|_| Failure::from("STORAGE_BUSY"))?;
        let active = domain.active.as_mut().filter(|a| a.generation==generation).ok_or(Failure::from("STALE_HANDLE"))?;
        match operation {
            "account_list" => {
                fields(v,&[])?;
                active.wallet.get_account_ids()?.into_iter().map(|id| {
                    active.wallet.get_account(id)?.map(|a| record(&a)).ok_or("STORAGE_ERROR".into())
                }).collect::<Result<Vec<_>>>().map(Value::Array)
            }
            "account_get" => {
                fields(v,&["accountId"])?;
                Ok(active.wallet.get_account(id(v)?)?.map(|a| record(&a)).unwrap_or(Value::Null))
            }
            "account_import" => {
                fields(v,&["viewingKey","birthday","name","viewOnly","enabledPools"])?;
                let p = active.wallet.params().clone();
                let encoded = string(v,"viewingKey")?;
                if encoded.len()>16384 { return Err("INVALID_ARGUMENT".into()); }
                let key = UnifiedFullViewingKey::decode(&p,encoded).map_err(|_| Failure::from("INVALID_VIEWING_KEY"))?;
                let name = match v.get("name") { None=>"",Some(Value::String(s)) if !s.is_empty() && s.len()<=256=>s, _=>return Err("INVALID_ARGUMENT".into()) };
                let view_only = match v.get("viewOnly") { None=>false,Some(Value::Bool(b))=>*b,_=>return Err("INVALID_ARGUMENT".into()) };
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    let genesis: Vec<u8> = ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?;
                    let birthday = birthday(v.get("birthday").ok_or(Failure::from("INVALID_BIRTHDAY"))?,&active.bytes,&genesis,&p)?;
                    let account = db.import_account_ufvk(name,&key,&birthday,if view_only { AccountPurpose::ViewOnly } else { AccountPurpose::Spending { derivation:None } },None)?;
                    Ok(record(&account))
                })
            }
            _=>Err("UNSUPPORTED".into())
        }
    })
}
#[wasm_bindgen]
pub fn views_call(generation: u32, operation: &str, input: &str) -> std::result::Result<String,String> {
    if input.len()>160000 { return Err("INVALID_ARGUMENT".into()); }
    let value = serde_json::from_str(input).map_err(|_| "INVALID_ARGUMENT".to_string())?;
    execute(generation,operation,&value).map(|v| v.to_string()).map_err(|e|e.0)
}
#[cfg(test)]
#[path = "../../tests/wallet-views-native.rs"]
mod tests;
