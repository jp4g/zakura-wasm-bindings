// Synthetic test authority derived from fixed bytes. NEVER use for production funds.
use super::*;
use serde_json::{json, Value};
use zcash_client_backend::proto::service::TreeState;
use prost::Message;
use zcash_keys::keys::UnifiedSpendingKey;
const PARAMS: &[u8] = br#"{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}"#;
fn fixture(seed: u8) -> Value {
    let p = crate::Document::parse(PARAMS).unwrap();
    let key = UnifiedSpendingKey::from_seed(&p, &[seed; 32], zip32::AccountId::ZERO).unwrap().to_unified_full_viewing_key();
    let state = TreeState { network: "regtest".into(), height: 99, hash: "07".repeat(32), time: 1,
        sapling_tree: "000000".into(), orchard_tree: "000000".into(), ironwood_tree: "000000".into() };
    json!({"viewingKey":key.encode(&p), "birthday":{"parameters":hex::encode(PARAMS),"genesis":"03".repeat(32),"firstScanHeight":100,"priorTreeState":hex::encode(state.encode_to_vec()),"source":"checkpoint","recoverUntilExclusive":120}})
}
fn open() -> (String,u32) {
    let root = std::env::var("WALLET_TEST_ROOT").unwrap();
    let path = format!("{root}/views-{}.db", uuid::Uuid::new_v4());
    let generation = crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    // Native control installs the proposed same-DB hook explicitly. Production
    // WASM uses the parent lifecycle patch; unpatched production fails closed.
    crate::wallet::storage_close(generation).unwrap();
    let mut conn=rusqlite::Connection::open(&path).unwrap(); super::initialize(&mut conn).unwrap(); conn.close().unwrap();
    let generation=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    (path,generation)
}
fn call(g: u32, op: &str, input: Value) -> std::result::Result<Value,String> {
    views_call(g,op,&input.to_string()).map(|s| serde_json::from_str(&s).unwrap())
}
#[test]
fn ufvk_import_retains_native_uuid_and_tracking_after_reopen() {
    let (path,g) = open();
    let account = call(g,"account_import",fixture(7)).unwrap();
    assert_eq!(account["name"],Value::Null);
    assert_eq!(account["birthdayHeight"],100);
    assert_eq!(account["viewOnly"],false);
    assert_eq!(account["signerAttached"],false);
    assert_eq!(account["accountIndex"],Value::Null);
    uuid::Uuid::parse_str(account["id"].as_str().unwrap()).unwrap();
    crate::wallet::storage_close(g).unwrap();
    let g2 = crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g2,"account_list",json!({})).unwrap(),json!([account]));
    assert_eq!(call(g2,"account_get",json!({"accountId":account["id"]})).unwrap(),account);
    assert_eq!(call(g,"account_list",json!({})).unwrap_err(),"STALE_HANDLE");
    crate::wallet::storage_close(g2).unwrap();
}
#[test]
fn birthday_rejects_unconsumed_proto_and_tree_bytes_before_mutation() {
    let (_path,g)=open();
    let good=fixture(8);
    for tree_suffix in [true,false] {
        let mut bad=good.clone();
        let mut bytes=hex::decode(bad["birthday"]["priorTreeState"].as_str().unwrap()).unwrap();
        if tree_suffix {
            let mut state=TreeState::decode(bytes.as_slice()).unwrap();
            state.ironwood_tree.push_str("00"); bytes=state.encode_to_vec();
        } else { bytes.extend_from_slice(&[0x40,0x01]); }
        bad["birthday"]["priorTreeState"]=json!(hex::encode(bytes));
        assert_eq!(call(g,"account_import",bad).unwrap_err(),"INVALID_BIRTHDAY");
        assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([]));
    }
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn import_enforces_authority_scope_and_rejects_overlapping_upgrades() {
    use zcash_address::unified::{Ufvk,Fvk,Encoding,Container};
    let (_path,g)=open();
    let full=fixture(9);
    let (network,container)=Ufvk::decode(full["viewingKey"].as_str().unwrap()).unwrap();
    let key=Ufvk::try_from_items(container.items_as_parsed().iter().filter(|k|matches!(k,Fvk::Sapling(_))).cloned().collect()).unwrap().encode(&network);
    let mut missing=full.clone(); missing["viewingKey"]=json!(key);
    assert_eq!(call(g,"account_import",missing.clone()).unwrap_err(),"MISSING_AUTHORITY");
    missing["enabledPools"]=json!(["sapling"]); missing["viewOnly"]=json!(true);
    let account=call(g,"account_import",missing).unwrap();
    assert_eq!(account["viewOnly"],true);
    assert_eq!(call(g,"account_import",full).unwrap_err(),"ACCOUNT_COLLISION");
    assert_eq!(call(g,"account_get",json!({"accountId":account["id"]})).unwrap(),account);
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn durable_addresses_use_exact_88_bit_indices_and_survive_reopen() {
    let (path,g)=open();
    let account=call(g,"account_import",fixture(10)).unwrap();
    let args=json!({"accountId":account["id"]});
    let before=call(g,"address_list",args.clone()).unwrap();
    let current=call(g,"address_current",args.clone()).unwrap();
    assert_eq!(call(g,"address_list",args.clone()).unwrap(),before,"current never allocates");
    assert!(current.is_string(),"native import exposes default address");
    let next=call(g,"address_next",args.clone()).unwrap();
    assert_eq!(next["receiverTypes"],json!(["p2pkh","sapling","orchard"]));
    assert_eq!(next["intendedPools"],json!(["transparent","sapling","ironwood"]));
    let mut exact=args.clone();
    exact["request"]=json!({"format":"unified","transparent":"omit","sapling":"omit","ironwood":"require"});
    exact["index"]=json!("309485009821345068724781055");
    let high=call(g,"address_at",exact.clone()).unwrap();
    assert_eq!(high["index"],exact["index"]);
    assert_eq!(high["receiverTypes"],json!(["orchard"]));
    let list=call(g,"address_list",args.clone()).unwrap();
    exact["index"]=json!("309485009821345068724781056");
    assert_eq!(call(g,"address_at",exact).unwrap_err(),"INVALID_ARGUMENT");
    assert_eq!(call(g,"address_list",args.clone()).unwrap(),list);
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g,"address_list",args).unwrap(),list);
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn explicit_full_scan_uses_bound_genesis_without_current_tip_substitution() {
    let (path,g)=open();
    let mut input=fixture(11); input["birthday"]=json!("fullScan");
    let account=call(g,"account_import",input).unwrap();
    assert_eq!(account["birthdayHeight"],1);
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(path).unwrap();
    let recover:Option<u32>=conn.query_row("SELECT recover_until_height FROM accounts",[],|r|r.get(0)).unwrap();
    assert_eq!(recover,None);
}
#[test]
fn failed_native_commit_rolls_back_without_consuming_address() {
    let (path,g)=open();
    let account=call(g,"account_import",fixture(12)).unwrap();
    let args=json!({"accountId":account["id"]});
    let before=call(g,"address_list",args.clone()).unwrap();
    let reader=rusqlite::Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN; SELECT * FROM accounts;").unwrap();
    assert!(call(g,"address_next",args.clone()).is_err());
    assert_eq!(call(g,"address_list",args.clone()).unwrap(),before);
    reader.execute_batch("ROLLBACK").unwrap();
    let allocated=call(g,"address_next",args.clone()).unwrap();
    let params=crate::Document::parse(PARAMS).unwrap();
    let key=UnifiedFullViewingKey::decode(&params,fixture(12)["viewingKey"].as_str().unwrap()).unwrap();
    let first_index=before[0]["index"].as_str().unwrap().parse::<u64>().unwrap()+1;
    let expected=key.find_address(DiversifierIndex::from(first_index),UnifiedAddressRequest::AllAvailableKeys).unwrap();
    assert_eq!(allocated["address"],expected.0.encode(&params));
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(path).unwrap();
    assert_eq!(conn.query_row("PRAGMA integrity_check",[],|r|r.get::<_,String>(0)).unwrap(),"ok");
}
#[test]
fn emit_synthetic_upstream_fixture_for_real_wasm_tests() {
    let root=std::env::var("WALLET_TEST_ROOT").unwrap();
    let p=crate::Document::parse(PARAMS).unwrap();
    let input=fixture(7);
    let key=UnifiedFullViewingKey::decode(&p,input["viewingKey"].as_str().unwrap()).unwrap();
    let (ua,j)=key.default_address(UnifiedAddressRequest::AllAvailableKeys).unwrap();
    let output=json!({"warning":"SYNTHETIC TEST AUTHORITY ONLY. NEVER USE FOR PRODUCTION FUNDS.","import":input,
        "uivk":key.to_unified_incoming_viewing_key().encode(&p),"defaultAddress":address_record(&p,&key.to_unified_incoming_viewing_key(),&ua,j).unwrap()});
    std::fs::write(format!("{root}/views-fixture.json"),output.to_string()).unwrap();
}
#[test]
fn birthday_checkpoint_is_durable_and_conflicting_hash_rejects() {
    let (path,g)=open();
    let original=fixture(14);
    let account=call(g,"account_import",original.clone()).unwrap();
    let mut conflicting=fixture(15);
    let raw=hex::decode(conflicting["birthday"]["priorTreeState"].as_str().unwrap()).unwrap();
    let mut state=TreeState::decode(raw.as_slice()).unwrap();state.hash="08".repeat(32);
    conflicting["birthday"]["priorTreeState"]=json!(hex::encode(state.encode_to_vec()));
    assert_eq!(call(g,"account_import",conflicting).unwrap_err(),"INCOHERENT_BIRTHDAY");
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(path).unwrap();
    let stored:String=conn.query_row("SELECT metadata FROM ext_viewing_accounts WHERE account_uuid=?1",[uuid::Uuid::parse_str(account["id"].as_str().unwrap()).unwrap()],|r|r.get(0)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(),original);
}
#[test]
fn transparent_exact_index_respects_native_recovery_gap() {
    let (_path,g)=open();let account=call(g,"account_import",fixture(16)).unwrap();
    let args=json!({"accountId":account["id"],"index":"1000","request":{"format":"unified","transparent":"require","sapling":"omit","ironwood":"require"}});
    assert_eq!(call(g,"address_at",args).unwrap_err(),"ADDRESS_GAP_LIMIT");
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn current_rechecks_requested_receivers_against_decoded_address() {
    let (path,g)=open();let account=call(g,"account_import",fixture(17)).unwrap();
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();conn.execute("UPDATE addresses SET receiver_flags=4 WHERE exposed_at_height IS NOT NULL",[]).unwrap();conn.close().unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g,"address_current",json!({"accountId":account["id"],"request":{"format":"unified","transparent":"omit","sapling":"require","ironwood":"omit"}})).unwrap_err(),"INVALID_STORED_ADDRESS");
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn transparent_receive_addresses_persist_without_a_unified_fallback() {
    let (path,g)=open();let account=call(g,"account_import",fixture(18)).unwrap();
    let args=json!({"accountId":account["id"],"request":{"format":"transparent"}});
    let first=call(g,"address_next",args.clone()).unwrap();
    assert_eq!(first["receiverTypes"],json!(["p2pkh"]));assert_eq!(first["intendedPools"],json!(["transparent"]));
    assert_eq!(call(g,"address_current",args.clone()).unwrap(),first["address"]);
    let mut at=args.clone();at["index"]=first["index"].clone();assert_eq!(call(g,"address_at",at).unwrap(),first);
    assert!(call(g,"address_list",json!({"accountId":account["id"]})).unwrap().as_array().unwrap().contains(&first));
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g,"address_current",args).unwrap(),first["address"]);
    crate::wallet::storage_close(g).unwrap();
}
