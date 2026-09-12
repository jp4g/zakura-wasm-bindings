//! Private UFVK and durable address operations on the existing storage owner.
use wasm_bindgen::prelude::*;
use secrecy::{Secret, SecretString, SecretVec, ExposeSecret};
use bip39::{Language, Mnemonic};
use std::borrow::Cow;
use prost::Message;
use serde_json::{json, Value};
use zcash_client_backend::{data_api::{Account, AccountBirthday, AccountPurpose, AddressSource, WalletRead, WalletWrite}, proto::service::TreeState};
use zcash_client_backend::data_api::ll::LowLevelWalletRead;
use zcash_keys::keys::transparent::gap_limits::{AddressStore, GapLimits};
use zcash_client_sqlite::{AccountUuid, error::SqliteClientError};
use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedIncomingViewingKey, UnifiedAddressRequest, ReceiverRequirement};
use zcash_address::unified::{Ufvk, Fvk, Encoding, Container};
use zcash_transparent::{address::TransparentAddress, keys::{IncomingViewingKey, NonHardenedChildIndex, TransparentKeyScope}};
use zcash_client_backend::wallet::Exposure;
use zcash_keys::address::{Address, UnifiedAddress};
use zip32::DiversifierIndex;
use zcash_protocol::consensus::{Parameters, NetworkType, NetworkUpgrade};

pub(super) const DEFINITIONS: [(&str, &str); 3] = [
    ("ext_viewing_version","CREATE TABLE ext_viewing_version(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL)"),
    ("ext_viewing_accounts","CREATE TABLE ext_viewing_accounts(account_uuid BLOB PRIMARY KEY CHECK(length(account_uuid)=16),metadata TEXT NOT NULL)"),
    ("ext_viewing_addresses","CREATE TABLE ext_viewing_addresses(account_uuid BLOB NOT NULL,address TEXT NOT NULL,diversifier TEXT NOT NULL,PRIMARY KEY(account_uuid,address))"),
];

// Called on the same owned connection after native migrations. Exact extension
// object admission is composed with the accepted storage schema before writes.
pub(super) fn initialize(conn: &mut rusqlite::Connection) -> std::result::Result<(),String> {
    (|| -> std::result::Result<(),rusqlite::Error> {
        let tx=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='ext_viewing_version')",[],|r|r.get(0))?;
        if !exists {
            for (_,sql) in DEFINITIONS {tx.execute_batch(sql)?;}
            tx.execute("INSERT INTO ext_viewing_version VALUES(1,1)",[])?;
        }
        for (name,expected) in DEFINITIONS {
            let actual:String=tx.query_row("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1",[name],|r|r.get(0))?;
            if actual!=expected {return Err(rusqlite::Error::InvalidQuery);}
        }
        let objects:u32=tx.query_row("SELECT count(*) FROM sqlite_schema WHERE tbl_name GLOB 'ext_viewing_*' AND sql IS NOT NULL",[],|r|r.get(0))?;
        let versions:u32=tx.query_row("SELECT count(*) FROM ext_viewing_version WHERE id=1 AND version IN (1,2)",[],|r|r.get(0))?;
        let bad_accounts:u32=tx.query_row("SELECT count(*) FROM ext_viewing_accounts WHERE length(CAST(metadata AS BLOB))>160000 OR NOT json_valid(metadata) OR length(account_uuid)!=16",[],|r|r.get(0))?;
        let bad_addresses:u32=tx.query_row("SELECT count(*) FROM ext_viewing_addresses WHERE length(diversifier)>27 OR length(address)>1024 OR length(account_uuid)!=16",[],|r|r.get(0))?;
        if objects!=3 || versions!=1 || bad_accounts!=0 || bad_addresses!=0 {return Err(rusqlite::Error::InvalidQuery);}
        let version:u32=tx.query_row("SELECT version FROM ext_viewing_version WHERE id=1",[],|r|r.get(0))?;
        if version==1 {
            zcash_client_sqlite::migrate_view_only_spend_support(&tx).map_err(|_|rusqlite::Error::InvalidQuery)?;
            tx.execute("UPDATE ext_viewing_version SET version=2 WHERE id=1",[])?;
        }
        tx.commit()
    })().map_err(|_|"VIEWING_SCHEMA_REQUIRED".into())
}
#[derive(Debug)]
struct Failure(String);
impl From<rusqlite::Error> for Failure { fn from(_: rusqlite::Error) -> Self { Self("STORAGE_ERROR".into()) } }
impl From<SqliteClientError> for Failure { fn from(e: SqliteClientError) -> Self { Self(match e {
    SqliteClientError::AccountCollision(..)=>"ACCOUNT_COLLISION",
    SqliteClientError::Zip32AccountIndexOutOfRange=>"ACCOUNT_INDEX_EXHAUSTED",
    SqliteClientError::ReachedGapLimit(..)=>"ADDRESS_GAP_LIMIT",
    SqliteClientError::ChainHeightUnknown=>"SYNC_REQUIRED",
    SqliteClientError::DiversifierIndexReuse(..)=>"ADDRESS_INDEX_REUSE",
    _=>"BACKEND_ERROR",
}.into()) } }
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
        "birthdayHeight":u32::from(account.birthday_height()),"accountIndex":account.source().key_derivation().map(|d|u32::from(d.account_index())),
        "viewOnly":account.purpose()==AccountPurpose::ViewOnly,"signerAttached":false})
}
fn stored_metadata(ext: &zcash_client_sqlite::ExtensionTransaction<'_>, account: AccountUuid) -> Result<Value> {
    let text:String=ext.query_row("SELECT metadata FROM ext_viewing_accounts WHERE account_uuid=?1 AND length(CAST(metadata AS BLOB))<=160000",[account.expose_uuid()],|r|r.get(0))?;
    serde_json::from_str(&text).map_err(|_|"INVALID_ACCOUNT_METADATA".into())
}
fn stored_record(ext: &zcash_client_sqlite::ExtensionTransaction<'_>, account: &impl Account<AccountId=AccountUuid>) -> Result<Value> {
    let metadata=stored_metadata(ext,account.id())?;
    let mut result=record(account); result["name"]=metadata.get("name").cloned().unwrap_or(Value::Null);
    Ok(result)
}
fn birthday(v: &Value, parameters: &[u8], genesis: &[u8], p: &crate::Document) -> Result<AccountBirthday> {
    if v.as_str()==Some("fullScan") {
        let hash=zcash_primitives::block::BlockHash::try_from_slice(genesis).ok_or(Failure::from("NETWORK_MISMATCH"))?;
        return Ok(AccountBirthday::from_parts(zcash_client_backend::data_api::chain::ChainState::empty(0u32.into(),hash),None));
    }
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
fn birthday_from_metadata(v:&Value, parameters:&[u8], genesis:&[u8], p:&crate::Document)->Result<AccountBirthday> {
    birthday(v.get("birthday").ok_or(Failure::from("INVALID_ACCOUNT_METADATA"))?,parameters,genesis,p)
}
fn request(v: &Value) -> Result<UnifiedAddressRequest> {
    let Some(r) = v.get("request") else { return Ok(UnifiedAddressRequest::AllAvailableKeys); };
    fields(r,&["format","transparent","sapling","ironwood"])?;
    if string(r,"format")? == "transparent" { return Err("UNSUPPORTED_ADDRESS_FORMAT".into()); }
    if string(r,"format")? != "unified" { return Err("INVALID_ARGUMENT".into()); }
    if r.as_object().unwrap().len()==1 { return Ok(UnifiedAddressRequest::AllAvailableKeys); }
    let requirement = |name| -> Result<ReceiverRequirement> { match string(r,name)? {
        "require"=>Ok(ReceiverRequirement::Require),"omit"=>Ok(ReceiverRequirement::Omit),
        "allow" if name=="transparent"=>Ok(ReceiverRequirement::Allow),_=>Err("INVALID_ARGUMENT".into())
    }};
    UnifiedAddressRequest::custom(requirement("ironwood")?,requirement("sapling")?,requirement("transparent")?).map_err(|_| "INVALID_ARGUMENT".into())
}
fn index(v: &Value) -> Result<DiversifierIndex> {
    let s=string(v,"index")?;
    let n=s.parse::<u128>().map_err(|_| Failure::from("INVALID_ARGUMENT"))?;
    if n >= (1u128<<88) || n.to_string()!=s { return Err("INVALID_ARGUMENT".into()); }
    Ok(DiversifierIndex::from(<[u8;11]>::try_from(&n.to_le_bytes()[..11]).unwrap()))
}
fn address_record(p: &crate::Document, key: &UnifiedIncomingViewingKey, ua: &UnifiedAddress, j: DiversifierIndex) -> Result<Value> {
    let req = UnifiedAddressRequest::custom(
        if ua.has_orchard() {ReceiverRequirement::Require} else {ReceiverRequirement::Omit},
        if ua.has_sapling() {ReceiverRequirement::Require} else {ReceiverRequirement::Omit},
        if ua.has_transparent() {ReceiverRequirement::Require} else {ReceiverRequirement::Omit},
    ).map_err(|_| Failure::from("INVALID_STORED_ADDRESS"))?;
    let expected = key.address(j,req).map_err(|_| Failure::from("INVALID_STORED_ADDRESS"))?;
    if &expected != ua || !ua.unknown().is_empty() { return Err("INVALID_STORED_ADDRESS".into()); }
    let mut receivers=Vec::new(); let mut pools=Vec::new();
    if ua.has_transparent() { receivers.push("p2pkh"); pools.push("transparent"); }
    if ua.has_sapling() { receivers.push("sapling"); pools.push("sapling"); }
    if ua.has_orchard() { receivers.push("orchard"); pools.push("ironwood"); }
    let mut bytes=[0u8;16]; bytes[..11].copy_from_slice(j.as_bytes());
    Ok(json!({"address":ua.encode(p),"receiverTypes":receivers,"intendedPools":pools,"index":u128::from_le_bytes(bytes).to_string()}))
}
fn transparent_record(p:&crate::Document,key:&UnifiedIncomingViewingKey,address:&TransparentAddress,j:NonHardenedChildIndex)->Result<Value> {
    let expected=key.transparent().as_ref().ok_or(Failure::from("MISSING_AUTHORITY"))?.derive_address(j).map_err(|_|Failure::from("ADDRESS_UNAVAILABLE"))?;
    if &expected!=address {return Err("INVALID_STORED_ADDRESS".into());}
    Ok(json!({"address":Address::Transparent(*address).encode(p),"index":j.index().to_string(),"receiverTypes":["p2pkh"],"intendedPools":["transparent"]}))
}
fn exposed_transparent<W: WalletRead<AccountId=AccountUuid,Error=SqliteClientError>>(db:&W,ext:&zcash_client_sqlite::ExtensionTransaction<'_>,p:&crate::Document,account:AccountUuid)->Result<Vec<Value>> {
    let key=db.get_account(account)?.ok_or(Failure::from("ACCOUNT_NOT_FOUND"))?.uivk();
    let raw:String=ext.query_row("SELECT json_group_array(json_object('address',address,'index',diversifier)) FROM (SELECT address,diversifier FROM ext_viewing_addresses WHERE account_uuid=?1 ORDER BY rowid)",[account.expose_uuid()],|r|r.get(0))?;
    let rows:Vec<Value>=serde_json::from_str(&raw).map_err(|_|Failure::from("INVALID_STORED_ADDRESS"))?;
    rows.into_iter().map(|row| {
        let Some(Address::Transparent(t))=Address::decode(p,string(&row,"address")?) else {return Err("INVALID_STORED_ADDRESS".into());};
        let meta=db.get_transparent_address_metadata(account,&t)?.ok_or(Failure::from("INVALID_STORED_ADDRESS"))?;
        let j=meta.source().address_index().ok_or(Failure::from("INVALID_STORED_ADDRESS"))?;
        if meta.source().scope()!=Some(TransparentKeyScope::EXTERNAL)||!matches!(meta.exposure(),Exposure::Exposed{..})||row["index"]!=j.index().to_string(){return Err("INVALID_STORED_ADDRESS".into());}
        transparent_record(p,&key,&t,j)
    }).collect()
}
fn address_list<W: WalletRead<AccountId=AccountUuid,Error=SqliteClientError>>(db: &W, p: &crate::Document, account: AccountUuid) -> Result<Vec<Value>> {
    let key=db.get_account(account)?.ok_or(Failure::from("ACCOUNT_NOT_FOUND"))?.uivk();
    db.list_addresses(account)?.into_iter().map(|info| {
        let AddressSource::Derived { diversifier_index, transparent_key_scope }=info.source();
        match info.address() {
            Address::Unified(ua) if transparent_key_scope.is_none()=>address_record(p,&key,ua,diversifier_index),
            Address::Transparent(t) if transparent_key_scope==Some(TransparentKeyScope::EXTERNAL)=>{
                let j=NonHardenedChildIndex::try_from(diversifier_index).map_err(|_|Failure::from("INVALID_STORED_ADDRESS"))?;
                transparent_record(p,&key,t,j)
            },
            _=>Err("UNSUPPORTED_STORED_ADDRESS".into()),
        }
    }).collect()
}
fn execute(generation: u32, operation: &str, v: &Value) -> Result<Value> {
    super::DOMAIN.with(|domain| {
        let mut domain = domain.try_borrow_mut().map_err(|_| Failure::from("STORAGE_BUSY"))?;
        let active = domain.active.as_mut().filter(|a| a.generation==generation).ok_or(Failure::from("STALE_HANDLE"))?;
        match operation {
            "account_list" => {
                fields(v,&[])?;
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    db.get_account_ids()?.into_iter().map(|id| {
                        stored_record(ext,&db.get_account(id)?.ok_or(Failure::from("STORAGE_ERROR"))?)
                    }).collect::<Result<Vec<_>>>().map(Value::Array)
                })
            }
            "account_get" => {
                fields(v,&["accountId"])?;
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    db.get_account(id(v)?)?.map(|a| stored_record(ext,&a)).transpose().map(|v|v.unwrap_or(Value::Null))
                })
            }
            "address_current" | "address_list" | "address_at" | "address_next" => {
                fields(v,match operation { "address_list"=>&["accountId"][..],"address_at"=>&["accountId","request","index"][..],_=>&["accountId","request"][..] })?;
                let account=id(v)?;
                let p=active.wallet.params().clone();
                let key=active.wallet.get_account(account)?.ok_or(Failure::from("ACCOUNT_NOT_FOUND"))?.uivk();
                if operation=="address_list" {
                    return active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                        let mut rows=address_list(db,&p,account)?;
                        for row in exposed_transparent(db,ext,&p,account)? {if !rows.contains(&row){rows.push(row);}}
                        Ok(Value::Array(rows))
                    });
                }
                if v.get("request").and_then(|r|r.get("format")).and_then(Value::as_str)==Some("transparent") {
                    fields(&v["request"],&["format"])?;
                    if !key.has_transparent(){return Err("RECEIVER_UNAVAILABLE".into());}
                    return active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                        if operation=="address_current" {return Ok(exposed_transparent(db,ext,&p,account)?.last().map(|r|r["address"].clone()).unwrap_or(Value::Null));}
                        let gap=GapLimits::default().external();
                        let start=db.find_gap_start(db.get_account_ref(account)?,TransparentKeyScope::EXTERNAL,gap)?.ok_or(Failure::from("ADDRESS_GAP_LIMIT"))?;
                        let end=start.index().saturating_add(gap).min(1<<31);
                        let receivers=db.get_transparent_receivers(account,false,false)?;
                        let requested=if operation=="address_at" {Some(NonHardenedChildIndex::try_from(index(v)?).map_err(|_|Failure::from("ADDRESS_UNAVAILABLE"))?)} else {None};
                        let selected=receivers.iter().filter_map(|(t,m)|{
                            let j=m.source().address_index()?;
                            (m.source().scope()==Some(TransparentKeyScope::EXTERNAL)&&j.index()<end&&requested.map_or(matches!(m.exposure(),Exposure::Unknown),|r|r==j)).then_some((*t,j))
                        }).min_by_key(|(_,j)|j.index()).ok_or(Failure::from("ADDRESS_GAP_LIMIT"))?;
                        let row=transparent_record(&p,&key,&selected.0,selected.1)?;
                        let height=db.chain_height()?.unwrap_or(db.get_account(account)?.ok_or(Failure::from("ACCOUNT_NOT_FOUND"))?.birthday_height());
                        db.mark_transparent_addresses_exposed(&[(selected.0,height)])?;
                        ext.execute("INSERT OR IGNORE INTO ext_viewing_addresses(account_uuid,address,diversifier) VALUES(?1,?2,?3)",rusqlite::params![account.expose_uuid(),string(&row,"address")?,string(&row,"index")?])?;
                        if !exposed_transparent(db,ext,&p,account)?.contains(&row){return Err("INVALID_STORED_ADDRESS".into());}
                        Ok(row)
                    });
                }
                let req=request(v)?;
                let requirements=key.receiver_requirements(req).map_err(|_| Failure::from("RECEIVER_UNAVAILABLE"))?;
                if operation=="address_current" {
                    let current=active.wallet.get_last_generated_address_matching(account,req)?;
                    let verified=address_list(&active.wallet,&p,account)?;
                    return match current {
                        None=>Ok(Value::Null),
                        Some(ua)=>{
                            for (r,present) in [(requirements.orchard(),ua.has_orchard()),(requirements.sapling(),ua.has_sapling()),(requirements.p2pkh(),ua.has_transparent())] {
                                if (r==ReceiverRequirement::Require && !present)||(r==ReceiverRequirement::Omit && present) {return Err("INVALID_STORED_ADDRESS".into());}
                            }
                            let encoded=ua.encode(&p); if verified.iter().any(|a|a["address"]==encoded) {Ok(json!(encoded))} else {Err("INVALID_STORED_ADDRESS".into())} }
                    };
                }
                let j=if operation=="address_at" { Some(index(v)?) } else { None };
                active.wallet.transactionally(|db| -> Result<Value> {
                    let (ua,j)=if let Some(j)=j {
                        let expected=key.address(j,req).map_err(|_|Failure::from("ADDRESS_UNAVAILABLE"))?;
                        if expected.has_transparent() {
                            let receivers=db.get_transparent_receivers(account,false,false)?;
                            let scope=receivers.values().find_map(|m|m.source().scope()).ok_or(Failure::from("ADDRESS_GAP_LIMIT"))?;
                            let gap=GapLimits::default().external();
                            let start=db.find_gap_start(db.get_account_ref(account)?,scope,gap)?.ok_or(Failure::from("ADDRESS_GAP_LIMIT"))?;
                            let mut bytes=[0u8;16];bytes[..11].copy_from_slice(j.as_bytes());
                            if u128::from_le_bytes(bytes)>=u128::from(start.index().saturating_add(gap).min(1<<31)) {return Err("ADDRESS_GAP_LIMIT".into());}
                        }
                        (db.get_address_for_index(account,j,req)?.ok_or(Failure::from("ADDRESS_UNAVAILABLE"))?,j)
                    } else {
                        db.get_next_available_address(account,req)?.ok_or(Failure::from("ACCOUNT_NOT_FOUND"))?
                    };
                    let record=address_record(&p,&key,&ua,j)?;
                    if !address_list(db,&p,account)?.contains(&record) { return Err("INVALID_STORED_ADDRESS".into()); }
                    Ok(record)
                })
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
                let mut key = UnifiedFullViewingKey::decode(&p,encoded).map_err(|_| Failure::from("INVALID_VIEWING_KEY"))?;
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
                    if pools.contains(pool) && upgrade.is_some_and(|nu| p.activation_height(nu).is_none()) { return Err("POOL_UNAVAILABLE".into()); }
                }
                let projected=Ufvk::try_from_items(container.items_as_parsed().iter().filter(|k| match k { Fvk::P2pkh(_)=>pools.contains("transparent"),Fvk::Sapling(_)=>pools.contains("sapling"),Fvk::Orchard(_)=>pools.contains("ironwood"),_=>false }).cloned().collect()).map_err(|_|Failure::from("UNSUPPORTED_POOL_PROJECTION"))?;
                key=UnifiedFullViewingKey::parse(&projected).map_err(|_|Failure::from("INVALID_VIEWING_KEY"))?;
                let name = match v.get("name") { None=>"",Some(Value::String(s)) if s.len()<=256=>s, _=>return Err("INVALID_ARGUMENT".into()) };
                let view_only = match v.get("viewOnly") { None=>false,Some(Value::Bool(b))=>*b,_=>return Err("INVALID_ARGUMENT".into()) };
                active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
                    let genesis: Vec<u8> = ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?;
                    let birthday = birthday(v.get("birthday").ok_or(Failure::from("INVALID_BIRTHDAY"))?,&active.bytes,&genesis,&p)?;
                    // Every checkpoint survives reopen and participates in coherence.
                    for existing in db.get_account_ids()? {
                        let metadata=stored_metadata(ext,existing)?;
                        let prior=birthday_from_metadata(&metadata,&active.bytes,&genesis,&p)?;
                        if prior.prior_chain_state().block_height()==birthday.prior_chain_state().block_height() && prior.prior_chain_state()!=birthday.prior_chain_state() { return Err("INCOHERENT_BIRTHDAY".into()); }
                    }
                    if db.get_account_for_ufvk(&key)?.is_some() { return Err("ACCOUNT_COLLISION".into()); }
                    if db.get_block_hash(birthday.prior_chain_state().block_height())?.is_some_and(|h| h != birthday.prior_chain_state().block_hash()) { return Err("INCOHERENT_BIRTHDAY".into()); }
                    let (default_address,default_index)=key.default_address(UnifiedAddressRequest::AllAvailableKeys).map_err(|_|Failure::from("ADDRESS_UNAVAILABLE"))?;
                    if default_address.has_transparent() && NonHardenedChildIndex::try_from(default_index).map_or(true,|i|i.index()>=GapLimits::default().external()) {return Err("ADDRESS_GAP_LIMIT".into());}
                    let account = db.import_account_ufvk(name,&key,&birthday,if view_only { AccountPurpose::ViewOnly } else { AccountPurpose::Spending { derivation:None } },None)?;
                    ext.execute("INSERT INTO ext_viewing_accounts(account_uuid,metadata) VALUES(?1,?2)",rusqlite::params![account.id().expose_uuid(),v.to_string()])?;
                    stored_record(ext,&account)
                })
            }
            _=>Err("UNSUPPORTED".into())
        }
    })
}
// Private prerequisite: caller supplies a birthday trust input, not verified current state.
// Vec ownership crosses generated glue once and immediately enters a zeroizing wrapper.
#[wasm_bindgen]
pub fn views_seed_call(generation: u32, operation: &str, input: &str, seed: Vec<u8>) -> std::result::Result<String,String> {
    let seed=SecretVec::new(seed);
    execute_seed(generation,operation,input,&seed).map(|v|v.to_string()).map_err(|e|e.0)
}
fn normalized_secret(bytes: &SecretVec<u8>, limit: usize) -> Result<SecretString> {
    if bytes.expose_secret().len()>limit {return Err("INVALID_ARGUMENT".into());}
    let text=std::str::from_utf8(bytes.expose_secret()).map_err(|_|Failure::from("INVALID_ARGUMENT"))?;
    let mut text=Cow::Borrowed(text);
    Mnemonic::normalize_utf8_cow(&mut text);
    let text=SecretString::new(text.into_owned());
    if text.expose_secret().len()>limit {return Err("INVALID_ARGUMENT".into());}
    Ok(text)
}
#[wasm_bindgen]
pub fn views_mnemonic_call(generation: u32, input: &str, mnemonic: Vec<u8>, passphrase: Vec<u8>) -> std::result::Result<String,String> {
    let mnemonic=SecretVec::new(mnemonic);
    let passphrase=SecretVec::new(passphrase);
    (|| -> Result<Value> {
        let mnemonic=normalized_secret(&mnemonic,4096)?;
        let passphrase=normalized_secret(&passphrase,65536)?;
        let mnemonic=Mnemonic::parse_in_normalized(Language::English,mnemonic.expose_secret()).map_err(|_|Failure::from("INVALID_ARGUMENT"))?;
        let seed=Secret::new(mnemonic.to_seed_normalized(passphrase.expose_secret()));
        let seed=SecretVec::new(seed.expose_secret().to_vec());
        execute_seed(generation,"account_import_hd",input,&seed)
    })().map(|v|v.to_string()).map_err(|e|e.0)
}
fn execute_seed(generation: u32, operation: &str, input: &str, seed: &SecretVec<u8>) -> Result<Value> {
    if input.len()>160000 || !matches!(seed.expose_secret().len(),32|64) {return Err("INVALID_ARGUMENT".into());}
    let v:Value=serde_json::from_str(input).map_err(|_|Failure::from("INVALID_ARGUMENT"))?;
    let account_index=match operation {
        "account_import_hd"=>{fields(&v,&["accountIndex","birthday","name","enabledPools"])?;
            Some(zip32::AccountId::try_from(height(&v,"accountIndex")?).map_err(|_|Failure::from("INVALID_ARGUMENT"))?)},
        "account_create_hd"=>{fields(&v,&["birthday","name","enabledPools"])?;None},
        _=>return Err("UNSUPPORTED".into()),
    };
    let defaults=json!(["transparent","sapling","ironwood"]);
    let enabled=v.get("enabledPools").unwrap_or(&defaults).as_array().ok_or(Failure::from("INVALID_ARGUMENT"))?;
    let mut pools=std::collections::BTreeSet::new();
    for pool in enabled {
        let pool=pool.as_str().ok_or(Failure::from("INVALID_ARGUMENT"))?;
        if !matches!(pool,"transparent"|"sapling"|"ironwood") || !pools.insert(pool) {return Err("INVALID_ARGUMENT".into());}
    }
    if pools.len()!=3 {return Err("UNSUPPORTED_HD_POOLS".into());}
    let name=match v.get("name") {None=>"",Some(Value::String(s)) if s.len()<=256=>s,_=>return Err("INVALID_ARGUMENT".into())};
    super::DOMAIN.with(|domain| {
        let mut domain=domain.try_borrow_mut().map_err(|_|Failure::from("STORAGE_BUSY"))?;
        let active=domain.active.as_mut().filter(|a|a.generation==generation).ok_or(Failure::from("STALE_HANDLE"))?;
        let p=active.wallet.params().clone();
        if [NetworkUpgrade::Sapling,NetworkUpgrade::Nu6_3].iter().any(|nu|p.activation_height(*nu).is_none()) {return Err("POOL_UNAVAILABLE".into());}
        active.wallet.transactionally_with_extension(|db,ext| -> Result<Value> {
            let genesis:Vec<u8>=ext.query_row("SELECT genesis FROM ext_wallet_storage WHERE id=1",[],|r|r.get(0))?;
            let birthday_value=v.get("birthday").ok_or(Failure::from("INVALID_BIRTHDAY"))?;
            let birthday=birthday(birthday_value,&active.bytes,&genesis,&p)?;
            let existing=db.get_account_ids()?;
            for account in &existing {
                let prior=birthday_from_metadata(&stored_metadata(ext,*account)?,&active.bytes,&genesis,&p)?;
                if prior.prior_chain_state().block_height()==birthday.prior_chain_state().block_height() && prior.prior_chain_state()!=birthday.prior_chain_state() {return Err("INCOHERENT_BIRTHDAY".into());}
            }
            if db.get_block_hash(birthday.prior_chain_state().block_height())?.is_some_and(|h|h!=birthday.prior_chain_state().block_hash()) {return Err("INCOHERENT_BIRTHDAY".into());}
            let (account,usk)=if let Some(index)=account_index {
                db.import_account_hd(name,seed,index,&birthday,None)?
            } else {
                let (id,usk)=db.create_account(name,seed,&birthday,None)?;
                (db.get_account(id)?.ok_or(Failure::from("STORAGE_ERROR"))?,usk)
            };
            // No signer exists in this prerequisite. Ordinary drop is not a RAM-erasure claim.
            drop(usk);
            // Native add_account can upgrade a partial UFVK and return its old UUID.
            if existing.contains(&account.id()) {return Err("ACCOUNT_COLLISION".into());}
            let (ua,j)=account.uivk().default_address(UnifiedAddressRequest::AllAvailableKeys).map_err(|_|Failure::from("ADDRESS_UNAVAILABLE"))?;
            if ua.has_transparent() && NonHardenedChildIndex::try_from(j).map_or(true,|i|i.index()>=GapLimits::default().external()) {return Err("ADDRESS_GAP_LIMIT".into());}
            let metadata=json!({"birthday":birthday_value,"name":v.get("name"),"enabledPools":defaults});
            ext.execute("INSERT INTO ext_viewing_accounts(account_uuid,metadata) VALUES(?1,?2)",rusqlite::params![account.id().expose_uuid(),metadata.to_string()])?;
            stored_record(ext,&account)
        })
    })
}
#[wasm_bindgen]
pub fn views_call(generation: u32, operation: &str, input: &str) -> std::result::Result<String,String> {
    if input.len()>160000 { return Err("INVALID_ARGUMENT".into()); }
    let value = serde_json::from_str(input).map_err(|_| "INVALID_ARGUMENT".to_string())?;
    execute(generation,operation,&value).map(|v| v.to_string()).map_err(|e|e.0)
}
#[cfg(test)]
#[path = "../../tests/wallet-views/native.rs"]
mod tests;
