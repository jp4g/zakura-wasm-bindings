use super::*;
fn propose(g:u32,op:&str,input:Value)->std::result::Result<Value,String>{
    crate::wallet::proposal::proposal_call(g,op,&input.to_string()).map(|s|serde_json::from_str(&s).unwrap())
}
#[test]
fn proposal_retains_native_selection_locks_and_reopens() {
    let fixture=full_scan_fixture();
    let (path,g)=open();
    let account=call(g,"account_import",fixture["import"].clone()).unwrap();
    let scan=|op:&str,v:Value|crate::wallet::scan::scan_call(g,op,&v.to_string()).map(|s|serde_json::from_str::<Value>(&s).unwrap()).unwrap();
    let mut revision=scan("scan_plan",json!({"target":fixture["target"]}))["revision"].clone();
    for data in fixture["batches"].as_array().unwrap(){let mut input=data.clone();input["target"]=fixture["target"].clone();input["revision"]=revision;revision=scan("scan_ingest_batch",input)["revision"].clone();}
    let address=call(g,"address_next",json!({"accountId":account["id"],"request":{"format":"transparent"}})).unwrap();
    let state=crate::wallet::sync::sync_call(g,"scan_state","{}").unwrap();
    revision=serde_json::from_str::<Value>(&state).unwrap()["revision"].clone();
    let mut input=json!({"revision":revision,"accountId":account["id"],"payments":[{"to":address["address"],"amount":"10000"}],
        "policy":{"spendPools":["sapling"],"transparent":"disallow","changePool":"sapling","feeRule":"zip317-standard","confirmations":{"trusted":1,"untrusted":1,"allowZeroConfirmationShielding":false},"expiry":{"kind":"offset","blocks":40},"lockExpiryBlocks":20}});
    let mut excessive=input.clone();excessive["payments"][0]["amount"]=json!("1000000");
    assert_eq!(propose(g,"proposal_create",excessive).unwrap_err(),"INSUFFICIENT_FUNDS");
    let mut capped=input.clone();capped["maxFee"]=json!("1");
    assert_eq!(propose(g,"proposal_create",capped).unwrap_err(),"FEE_LIMIT_EXCEEDED");
    let mut wrong_pool=input.clone();wrong_pool["policy"]["changePool"]=json!("ironwood");
    assert_eq!(propose(g,"proposal_create",wrong_pool).unwrap_err(),"UNSUPPORTED_POOL");
    let mut legacy=input.clone();legacy["policy"]["spendPools"]=json!(["orchard"]);
    assert_eq!(propose(g,"proposal_create",legacy).unwrap_err(),"INVALID_ARGUMENT");
    assert_eq!(propose(g,"proposal_list",json!({"afterSequence":"0","limit":200})).unwrap()["items"],json!([]));
    let plan=propose(g,"proposal_create",input.clone()).unwrap();
    assert_eq!(plan["steps"][0]["inputs"][0]["pool"],"sapling");
    assert_eq!(plan["steps"][0]["outputs"][0]["address"],address["address"]);
    assert_eq!(plan["steps"][0]["outputs"][1]["address"],Value::Null);
    assert_eq!(plan["targetHeight"],101);
    assert_eq!(plan["steps"][0]["transactionVersion"],6);
    assert_eq!(plan["expiryHeight"],141);
    input["revision"]=plan["revision"].clone();
    assert_eq!(propose(g,"proposal_create",input).unwrap_err(),"INSUFFICIENT_FUNDS");
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM ext_wallet_proposals",[],|r|r.get::<_,i64>(0)).unwrap(),1);drop(conn);
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    let discovered=propose(g,"proposal_list",json!({"afterSequence":"0","limit":1})).unwrap();
    assert_eq!(discovered["highWater"],"1");
    assert_eq!(propose(g,"proposal_get",json!({"operationId":discovered["items"][0]["operationId"]})).unwrap(),plan);
    assert_eq!(propose(g,"proposal_list",json!({"afterSequence":"1","highWater":"1","limit":1})).unwrap()["items"],json!([]));
    crate::wallet::storage_close(g).unwrap();
}

#[test]
fn proposal_schema_rejects_wrong_object_before_open() {
    let (path,g)=open();crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE ext_wallet_proposals;CREATE VIEW ext_wallet_proposals AS SELECT * FROM nonexistent").unwrap();drop(conn);
    assert_eq!(crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap_err(),"SCHEMA_MISMATCH");
}
