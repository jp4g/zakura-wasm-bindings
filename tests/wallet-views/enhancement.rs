use super::*;
fn enhance(g:u32,op:&str,input:Value)->std::result::Result<Value,String>{crate::wallet::enhancement::enhancement_call(g,op,&input.to_string()).map(|s|serde_json::from_str(&s).unwrap())}
#[test]
fn enhancement_full_transaction_is_atomic_and_persists_after_reopen() {
    use zcash_client_backend::data_api::WalletRead;
    let (path,g)=open();call(g,"account_import",fixture(10)).unwrap();
    let p=crate::Document::parse(PARAMS).unwrap();
    let key=UnifiedFullViewingKey::decode(&p,fixture(10)["viewingKey"].as_str().unwrap()).unwrap();
    let address=*key.default_address(UnifiedAddressRequest::AllAvailableKeys).unwrap().0.transparent().unwrap();
    let coinbase=zcash_transparent::bundle::TxIn::coinbase(100.into(),None).unwrap();
    let tx=zcash_primitives::transaction::TransactionData::<zcash_primitives::transaction::Authorized>::from_parts(
        zcash_primitives::transaction::TxVersion::V5,zcash_protocol::consensus::BranchId::for_height(&p,100.into()),0,120.into(),
        Some(zcash_transparent::bundle::Bundle{vin:vec![zcash_transparent::bundle::TxIn::from_parts(coinbase.prevout().clone(),zcash_transparent::address::Script::read(&[2u8,1,100][..]).unwrap(),coinbase.sequence())],vout:vec![zcash_transparent::bundle::TxOut::new(zcash_protocol::value::Zatoshis::const_from_u64(1000),address.script().into())],authorization:zcash_transparent::bundle::Authorized}),None,None,None).freeze().unwrap();
    let id=tx.txid();let mut raw=Vec::new();tx.write(&mut raw).unwrap();
    // Seed a closed synthetic fixture database with upstream retrieval intent.
    // The extension transaction authorizer remains unchanged in production.
    use zcash_client_backend::data_api::WalletWrite;
    crate::wallet::DOMAIN.with(|d|d.borrow_mut().active.as_mut().unwrap().wallet.update_chain_tip(100.into())).unwrap();
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();
    conn.execute("INSERT INTO tx_retrieval_queue(txid,query_type) VALUES(?1,1)",[id.as_ref()]).unwrap();drop(conn);
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    let requests=enhance(g,"enhancement_requests",json!({})).unwrap();
    let request=requests["requests"].as_array().unwrap().iter().find(|r|r["txid"]==id.to_string()).unwrap().clone();
    let apply=json!({"revision":requests["revision"],"request":request,"result":{"transactions":[{"bytes":hex::encode(&raw),"minedHeight":100}]}});
    let mut bad=apply.clone();bad["result"]["transactions"][0]["bytes"]=json!("00");
    assert_eq!(enhance(g,"enhancement_apply",bad).unwrap_err(),"INVALID_ARGUMENT");
    assert_eq!(enhance(g,"enhancement_requests",json!({})).unwrap(),requests);
    let applied=enhance(g,"enhancement_apply",apply.clone()).unwrap();assert_ne!(applied["revision"],requests["revision"]);
    assert_eq!(enhance(g,"enhancement_apply",apply).unwrap_err(),"STALE_REVISION");
    let remaining=enhance(g,"enhancement_requests",json!({})).unwrap();
    assert!(!remaining["requests"].as_array().unwrap().iter().any(|r|r["kind"]=="enhancement"&&r["txid"]==id.to_string()));
    crate::wallet::DOMAIN.with(|d|assert!(d.borrow().active.as_ref().unwrap().wallet.get_transaction(id).unwrap().is_some()));
    crate::wallet::scan::scan_call(g,"scan_plan",&json!({"target":{"height":101,"hash":"04".repeat(32)}}).to_string()).unwrap();
    let pending=enhance(g,"enhancement_requests",json!({})).unwrap();
    let address_request=pending["requests"].as_array().unwrap().iter().find(|r|r["kind"]=="address"&&r["txStatus"]=="mined"&&r["outputStatus"]=="all").expect("real transparent spend-search request").clone();
    let as_of=address_request["endExclusive"].as_u64().unwrap()-1;
    let partial=enhance(g,"enhancement_apply",json!({"revision":pending["revision"],"request":address_request,"result":{"transactions":[],"asOfHeight":as_of,"complete":false}})).unwrap();
    assert_ne!(partial["revision"],pending["revision"]);
    assert!(enhance(g,"enhancement_requests",json!({})).unwrap()["requests"].as_array().unwrap().contains(&address_request));
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    crate::wallet::DOMAIN.with(|d|assert!(d.borrow().active.as_ref().unwrap().wallet.get_transaction(id).unwrap().is_some()));
    let reopened=enhance(g,"enhancement_requests",json!({})).unwrap();
    assert_ne!(reopened["revision"],applied["revision"]);
    assert!(reopened["requests"].as_array().unwrap().contains(&address_request));
    enhance(g,"enhancement_apply",json!({"revision":reopened["revision"],"request":address_request,"result":{"transactions":[],"asOfHeight":as_of,"complete":true}})).unwrap();
    assert!(!enhance(g,"enhancement_requests",json!({})).unwrap()["requests"].as_array().unwrap().contains(&address_request));
    crate::wallet::storage_close(g).unwrap();
    // A known unmined transaction produces the upstream status polling request.
    let conn=rusqlite::Connection::open(&path).unwrap();
    conn.execute("UPDATE transactions SET mined_height=NULL,confirmed_unmined_at_height=NULL WHERE txid=?1",[id.as_ref()]).unwrap();
    conn.execute("INSERT OR IGNORE INTO tx_retrieval_queue(txid,query_type) VALUES(?1,0)",[id.as_ref()]).unwrap();drop(conn);
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    let pending=enhance(g,"enhancement_requests",json!({})).unwrap();
    let status=pending["requests"].as_array().unwrap().iter().find(|r|r["kind"]=="status"&&r["txid"]==id.to_string()).unwrap();
    let invalid=json!({"revision":pending["revision"],"request":status,"result":{"status":"mined","height":102}});
    assert_eq!(enhance(g,"enhancement_apply",invalid).unwrap_err(),"CHAIN_MISMATCH");
    assert_eq!(enhance(g,"enhancement_requests",json!({})).unwrap(),pending);
    let applied=enhance(g,"enhancement_apply",json!({"revision":pending["revision"],"request":status,"result":{"status":"notInMainChain"}})).unwrap();
    assert_ne!(applied["revision"],pending["revision"]);
    crate::wallet::storage_close(g).unwrap();
}
