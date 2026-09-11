//! Private UFVK and durable address operations on the existing storage owner.
use wasm_bindgen::prelude::*;
use prost::Message;
use serde_json::{json, Value};
use zcash_client_backend::{data_api::{Account, AccountBirthday, AccountPurpose, WalletRead, WalletWrite}, proto::service::TreeState};
use zcash_client_sqlite::{AccountUuid, error::SqliteClientError};
use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedIncomingViewingKey};
use zcash_address::unified::{Ufvk, Fvk, Encoding, Container};
use zcash_protocol::consensus::{Parameters, NetworkType, NetworkUpgrade};

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
    // This private encoding accepts the pinned prost canonical form only: unknown,
    // duplicate and otherwise unconsumed fields cannot silently disappear.
    if state.encode_to_vec() != raw { return Err("INVALID_BIRTHDAY".into()); }
    let trees = [
        (state.sapling_tree.as_str(), NetworkUpgrade::Sapling),
        (state.orchard_tree.as_str(), NetworkUpgrade::Nu5),
        (state.ironwood_tree.as_str(), NetworkUpgrade::Nu6_3),
    ];
    for (i,(encoded,upgrade)) in trees.into_iter().enumerate() {
        let mut canonical = Vec::new();
        let empty = match i {
            0 => { let tree = state.sapling_tree().map_err(|_| Failure::from("INVALID_BIRTHDAY"))?; zcash_primitives::merkle_tree::write_commitment_tree(&tree,&mut canonical).map_err(|_| Failure::from("INVALID_BIRTHDAY"))?; tree.size()==0 },
            _ => { let tree = if i==1 { state.orchard_tree() } else { state.ironwood_tree() }.map_err(|_| Failure::from("INVALID_BIRTHDAY"))?; zcash_primitives::merkle_tree::write_commitment_tree(&tree,&mut canonical).map_err(|_| Failure::from("INVALID_BIRTHDAY"))?; tree.size()==0 },
        };
        let active = p.is_nu_active(upgrade,(first-1).into());
        if (!encoded.is_empty() && encoded != hex::encode(canonical)) || (active && encoded.is_empty()) || (!active && !empty) { return Err("INVALID_BIRTHDAY".into()); }
    }
    let network = match p.network_type() { NetworkType::Main=>"main",NetworkType::Test=>"test",NetworkType::Regtest=>"regtest" };
    if state.network != network || state.height != u64::from(first-1) { return Err("INVALID_BIRTHDAY".into()); }
    let decoded = AccountBirthday::from_treestate(state,recover.map(Into::into)).map_err(|_| Failure::from("INVALID_BIRTHDAY"))?;
    if first==1 && decoded.prior_chain_state().block_hash().0.as_slice() != genesis { return Err("NETWORK_MISMATCH".into()); }
    Ok(decoded)
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
                if UnifiedIncomingViewingKey::decode(&p,encoded).is_ok() { return Err("INCOMING_ONLY_WALLET_UNSUPPORTED".into()); }
                let (network,container) = Ufvk::decode(encoded).map_err(|_| Failure::from("INVALID_VIEWING_KEY"))?;
                if network != p.network_type() { return Err("NETWORK_MISMATCH".into()); }
                if container.items_as_parsed().iter().any(|k| !matches!(k,Fvk::Orchard(_)|Fvk::Sapling(_)|Fvk::P2pkh(_))) { return Err("UNSUPPORTED_VIEWING_COMPONENT".into()); }
                let key = UnifiedFullViewingKey::decode(&p,encoded).map_err(|_| Failure::from("INVALID_VIEWING_KEY"))?;
                let mut pools = std::collections::BTreeSet::new();
                let defaults = json!(["transparent","sapling","ironwood"]);
                let enabled = v.get("enabledPools").unwrap_or(&defaults).as_array().ok_or(Failure::from("INVALID_ARGUMENT"))?;
                if enabled.is_empty() { return Err("INVALID_ARGUMENT".into()); }
                for pool in enabled {
                    let pool = pool.as_str().ok_or(Failure::from("INVALID_ARGUMENT"))?;
                    if !matches!(pool,"transparent"|"sapling"|"ironwood") || !pools.insert(pool) { return Err("INVALID_ARGUMENT".into()); }
                }
                for (pool,present,upgrade) in [("transparent",key.transparent().is_some(),None),("sapling",key.sapling().is_some(),Some(NetworkUpgrade::Sapling)),("ironwood",key.orchard().is_some(),Some(NetworkUpgrade::Nu6_3))] {
                    if pools.contains(pool) && !present { return Err("MISSING_AUTHORITY".into()); }
                    // Until the parent supplies an extension migration that preserves
                    // original containers, reject projections that would discard keys.
                    if !pools.contains(pool) && present { return Err("UNSUPPORTED_POOL_PROJECTION".into()); }
                    if pools.contains(pool) && upgrade.is_some_and(|nu| p.activation_height(nu).is_none()) { return Err("POOL_UNAVAILABLE".into()); }
                }
                let name = match v.get("name") { None=>"",Some(Value::String(s)) if !s.is_empty() && s.len()<=256=>s, _=>return Err("INVALID_ARGUMENT".into()) };
                let view_only = match v.get("viewOnly") { None=>false,Some(Value::Bool(b))=>*b,_=>return Err("INVALID_ARGUMENT".into()) };
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    let genesis: Vec<u8> = ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?;
                    let birthday = birthday(v.get("birthday").ok_or(Failure::from("INVALID_BIRTHDAY"))?,&active.bytes,&genesis,&p)?;
                    if db.get_account_for_ufvk(&key)?.is_some() { return Err("ACCOUNT_COLLISION".into()); }
                    if db.get_block_hash(birthday.prior_chain_state().block_height())?.is_some_and(|h| h != birthday.prior_chain_state().block_hash()) { return Err("INCOHERENT_BIRTHDAY".into()); }
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
