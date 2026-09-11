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
