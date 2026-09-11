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
