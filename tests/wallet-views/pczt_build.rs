use super::*;
fn invoke(g:u32,operation:&str,input:Value)->std::result::Result<Value,String> {
    crate::wallet::pczt_build::pczt_build_call(g,operation,&input.to_string()).map(|v|serde_json::from_str(&v).unwrap())
}
const WORDS:&str="abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
fn prepared(post_zip212:bool)->(String,u32,Value) {let (path,g,plan,_)=prepared_signer(post_zip212,false,false);(path,g,plan)}
fn scan_fixture(with_signer:bool,internal:bool)->Value {
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
}
pub(super) fn pczt_fixture()->Value {
    json!({"mnemonic":WORDS,"accountIndex":0,"external":scan_fixture(true,false),"internal":scan_fixture(true,true)})
}
fn prepared_signer(post_zip212:bool,with_signer:bool,internal:bool)->(String,u32,Value,Option<u32>) {prepared_proving(post_zip212,with_signer,internal,false)}
fn prepared_proving(post_zip212:bool,with_signer:bool,internal:bool,ironwood:bool)->(String,u32,Value,Option<u32>) {
    let fixture=if post_zip212{scan_fixture(with_signer,internal)}else{full_scan_fixture()};
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
    let s=call(g,"address_next",json!({"accountId":account["id"],"request":{"format":"unified","transparent":"omit","sapling":if ironwood{"omit"}else{"require"},"ironwood":if ironwood{"require"}else{"omit"}}})).unwrap();
    let state:Value=serde_json::from_str(&crate::wallet::sync::sync_call(g,"scan_state","{}").unwrap()).unwrap();
    let input=json!({"revision":state["revision"],"accountId":account["id"],"payments":[{"to":t["address"],"amount":"10000"},{"to":s["address"],"amount":"10000","memo":hex::encode(zcash_protocol::memo::MemoBytes::empty().as_array())}],
        "policy":{"spendPools":["sapling"],"transparent":"disallow","changePool":if ironwood{"ironwood"}else{"sapling"},"feeRule":"zip317-standard","confirmations":{"trusted":1,"untrusted":1,"allowZeroConfirmationShielding":false},"expiry":{"kind":"offset","blocks":40},"lockExpiryBlocks":20}});
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

#[test]
fn pczt_import_retains_versions_and_rolls_back_invalid_returns() {
    let(path,g,plan,token)=prepared_signer(true,true,false);let token=token.unwrap();
    let original=invoke(g,"pczt_build",request(&plan)).unwrap();
    let bytes=hex::decode(original["bytes"].as_str().unwrap()).unwrap();
    let signed=crate::wallet::signer::signer_authorize(token,PARAMS,&[3;32],40001,plan["branchId"].as_u64().unwrap() as u32,&bytes,4194304).unwrap();
    let import=|g,bytes:&[u8]|crate::wallet::pczt_build::pczt_import_call(g,plan["operationId"].as_str().unwrap(),bytes,4194304).map(|s|serde_json::from_str::<Value>(&s).unwrap());
    let conn=rusqlite::Connection::open(&path).unwrap();
    let before=policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]);
    let mut changed=serde_json::to_value(pczt::v2::Pczt::try_from(pczt::Pczt::parse(&signed).unwrap()).unwrap()).unwrap();
    changed["transparent"]["outputs"][0]["value"]=json!(1);
    let changed=serde_json::from_value::<pczt::v2::Pczt>(changed).unwrap().serialize();
    assert_eq!(import(g,&changed).unwrap_err(),"PCZT_ASSOCIATION_MISMATCH");
    let mut signature=vec![];
    pczt::roles::verifier::Verifier::new(pczt::Pczt::parse(&signed).unwrap()).with_sapling::<(),_>(|bundle|{let bytes:[u8;64]=bundle.spends()[0].spend_auth_sig().clone().unwrap().into();signature=bytes.to_vec();Ok(())}).unwrap();
    let mut invalid_signed=signed.clone();let at=invalid_signed.windows(64).position(|part|part==signature).unwrap();invalid_signed[at+63]^=1;
    assert_eq!(import(g,&invalid_signed).unwrap_err(),"INVALID_PCZT");
    assert_eq!(crate::wallet::pczt_build::pczt_import_call(g,&"00".repeat(32),&signed,4194304).unwrap_err(),"OPERATION_NOT_FOUND");
    conn.execute_batch("CREATE TRIGGER import_fail BEFORE INSERT ON ext_wallet_pczt_artifacts BEGIN SELECT RAISE(ABORT,'synthetic import failure'); END").unwrap();
    assert_eq!(import(g,&signed).unwrap_err(),"STORAGE_ERROR");
    conn.execute_batch("DROP TRIGGER import_fail").unwrap();
    assert_eq!(policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]),before);
    let reduced=crate::standalone_pczt::parse_standalone_pczt(PARAMS,&[3;32],40001,plan["branchId"].as_u64().unwrap() as u32,&signed,4194304).unwrap().redact("zakura-signer-full/1").unwrap().serialize().unwrap();
    assert!(reduced.len()<signed.len());
    assert_eq!(crate::wallet::pczt_build::pczt_import_call(g,plan["operationId"].as_str().unwrap(),&reduced,reduced.len() as u32).unwrap_err(),"RESOURCE_LIMIT");
    assert_eq!(policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]),before);
    let accepted=import(g,&signed).unwrap();assert_eq!(accepted["authorizationComplete"],true);assert_ne!(accepted["artifactId"],original["artifactId"]);
    let after=policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]);
    assert_eq!(import(g,&signed).unwrap(),accepted);assert_eq!(policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]),after);
    crate::wallet::signer::signer_release(token).unwrap();drop(conn);crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"]})).unwrap(),accepted);
    for artifact in [original,accepted]{assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"],"artifactId":artifact["artifactId"]})).unwrap(),artifact);}
    crate::wallet::storage_close(g).unwrap();
}

#[test]
#[ignore = "requires locally qualified canonical Sapling parameters and real proving"]
fn pczt_prove_real_sapling_retains_and_verifies() { prove_retained(false,false); }
#[test]
#[ignore = "requires locally qualified parameters and real Sapling/Ironwood proving"]
fn pczt_prove_real_ironwood_retains_and_verifies() { prove_retained(true,false); }
#[test]
#[ignore = "requires canonical assets and real finalize proof fixture"]
fn pczt_finalize_real_sapling_atomic() { prove_retained(false,true); }
#[test]
#[ignore = "requires canonical assets and real finalize proof fixture"]
fn pczt_finalize_real_ironwood_atomic() { prove_retained(true,true); }
fn prove_retained(ironwood:bool,finalize:bool) {
    let root=std::env::var("PCZT_PROVING_PARAMETERS").unwrap();
    let spend=std::fs::read(format!("{root}/sapling-spend.params")).unwrap();
    let output=std::fs::read(format!("{root}/sapling-output.params")).unwrap();
    let(path,g,plan,token)=prepared_proving(true,true,false,ironwood);let token=token.unwrap();
    let original=invoke(g,"pczt_build",request(&plan)).unwrap();
    let bytes=hex::decode(original["bytes"].as_str().unwrap()).unwrap();
    let signed=crate::wallet::signer::signer_authorize(token,PARAMS,&[3;32],40001,plan["branchId"].as_u64().unwrap() as u32,&bytes,4194304).unwrap();
    let retained:Value=serde_json::from_str(&crate::wallet::pczt_build::pczt_import_call(g,plan["operationId"].as_str().unwrap(),&signed,4194304).unwrap()).unwrap();
    let prove=|spend:&[u8],output:&[u8]|crate::wallet::pczt_prove::pczt_prove_call(g,plan["operationId"].as_str().unwrap(),retained["artifactId"].as_str().unwrap(),spend,output,4194304);
    let conn=rusqlite::Connection::open(&path).unwrap();let before=policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]);
    assert_eq!(prove(&spend[..spend.len()-1],&output).unwrap_err(),"ASSET_INTEGRITY");
    assert_eq!(prove(&[],&output).unwrap_err(),"PROVING_MATERIAL_REQUIRED");
    let mut corrupt=output.clone();corrupt[0]^=1;assert_eq!(prove(&spend,&corrupt).unwrap_err(),"ASSET_INTEGRITY");
    assert_eq!(retained["requiresSaplingProofs"],true);assert_eq!(retained["requiresIronwoodProof"],ironwood);
    assert_eq!(policy_rows(&conn,&["ext_wallet_pczt","ext_wallet_pczt_artifacts","ext_wallet_revision"]),before);
    let proven:Value=serde_json::from_str(&prove(&spend,&output).unwrap()).unwrap();
    assert_eq!(proven["proofsComplete"],true);assert_eq!(proven["requiresSaplingProofs"],false);assert_eq!(proven["requiresIronwoodProof"],false);assert_ne!(proven["artifactId"],retained["artifactId"]);
    let value=pczt::Pczt::parse(&hex::decode(proven["bytes"].as_str().unwrap()).unwrap()).unwrap();
    assert!(!value.sapling().spends().is_empty());assert!(!value.sapling().outputs().is_empty());
    assert_eq!(!value.ironwood().actions().is_empty(),ironwood);
    let spend_vk=sapling::circuit::SpendParameters::read(&spend[..],false).unwrap().verifying_key();
    let output_vk=sapling::circuit::OutputParameters::read(&output[..],false).unwrap().verifying_key();
    pczt::roles::tx_extractor::TransactionExtractor::new(value).with_sapling(&spend_vk,&output_vk).extract().unwrap();
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"],"artifactId":original["artifactId"]})).unwrap(),original);
    let finalized=if finalize {
        let call=|spend:&[u8],output:&[u8]|crate::wallet::pczt_finalize::pczt_finalize_call(g,plan["operationId"].as_str().unwrap(),proven["artifactId"].as_str().unwrap(),spend,output,4194304);
        let tables=["transactions","sent_notes","sapling_received_note_spends","orchard_received_note_spends","ironwood_received_note_spends","ext_wallet_finalized","ext_wallet_revision"];
        let before=policy_rows(&conn,&tables);
        conn.execute_batch("CREATE TRIGGER finalize_fail BEFORE INSERT ON ext_wallet_finalized BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
        assert!(call(&spend,&output).is_err());conn.execute_batch("DROP TRIGGER finalize_fail").unwrap();
        assert_eq!(policy_rows(&conn,&tables),before,"backend storage and outbox insertion roll back together");
        assert_eq!(crate::wallet::pczt_finalize::finalized_get(g,plan["operationId"].as_str().unwrap()).unwrap(),"null");
        let finalized=call(&spend,&output).unwrap();
        assert_eq!(call(&[],&[]).unwrap(),finalized,"already committed exact bytes need no extraction or assets");
        let txid:Vec<u8>=conn.query_row("SELECT txid FROM ext_wallet_finalized",[],|r|r.get(0)).unwrap();
        conn.execute("UPDATE ext_wallet_finalized SET txid=zeroblob(32)",[]).unwrap();
        assert_eq!(crate::wallet::pczt_finalize::finalized_get(g,plan["operationId"].as_str().unwrap()).unwrap_err(),"STORAGE_ERROR");
        conn.execute("UPDATE ext_wallet_finalized SET txid=?1",[txid]).unwrap();
        Some(finalized)
    }else{None};
    drop(conn);crate::wallet::storage_close(g).unwrap();let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(invoke(g,"pczt_get_artifact",json!({"operationId":plan["operationId"]})).unwrap(),proven);
    if let Some(finalized)=finalized {
        let mut old:Value=serde_json::from_str(&finalized).unwrap();let mut reopened:Value=serde_json::from_str(&crate::wallet::pczt_finalize::finalized_get(g,plan["operationId"].as_str().unwrap()).unwrap()).unwrap();
        let current:Value=serde_json::from_str(&crate::wallet::sync::sync_call(g,"scan_state","{}").unwrap()).unwrap();assert_eq!(reopened["revision"],current["revision"]);
        old.as_object_mut().unwrap().remove("revision");reopened.as_object_mut().unwrap().remove("revision");assert_eq!(reopened,old);
    }
    crate::wallet::storage_close(g).unwrap();crate::wallet::signer::signer_release(token).unwrap();
}
