use super::*;
fn invoke(g:u32,operation:&str,input:Value)->std::result::Result<Value,String> {
    crate::wallet::pczt_build::pczt_build_call(g,operation,&input.to_string()).map(|v|serde_json::from_str(&v).unwrap())
}
const WORDS:&str="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
fn prepared(post_zip212:bool)->(String,u32,Value) {let (path,g,plan,_)=prepared_signer(post_zip212,false,false);(path,g,plan)}
fn prepared_signer(post_zip212:bool,with_signer:bool,internal:bool)->(String,u32,Value,Option<u32>) {
    let fixture=if !post_zip212 { full_scan_fixture() } else {
        let p=crate::Document::parse(PARAMS).unwrap();let mut import=fixture(10);
        let key=if with_signer {
            let seed=bip39::Mnemonic::parse(WORDS).unwrap().to_seed("");
            zcash_keys::keys::UnifiedSpendingKey::from_seed(&p,&seed,zip32::AccountId::ZERO).unwrap().to_unified_full_viewing_key()
        } else {UnifiedFullViewingKey::decode(&p,import["viewingKey"].as_str().unwrap()).unwrap()};
        import["viewingKey"]=json!(key.encode(&p));
        let state=TreeState{network:"regtest".into(),height:39999,hash:"07".repeat(32),time:1,sapling_tree:"000000".into(),orchard_tree:"000000".into(),ironwood_tree:"000000".into()};
        import["birthday"]["firstScanHeight"]=json!(40000);import["birthday"].as_object_mut().unwrap().remove("recoverUntilExclusive");
        import["birthday"]["priorTreeState"]=json!(hex::encode(state.encode_to_vec()));
        let mut block=policy_block(&key);block.height=40000;
        if internal {
            use zcash_note_encryption::Domain;
            let dfvk=key.sapling().unwrap();
            let note=sapling::Note::from_parts(dfvk.change_address().1,sapling::value::NoteValue::from_raw(50000),sapling::Rseed::AfterZip212([42;32]));
            let encryptor=sapling::note_encryption::sapling_note_encryption(Some(dfvk.to_internal_fvk().ovk),note.clone(),[0;512],&mut rand_core::UnwrapErr(getrandom::SysRng));
            block.vtx[0].outputs[0]=zcash_client_backend::proto::compact_formats::CompactSaplingOutput{cmu:note.cmu().to_bytes().to_vec(),ephemeral_key:sapling::note_encryption::SaplingDomain::epk_bytes(encryptor.epk()).0.to_vec(),ciphertext:encryptor.encrypt_note_plaintext()[..52].to_vec()};
        }
        json!({"import":import,"target":{"height":40000,"hash":"08".repeat(32)},"batches":[{"priorTreeState":hex::encode(state.encode_to_vec()),"blocks":[hex::encode(block.encode_to_vec())]}]})
    };
    let (path,g)=open();
    let (account,token)=if with_signer {
        let input=json!({"accountIndex":0,"birthday":fixture["import"]["birthday"]});
        let result:Value=serde_json::from_str(&crate::wallet::signer::signer_create_account(g,"account_import_hd",&input.to_string(),WORDS.as_bytes().to_vec(),vec![]).unwrap()).unwrap();
        (result["account"].clone(),Some(result["signerToken"].as_u64().unwrap() as u32))
    }else{(call(g,"account_import",fixture["import"].clone()).unwrap(),None)};
    let scan=|operation:&str,input:Value|crate::wallet::scan::scan_call(g,operation,&input.to_string()).map(|s|serde_json::from_str::<Value>(&s).unwrap()).unwrap();
    let mut revision=scan("scan_plan",json!({"target":fixture["target"]}))["revision"].clone();
    for data in fixture["batches"].as_array().unwrap(){let mut input=data.clone();input["target"]=fixture["target"].clone();input["revision"]=revision;revision=scan("scan_ingest_batch",input)["revision"].clone();}
    let t=call(g,"address_next",json!({"accountId":account["id"],"request":{"format":"transparent"}})).unwrap();
    let s=call(g,"address_next",json!({"accountId":account["id"],"request":{"format":"unified","transparent":"omit","sapling":"require","ironwood":"omit"}})).unwrap();
    let state:Value=serde_json::from_str(&crate::wallet::sync::sync_call(g,"scan_state","{}").unwrap()).unwrap();
    let input=json!({"revision":state["revision"],"accountId":account["id"],"payments":[{"to":t["address"],"amount":"10000"},{"to":s["address"],"amount":"10000","memo":hex::encode(zcash_protocol::memo::MemoBytes::empty().as_array())}],
        "policy":{"spendPools":["sapling"],"transparent":"disallow","changePool":"sapling","feeRule":"zip317-standard","confirmations":{"trusted":1,"untrusted":1,"allowZeroConfirmationShielding":false},"expiry":{"kind":"offset","blocks":40},"lockExpiryBlocks":20}});
    let plan:Value=serde_json::from_str(&crate::wallet::proposal::proposal_call(g,"proposal_create",&input.to_string()).unwrap()).unwrap();
    (path,g,plan,token)
}
fn request(plan:&Value)->Value {json!({"operationId":plan["operationId"],"proposalId":plan["proposalId"],"reviewCommitment":plan["reviewCommitment"]})}
#[test]
fn pczt_build_atomic_exact_outputs_and_retained_reopen() {
    let (path,g,plan)=prepared(true);let input=request(&plan);
    let mut forged=input.clone();forged["reviewCommitment"]=json!("00".repeat(32));
    assert_eq!(invoke(g,"pczt_build",forged).unwrap_err(),"PCZT_ASSOCIATION_MISMATCH");
    let conn=rusqlite::Connection::open(&path).unwrap();
    let before=policy_rows(&conn,&["addresses","ext_wallet_pczt","ext_wallet_revision"]);
    conn.execute_batch("CREATE TRIGGER build_fail BEFORE INSERT ON ext_wallet_pczt BEGIN SELECT RAISE(ABORT,'synthetic artifact failure'); END").unwrap();
    assert_eq!(invoke(g,"pczt_build",input.clone()).unwrap_err(),"STORAGE_ERROR");
    conn.execute_batch("DROP TRIGGER build_fail").unwrap();
    assert_eq!(policy_rows(&conn,&["addresses","ext_wallet_pczt","ext_wallet_revision"]),before);
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"]})).unwrap(),Value::Null);
    let artifact=invoke(g,"pczt_build",input.clone()).unwrap();
    assert_eq!(artifact["outputs"].as_array().unwrap().len(),3);
    assert_eq!(artifact["outputs"][0]["address"],plan["steps"][0]["outputs"][0]["address"]);
    assert_eq!(artifact["outputs"][1]["address"],plan["steps"][0]["outputs"][1]["address"]);
    assert_eq!(artifact["outputs"][1]["memo"],Value::Null,"explicit empty memo is native empty semantics");
    assert_eq!(artifact["outputs"][2]["kind"],"change");assert!(artifact["outputs"][2]["address"].as_str().is_some());
    // Pinned decrypt_diversifier misses internal scope; native change derivation proves ownership.
    let p=crate::Document::parse(PARAMS).unwrap();
    let key=UnifiedFullViewingKey::decode(&p,fixture(10)["viewingKey"].as_str().unwrap()).unwrap();
    let zcash_keys::address::Address::Sapling(change)=zcash_keys::address::Address::decode(&p,artifact["outputs"][2]["address"].as_str().unwrap()).unwrap() else {panic!("Sapling change")};
    assert_eq!(key.sapling().unwrap().diversified_change_address(*change.diversifier()),Some(change));
    assert_eq!(artifact["proofsComplete"],false);assert_eq!(artifact["authorizationComplete"],false);
    assert_eq!(invoke(g,"pczt_build",input.clone()).unwrap(),artifact,"idempotent build does not rebuild random effects");
    drop(conn);crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"]})).unwrap(),artifact);
    assert_eq!(invoke(g,"pczt_build",input).unwrap(),artifact,"retained bytes survive new owner revision");
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn pczt_build_rejects_stale_and_multistep_without_artifact() {
    let (path,g,plan)=prepared(true);
    call(g,"address_next",json!({"accountId":plan["accountId"],"request":{"format":"transparent"}})).unwrap();
    assert_eq!(invoke(g,"pczt_build",request(&plan)).unwrap_err(),"STALE_PROPOSAL");
    let conn=rusqlite::Connection::open(&path).unwrap();
    let encoded:Vec<u8>=conn.query_row("SELECT plan FROM ext_wallet_proposals",[],|r|r.get(0)).unwrap();
    let mut wire=zcash_client_backend::proto::proposal::Proposal::decode(&encoded[..]).unwrap();wire.steps.push(wire.steps[0].clone());
    let encoded=wire.encode_to_vec();conn.execute("UPDATE ext_wallet_proposals SET plan=?1",[&encoded]).unwrap();
    let policy:String=conn.query_row("SELECT policy FROM ext_wallet_proposals",[],|r|r.get(0)).unwrap();let policy:Value=serde_json::from_str(&policy).unwrap();
    let p=crate::Document::parse(PARAMS).unwrap();
    let review=crate::wallet::proposal::review(&wire,&p,plan["accountId"].as_str().unwrap(),plan["operationId"].as_str().unwrap(),&policy,plan["revision"].as_str().unwrap()).unwrap();
    let bound=crate::wallet::proposal::bind_review(review,&encoded,PARAMS,&[3;32],&policy);
    assert_eq!(invoke(g,"pczt_build",request(&bound)).unwrap_err(),"PCZT_MULTI_STEP_UNSUPPORTED");
    assert_eq!(conn.query_row("SELECT count(*) FROM ext_wallet_pczt",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    drop(conn);crate::wallet::storage_close(g).unwrap();
}

#[test]
fn pczt_build_preserves_native_zip212_output_precondition() {
    let (_,g,plan)=prepared(false);
    assert_eq!(invoke(g,"pczt_build",request(&plan)).unwrap_err(),"ROLE_PRECONDITION");
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"]})).unwrap(),Value::Null);
    crate::wallet::storage_close(g).unwrap();
}

#[test]
fn pczt_build_real_retained_signer_authorizes_after_wallet_close() {
    for internal in [false,true] {
    let (_path,g,plan,token)=prepared_signer(true,true,internal);let token=token.unwrap();
    let artifact=invoke(g,"pczt_build",request(&plan)).unwrap();
    let bytes=hex::decode(artifact["bytes"].as_str().unwrap()).unwrap();
    let original=pczt::Pczt::parse(&bytes).unwrap();
    pczt::roles::verifier::Verifier::new(original.clone()).with_sapling::<(),_>(|bundle|{assert!(bundle.spends().iter().any(|s|s.proof_generation_key().is_none()));Ok(())}).unwrap();
    let other:Value=serde_json::from_str(&crate::wallet::signer::signer_create_account(g,"account_import_hd",&json!({"accountIndex":0,"birthday":"fullScan"}).to_string(),WORDS.as_bytes().to_vec(),b"foreign".to_vec()).unwrap()).unwrap();
    let other=other["signerToken"].as_u64().unwrap() as u32;
    crate::wallet::storage_close(g).unwrap();
    assert_eq!(crate::wallet::signer::signer_authorize(other,PARAMS,&[3;32],40001,plan["branchId"].as_u64().unwrap() as u32,&bytes,4194304).unwrap_err(),"SIGNER_MISMATCH");
    crate::wallet::signer::signer_release(other).unwrap();
    let signed=crate::wallet::signer::signer_authorize(token,PARAMS,&[3;32],40001,plan["branchId"].as_u64().unwrap() as u32,&bytes,4194304).unwrap();
    let signed=pczt::Pczt::parse(&signed).unwrap();
    let mut role=pczt::roles::signer::Signer::new(signed.clone()).unwrap();
    pczt::roles::verifier::Verifier::new(signed).with_sapling::<(),_>(|bundle|{for (index,spend) in bundle.spends().iter().enumerate(){role.apply_sapling_signature(index,spend.spend_auth_sig().clone().unwrap()).unwrap();}Ok(())}).unwrap();
    assert_eq!(original.serialize().unwrap(),bytes,"authorization does not mutate retained artifact");
    crate::wallet::signer::signer_release(token).unwrap();
    }
}
