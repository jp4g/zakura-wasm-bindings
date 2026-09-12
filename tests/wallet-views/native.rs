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
    let account = call(g,"account_import",fixture(10)).unwrap();
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
    let input=fixture(10);
    let key=UnifiedFullViewingKey::decode(&p,input["viewingKey"].as_str().unwrap()).unwrap();
    let (ua,j)=key.default_address(UnifiedAddressRequest::AllAvailableKeys).unwrap();
    let mut explicit_false=fixture(12);explicit_false["viewOnly"]=json!(false);
    let mut explicit_true=fixture(14);explicit_true["viewOnly"]=json!(true);
    let output=json!({"warning":"SYNTHETIC TEST AUTHORITY ONLY. NEVER USE FOR PRODUCTION FUNDS.","import":input,
        "policyImports":[explicit_false,explicit_true],
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
#[test]
fn extension_admission_rejects_oversize_metadata_and_changed_schema() {
    let (path,g)=open();crate::wallet::storage_close(g).unwrap();
    let mut conn=rusqlite::Connection::open(path).unwrap();
    conn.execute("INSERT INTO ext_viewing_accounts VALUES(?1,?2)",rusqlite::params![uuid::Uuid::new_v4(),"x".repeat(160001)]).unwrap();
    assert_eq!(super::initialize(&mut conn).unwrap_err(),"VIEWING_SCHEMA_REQUIRED");
    conn.execute("DELETE FROM ext_viewing_accounts",[]).unwrap();
    conn.execute_batch("DROP TABLE ext_viewing_accounts; CREATE TABLE ext_viewing_accounts(account_uuid BLOB,metadata TEXT);").unwrap();
    assert_eq!(super::initialize(&mut conn).unwrap_err(),"VIEWING_SCHEMA_REQUIRED");
}
#[test]
fn projected_import_retains_original_key_and_nonempty_distinct_pool_trees() {
    use zcash_primitives::merkle_tree::{HashSer,write_commitment_tree};
    let (path,g)=open();let mut input=fixture(19);
    let raw=hex::decode(input["birthday"]["priorTreeState"].as_str().unwrap()).unwrap();
    let mut state=TreeState::decode(raw.as_slice()).unwrap();state.height=100;
    let mut sapling=state.sapling_tree().unwrap();let node=HashSer::read(&[0u8;32][..]).unwrap();sapling.append(node).unwrap();
    let mut orchard=state.orchard_tree().unwrap();let node=HashSer::read(&[0u8;32][..]).unwrap();orchard.append(node).unwrap();
    let mut ironwood=state.ironwood_tree().unwrap();
    for _ in 0..2 {let node=HashSer::read(&[0u8;32][..]).unwrap();ironwood.append(node).unwrap();}
    let mut buf=Vec::new();write_commitment_tree(&sapling,&mut buf).unwrap();state.sapling_tree=hex::encode(&buf);
    buf.clear();write_commitment_tree(&orchard,&mut buf).unwrap();state.orchard_tree=hex::encode(&buf);
    buf.clear();write_commitment_tree(&ironwood,&mut buf).unwrap();state.ironwood_tree=hex::encode(&buf);
    input["birthday"]["firstScanHeight"]=json!(101);input["birthday"]["priorTreeState"]=json!(hex::encode(state.encode_to_vec()));
    input["enabledPools"]=json!(["sapling"]);input["viewOnly"]=json!(true);input["name"]=json!("");
    let account=call(g,"account_import",input.clone()).unwrap();assert_eq!(account["name"],"");
    assert_eq!(call(g,"address_list",json!({"accountId":account["id"]})).unwrap()[0]["receiverTypes"],json!(["sapling"]));
    crate::wallet::storage_close(g).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();
    let stored:String=conn.query_row("SELECT metadata FROM ext_viewing_accounts",[],|r|r.get(0)).unwrap();assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(),input);
    let spend:u32=conn.query_row("SELECT has_spend_key FROM accounts",[],|r|r.get(0)).unwrap();assert_eq!(spend,0);
    conn.close().unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g,"account_get",json!({"accountId":account["id"]})).unwrap(),account);
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn native_default_address_cannot_exceed_recovery_gap_during_import() {
    let (_path,g)=open();
    assert_eq!(call(g,"account_import",fixture(7)).unwrap_err(),"ADDRESS_GAP_LIMIT");
    assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([]));
    crate::wallet::storage_close(g).unwrap();
}

// Real encrypted compact outputs, decrypted by the pinned scanner and persisted by
// WalletWrite::put_blocks. Values/keys are synthetic; no provider or funds involved.
fn policy_block(key: &UnifiedFullViewingKey) -> zcash_client_backend::proto::compact_formats::CompactBlock {
    use zcash_client_backend::proto::compact_formats::*;
    use zcash_note_encryption::Domain;
    use orchard::note::{NoteVersion, RandomSeed, Rho, ExtractedNoteCommitment};
    use orchard::note_encryption::{IronwoodDomain, IronwoodNoteEncryption, OrchardDomain, OrchardNoteEncryption};
    let mut rng = rand_core::UnwrapErr(getrandom::SysRng);
    let dfvk = key.sapling().unwrap();
    let note = sapling::Note::from_parts(dfvk.default_address().1,
        sapling::value::NoteValue::from_raw(50_000), sapling::Rseed::AfterZip212([42;32]));
    let encryptor = sapling::note_encryption::sapling_note_encryption(Some(dfvk.fvk().ovk),note.clone(),[0;512],&mut rng);
    let output = CompactSaplingOutput { cmu:note.cmu().to_bytes().to_vec(),
        ephemeral_key:sapling::note_encryption::SaplingDomain::epk_bytes(encryptor.epk()).0.to_vec(),
        ciphertext:encryptor.encrypt_note_plaintext()[..52].to_vec() };
    let fvk = key.orchard().unwrap();
    let action = |ironwood| {
        let rho = Rho::from_bytes(&[0;32]).unwrap();
        let rseed = RandomSeed::from_bytes([43;32],&rho).unwrap();
        let note = orchard::Note::from_parts(fvk.address_at(0u32,zip32::Scope::External),
            orchard::value::NoteValue::from_raw(60_000),rho,rseed,
            if ironwood {NoteVersion::V3} else {NoteVersion::V2}).unwrap();
        let (epk,ciphertext) = if ironwood {
            let e=IronwoodNoteEncryption::new(Some(fvk.to_ovk(zip32::Scope::External)),note,[0;512]);
            (IronwoodDomain::epk_bytes(e.epk()).0,e.encrypt_note_plaintext())
        } else {
            let e=OrchardNoteEncryption::new(Some(fvk.to_ovk(zip32::Scope::External)),note,[0;512]);
            (OrchardDomain::epk_bytes(e.epk()).0,e.encrypt_note_plaintext())
        };
        CompactOrchardAction { nullifier:vec![0;32],cmx:ExtractedNoteCommitment::from(note.commitment()).to_bytes().to_vec(),ephemeral_key:epk.to_vec(),ciphertext:ciphertext[..52].to_vec() }
    };
    CompactBlock { height:100,hash:vec![8;32],prev_hash:vec![7;32],time:2,
        vtx:vec![CompactTx { txid:vec![9;32],index:1,outputs:vec![output],actions:vec![action(false)],ironwood_actions:vec![action(true)],..Default::default() }],
        chain_metadata:Some(ChainMetadata {sapling_commitment_tree_size:1,orchard_commitment_tree_size:1,ironwood_commitment_tree_size:1}),..Default::default() }
}

#[test]
fn native_policy_real_scan_retention_selection_and_reopen() { policy_scan_matrix(false); }

#[test]
fn native_policy_legacy_marks_migrate_on_open() { policy_scan_matrix(true); }

// Read native rows verbatim for preservation/rollback assertions; never synthesize
// serialized tree data. ORDER BY rowid makes repeat-open comparisons deterministic.
fn policy_rows(conn: &rusqlite::Connection, tables: &[&str]) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    tables.iter().map(|table| {
        let mut stmt=conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid")).unwrap();
        let columns=stmt.column_count();
        stmt.query_map([],|row| (0..columns).map(|i|row.get(i)).collect()).unwrap().collect::<std::result::Result<Vec<_>,_>>().unwrap()
    }).collect()
}

fn policy_scan_matrix(legacy: bool) {
    use zcash_client_backend::{scanning::{scan_block,ScanningKeys,Nullifiers},data_api::{InputSource,WalletCommitmentTrees,BlockMetadata,locking::LockFilter}};
    use zcash_primitives::{block::BlockHash,transaction::TxId};
    use zcash_protocol::ShieldedPool;
    for policy in [None,Some(false),Some(true)] {
        let (path,g)=open();
        let mut input=fixture(10);
        input["birthday"].as_object_mut().unwrap().remove("recoverUntilExclusive");
        if let Some(value)=policy {input["viewOnly"]=json!(value);}
        let account=call(g,"account_import",input.clone()).unwrap();
        let account_id=id(&json!({"accountId":account["id"]})).unwrap();
        let p=crate::Document::parse(PARAMS).unwrap();
        let key=UnifiedFullViewingKey::decode(&p,input["viewingKey"].as_str().unwrap()).unwrap();
        let birthday=birthday(&input["birthday"],PARAMS,&[3;32],&p).unwrap();
        let mut other_input=fixture(12);
        other_input["birthday"].as_object_mut().unwrap().remove("recoverUntilExclusive");
        let other=call(g,"account_import",other_input.clone()).unwrap();
        let other_id=id(&json!({"accountId":other["id"]})).unwrap();
        let other_key=UnifiedFullViewingKey::decode(&p,other_input["viewingKey"].as_str().unwrap()).unwrap();
        let mut cb=policy_block(&key);
        let mut other_cb=policy_block(&other_key);
        other_cb.vtx[0].txid=vec![11;32]; other_cb.vtx[0].index=2;
        cb.vtx.extend(other_cb.vtx);
        let metadata=cb.chain_metadata.as_mut().unwrap();
        metadata.sapling_commitment_tree_size=2; metadata.orchard_commitment_tree_size=2; metadata.ironwood_commitment_tree_size=2;
        let keys=ScanningKeys::from_account_ufvks([(account_id,key),(other_id,other_key)]);
        let prior=BlockMetadata::from_parts(99u32.into(),BlockHash([7;32]),Some(0),Some(0),Some(0));
        let scan=scan_block(&p,cb,&keys,&Nullifiers::empty(),Some(&prior)).unwrap();
        assert_eq!(scan.transactions()[0].sapling_outputs().len(),1);
        assert_eq!(scan.transactions()[0].orchard_outputs().len(),1);
        assert_eq!(scan.transactions()[0].ironwood_outputs().len(),1);
        let sapling_commitments=scan.sapling().commitments().to_vec();
        let orchard_commitments=scan.orchard().commitments().to_vec();
        let ironwood_commitments=scan.ironwood().commitments().to_vec();
        crate::wallet::DOMAIN.with(|domain| {
            let mut domain=domain.borrow_mut();let db=&mut domain.active.as_mut().unwrap().wallet;
            db.update_chain_tip(100u32.into()).unwrap();
            db.put_blocks(birthday.prior_chain_state(),vec![scan]).unwrap();
            let address = *db.get_last_generated_address_matching(account_id,UnifiedAddressRequest::AllAvailableKeys).unwrap().unwrap().transparent().unwrap();
            let output = zcash_client_backend::wallet::WalletTransparentOutput::from_parts(
                zcash_transparent::bundle::OutPoint::new([10;32],0),
                zcash_transparent::bundle::TxOut::new(zcash_protocol::value::Zatoshis::const_from_u64(70_000),address.script().into()),
                Some(100u32.into()),Some(account_id),Some(TransparentKeyScope::EXTERNAL),None).unwrap();
            db.put_received_transparent_utxo(&output).unwrap();
            if legacy {
                // Genuine scanner commitments and original native retention flags, persisted
                // through native insertion APIs, reproduce pre-policy per-note marks.
                db.with_sapling_tree_mut(|tree| tree.batch_insert(0u64.into(),sapling_commitments.clone().into_iter())).unwrap();
                db.with_orchard_tree_mut(|tree| tree.batch_insert(0u64.into(),orchard_commitments.clone().into_iter())).unwrap();
                db.with_ironwood_tree_mut(|tree| tree.batch_insert(0u64.into(),ironwood_commitments.clone().into_iter())).unwrap();
                // Deferred native removals retain marks until checkpoint pruning. The
                // policy migration must clear only the view-only removal records too.
                for position in [0u64,1] {
                    db.with_sapling_tree_mut(|tree| tree.remove_mark(position.into(),Some(&100u32.into()))).unwrap();
                    db.with_orchard_tree_mut(|tree| tree.remove_mark(position.into(),Some(&100u32.into()))).unwrap();
                    db.with_ironwood_tree_mut(|tree| tree.remove_mark(position.into(),Some(&100u32.into()))).unwrap();
                }
                assert_eq!(db.with_sapling_tree_mut(|tree| tree.marked_positions()).unwrap().len(),2);
                assert_eq!(db.with_orchard_tree_mut(|tree| tree.marked_positions()).unwrap().len(),2);
                assert_eq!(db.with_ironwood_tree_mut(|tree| tree.marked_positions()).unwrap().unwrap().len(),2);
            }
        });
        crate::wallet::storage_close(g).unwrap();
        let mut conn=rusqlite::Connection::open(&path).unwrap();
        if legacy { conn.execute("UPDATE ext_viewing_version SET version=1",[]).unwrap(); }
        for pool in ["sapling","orchard","ironwood"] {
            let count:u32=conn.query_row(&format!("SELECT count(*) FROM {pool}_received_notes WHERE nf IS NOT NULL AND value>0 AND commitment_tree_position=0"),[],|r|r.get(0)).unwrap();
            assert_eq!(count,1,"viewing history and spentness retained: {pool}");
        }
        assert_eq!(conn.query_row("SELECT ufvk IS NOT NULL FROM accounts",[],|r|r.get::<_,bool>(0)).unwrap(),true);
        let preserved_tables=["accounts","ext_viewing_accounts","ext_viewing_addresses","transactions","sapling_received_notes","orchard_received_notes","ironwood_received_notes","transparent_received_outputs","sapling_tree_checkpoints","orchard_tree_checkpoints","ironwood_tree_checkpoints"];
        let preserved=policy_rows(&conn,&preserved_tables);
        let tree_tables=["sapling_tree_shards","orchard_tree_shards","ironwood_tree_shards","sapling_tree_cap","orchard_tree_cap","ironwood_tree_cap","sapling_tree_checkpoint_marks_removed","orchard_tree_checkpoint_marks_removed","ironwood_tree_checkpoint_marks_removed","ext_viewing_version"];
        let initial_trees=policy_rows(&conn,&tree_tables);
        if legacy && policy==Some(true) {
            let before=initial_trees.clone();
            let reader=rusqlite::Connection::open(&path).unwrap();
            reader.execute_batch("BEGIN; SELECT * FROM accounts;").unwrap();
            conn.busy_timeout(std::time::Duration::ZERO).unwrap();
            let changes=conn.total_changes();
            assert_eq!(super::initialize(&mut conn).unwrap_err(),"VIEWING_SCHEMA_REQUIRED");
            assert!(conn.total_changes()>changes,"native migration wrote before blocked COMMIT");
            assert_eq!(policy_rows(&conn,&tree_tables),before,"all pool writes and version roll back");
            assert_eq!(policy_rows(&conn,&preserved_tables),preserved);
            eprintln!("native migration COMMIT blocked: all tree/version writes rolled back");
            assert!(crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).is_err(),"locked legacy opening must not publish an owner");
            assert!(call(g,"account_list",json!({})).is_err());
            assert_eq!(policy_rows(&conn,&tree_tables),before,"failed open cannot expose a partial migration");
            reader.execute_batch("ROLLBACK").unwrap();
        }
        conn.close().unwrap();
        let mut migrated=None;
        for reopen in 0..2 {
        let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
        assert_eq!(call(g,"account_get",json!({"accountId":account["id"]})).unwrap(),account);
        assert_eq!(call(g,"account_get",json!({"accountId":other["id"]})).unwrap(),other);
        crate::wallet::DOMAIN.with(|domain| {
            let mut domain=domain.borrow_mut();let db=&mut domain.active.as_mut().unwrap().wallet;
            let expected=usize::from(policy!=Some(true));
            let summary=db.get_wallet_summary(zcash_client_backend::data_api::wallet::ConfirmationsPolicy::MIN).unwrap().unwrap();
            let balance=&summary.account_balances()[&account_id];
            eprintln!("legacy={legacy} policy={policy:?} native summary spendable: {:?}", [balance.sapling_balance().spendable_value(),balance.orchard_balance().spendable_value(),balance.ironwood_balance().spendable_value(),balance.unshielded_regular_balance().spendable_value()]);
            for (pool, balance, value) in [("sapling",balance.sapling_balance(),50_000u64),("orchard",balance.orchard_balance(),60_000),("ironwood",balance.ironwood_balance(),60_000),("transparent",balance.unshielded_regular_balance(),70_000)] {
                assert_eq!(u64::from(balance.total()),value,"viewing total: {pool}");
                assert_eq!(u64::from(balance.spendable_value()),value*expected as u64,"summary purpose: {pool}");
            }

            let sapling=db.with_sapling_tree_mut(|tree| tree.marked_positions()).unwrap();
            let orchard=db.with_orchard_tree_mut(|tree| tree.marked_positions()).unwrap();
            let ironwood=db.with_ironwood_tree_mut(|tree| tree.marked_positions()).unwrap().unwrap();
            assert_eq!([sapling.len(),orchard.len(),ironwood.len()],[expected+1;3],"legacy={legacy}, policy={policy:?}");
            for marks in [&sapling,&orchard,&ironwood] { assert!(marks.contains(&1u64.into()),"spending mark preserved"); }
            for (bal,value) in [(summary.account_balances()[&other_id].sapling_balance(),50_000u64),(summary.account_balances()[&other_id].orchard_balance(),60_000),(summary.account_balances()[&other_id].ironwood_balance(),60_000)] {
                assert_eq!(u64::from(bal.spendable_value()),value,"mixed spending account unchanged");
            }
            let selected = db.select_spendable_notes(account_id,
                zcash_client_backend::data_api::TargetValue::AtLeast(zcash_protocol::value::Zatoshis::const_from_u64(10_000)),
                &[ShieldedPool::Sapling,ShieldedPool::Orchard,ShieldedPool::Ironwood],101u32.into(),
                zcash_client_backend::data_api::wallet::ConfirmationsPolicy::MIN,&[],LockFilter::Unfiltered).unwrap();
            assert_eq!([selected.sapling().len(),selected.orchard().len(),selected.ironwood().len()],[expected;3]);
            let address = *db.get_last_generated_address_matching(account_id,UnifiedAddressRequest::AllAvailableKeys).unwrap().unwrap().transparent().unwrap();
            let outputs = db.get_spendable_transparent_outputs(&address,101u32.into(),
                zcash_client_backend::data_api::wallet::ConfirmationsPolicy::MIN,
                zcash_client_backend::data_api::CoinbaseFilter::AllTransparentOutputs,LockFilter::Unfiltered).unwrap();
            assert_eq!(outputs.len(),expected);
            assert_eq!(db.get_unspent_transparent_output(&zcash_transparent::bundle::OutPoint::new([10;32],0),101u32.into()).unwrap().is_some(),expected==1);
            for pool in [ShieldedPool::Sapling,ShieldedPool::Orchard,ShieldedPool::Ironwood] {
                assert_eq!(db.get_spendable_note(&TxId::from_bytes([9;32]),pool,0,101u32.into(),LockFilter::Unfiltered).unwrap().is_some(),expected==1,"{pool:?}");
            }
        });
        crate::wallet::storage_close(g).unwrap();
        let conn=rusqlite::Connection::open(&path).unwrap();
        assert_eq!(policy_rows(&conn,&preserved_tables),preserved,"history/authority/checkpoints preserved on open {reopen}");
        let after=policy_rows(&conn,&tree_tables);
        if policy!=Some(true) {assert_eq!(after[..9],initial_trees[..9],"spending-purpose tree storage unchanged");}
        if legacy {
            for pool in ["sapling","orchard","ironwood"] {
                let positions:Vec<u64>=conn.prepare(&format!("SELECT mark_removed_position FROM {pool}_tree_checkpoint_marks_removed ORDER BY mark_removed_position")).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<std::result::Result<_,_>>().unwrap();
                assert_eq!(positions,if policy==Some(true) {vec![1]} else {vec![0,1]},"only view-only deferred marks removed");
            }
        }
        if let Some(before)=&migrated {assert_eq!(&after,before,"repeat-open idempotence");}
        migrated=Some(after);
        assert_eq!(conn.query_row("SELECT version FROM ext_viewing_version",[],|r|r.get::<_,u32>(0)).unwrap(),2);
        }
    }
}

#[test]
fn hd_explicit_native_index_and_next_survive_reopen() {
    for length in [32,64] {
    let (path,g)=open();
    let input=json!({"birthday":fixture(40)["birthday"],"accountIndex":0,"name":"private HD"});
    let account:Value=serde_json::from_str(&views_seed_call(g,"account_import_hd",&input.to_string(),vec![40;length]).unwrap()).unwrap();
    let p=crate::Document::parse(PARAMS).unwrap();
    let key=UnifiedSpendingKey::from_seed(&p,&vec![40;length],zip32::AccountId::ZERO).unwrap().to_unified_full_viewing_key();
    let (ua,j)=key.default_address(UnifiedAddressRequest::AllAvailableKeys).unwrap();
    assert_eq!(call(g,"address_list",json!({"accountId":account["id"]})).unwrap(),json!([address_record(&p,&key.to_unified_incoming_viewing_key(),&ua,j).unwrap()]));
    assert_eq!(account["accountIndex"],0);
    assert_eq!(account["signerAttached"],false);
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(call(g,"account_get",json!({"accountId":account["id"]})).unwrap(),account);
    let input=json!({"birthday":fixture(40)["birthday"]});
    let next:Value=serde_json::from_str(&views_seed_call(g,"account_create_hd",&input.to_string(),vec![40;length]).unwrap()).unwrap();
    assert_eq!(next["accountIndex"],1);
    crate::wallet::storage_close(g).unwrap();
    }
}

fn hd(g:u32, op:&str, seed:u8, input:Value) -> std::result::Result<Value,String> {
    views_seed_call(g,op,&input.to_string(),vec![seed;32]).map(|s|serde_json::from_str(&s).unwrap())
}
fn hd_input(index:u32) -> Value {json!({"accountIndex":index,"birthday":fixture(40)["birthday"]})}
#[test]
fn hd_gap_independent_seed_provenance_and_native_default() {
    let (path,g)=open();
    let p=crate::Document::parse(PARAMS).unwrap();
    let mut records=Vec::new();
    for index in [0,3] {
        let a=hd(g,"account_import_hd",40,hd_input(index)).unwrap();
        assert_eq!(a["accountIndex"],index);
        let key=UnifiedSpendingKey::from_seed(&p,&[40;32],zip32::AccountId::try_from(index).unwrap()).unwrap().to_unified_full_viewing_key();
        let (ua,j)=key.default_address(UnifiedAddressRequest::AllAvailableKeys).unwrap();
        assert_eq!(call(g,"address_list",json!({"accountId":a["id"]})).unwrap(),json!([address_record(&p,&key.to_unified_incoming_viewing_key(),&ua,j).unwrap()]));
        super::super::DOMAIN.with(|domain| {
            let d=domain.borrow();let db=&d.active.as_ref().unwrap().wallet;
            let native=db.get_account(id(&json!({"accountId":a["id"]})).unwrap()).unwrap().unwrap();
            assert!(matches!(native.source(),zcash_client_backend::data_api::AccountSource::Derived{..}));
            assert_eq!(native.source().key_derivation().unwrap().seed_fingerprint(),&zip32::fingerprint::SeedFingerprint::from_seed(&[40;32]).unwrap());
            assert_eq!(native.source().key_source(),None);
        });
        records.push(a);
    }
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    let args=json!({"birthday":fixture(40)["birthday"]});
    let next=hd(g,"account_create_hd",40,args.clone()).unwrap();assert_eq!(next["accountIndex"],4);
    let independent=hd(g,"account_create_hd",41,args).unwrap();assert_eq!(independent["accountIndex"],0);
    for a in records {assert_eq!(call(g,"account_get",json!({"accountId":a["id"]})).unwrap(),a);}
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn hd_seed_domain_rejects_before_mutation() {
    let (path,g)=open();
    let before=std::fs::read(&path).unwrap();
    for length in [33,0,16,31,48,63,65,252,253] {
        for op in ["account_import_hd","account_create_hd"] {
            let mut input=hd_input(0);
            if op=="account_create_hd" {input.as_object_mut().unwrap().remove("accountIndex");}
            assert_eq!(views_seed_call(g,op,&input.to_string(),vec![40;length]).unwrap_err(),"INVALID_ARGUMENT","{op} seed length {length}");
            assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([]));
            assert_eq!(std::fs::read(&path).unwrap(),before,"invalid seed leaves DB bytes unchanged");
        }
    }
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn hd_validation_and_partial_collision_roll_back_native_state() {
    let (path,g)=open();
    for index in [json!(-1),json!(2147483648u64),json!(1.5),json!("0"),Value::Null] {
        let mut v=hd_input(0);v["accountIndex"]=index;
        assert_eq!(hd(g,"account_import_hd",40,v).unwrap_err(),"INVALID_ARGUMENT");
    }
    for length in [0,31,253] {assert_eq!(views_seed_call(g,"account_import_hd",&hd_input(0).to_string(),vec![40;length]).unwrap_err(),"INVALID_ARGUMENT");}
    for pools in [json!(["sapling"]),json!([])] {
        let mut v=hd_input(0);v["enabledPools"]=pools;assert_eq!(hd(g,"account_import_hd",40,v).unwrap_err(),"UNSUPPORTED_HD_POOLS");
    }
    let mut bad=hd_input(0);bad["birthday"]["genesis"]=json!("04".repeat(32));
    assert_eq!(hd(g,"account_import_hd",40,bad).unwrap_err(),"NETWORK_MISMATCH");
    let mut bad=hd_input(0);bad["birthday"]["priorTreeState"]=json!("00");
    assert_eq!(hd(g,"account_import_hd",40,bad).unwrap_err(),"INVALID_BIRTHDAY");
    let mut bad=hd_input(0);bad["seed"]=json!("forbidden");
    assert_eq!(hd(g,"account_import_hd",40,bad).unwrap_err(),"INVALID_ARGUMENT");
    assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([]));
    let mut partial=fixture(40);partial["enabledPools"]=json!(["sapling"]);partial["viewOnly"]=json!(true);partial["name"]=json!("retain me");
    let a=call(g,"account_import",partial).unwrap();
    let conn=rusqlite::Connection::open(&path).unwrap();
    let tables=["accounts","addresses","ext_viewing_accounts","ext_viewing_addresses"];
    let before=policy_rows(&conn,&tables);
    assert_eq!(hd(g,"account_import_hd",40,hd_input(0)).unwrap_err(),"ACCOUNT_COLLISION");
    assert_eq!(hd(g,"account_create_hd",40,json!({"birthday":fixture(40)["birthday"]})).unwrap_err(),"ACCOUNT_COLLISION");
    assert_eq!(policy_rows(&conn,&tables),before);
    assert_eq!(call(g,"account_get",json!({"accountId":a["id"]})).unwrap(),a);
    crate::wallet::storage_close(g).unwrap();
    let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
    assert_eq!(policy_rows(&conn,&tables),before);
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn hd_duplicate_overflow_and_cross_import_birthday_coherence() {
    let (_path,g)=open();
    hd(g,"account_import_hd",40,hd_input(0)).unwrap();
    assert_eq!(hd(g,"account_import_hd",40,hd_input(0)).unwrap_err(),"ACCOUNT_COLLISION");
    let mut conflicting=fixture(41);
    let mut state=TreeState::decode(hex::decode(conflicting["birthday"]["priorTreeState"].as_str().unwrap()).unwrap().as_slice()).unwrap();
    state.hash="08".repeat(32);conflicting["birthday"]["priorTreeState"]=json!(hex::encode(state.encode_to_vec()));
    assert_eq!(call(g,"account_import",conflicting.clone()).unwrap_err(),"INCOHERENT_BIRTHDAY");
    crate::wallet::storage_close(g).unwrap();
    let (_path,g)=open();call(g,"account_import",conflicting).unwrap();
    assert_eq!(hd(g,"account_import_hd",40,hd_input(0)).unwrap_err(),"INCOHERENT_BIRTHDAY");
    crate::wallet::storage_close(g).unwrap();
    let (_path,g)=open();
    let mut full=hd_input(2147483647);full["birthday"]=json!("fullScan");
    let max=hd(g,"account_import_hd",40,full).unwrap();assert_eq!(max["accountIndex"],2147483647u64);
    assert_eq!(hd(g,"account_create_hd",40,json!({"birthday":"fullScan"})).unwrap_err(),"ACCOUNT_INDEX_EXHAUSTED");
    assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([max]));
    crate::wallet::storage_close(g).unwrap();
}
#[test]
fn hd_metadata_and_commit_failures_roll_back_and_reopen() {
    for commit_failure in [false,true] {
        let (path,g)=open();
        let conn=rusqlite::Connection::open(&path).unwrap();
        let tables=["accounts","addresses","ext_viewing_accounts","ext_viewing_addresses"];
        let before=policy_rows(&conn,&tables);
        if commit_failure {conn.execute_batch("BEGIN; SELECT * FROM accounts;").unwrap();}
        else {
            // Failure only fires after native account AND default-address writes exist.
            conn.execute_batch("CREATE TRIGGER hd_fail BEFORE INSERT ON ext_viewing_accounts WHEN (SELECT count(*) FROM accounts)>0 AND (SELECT count(*) FROM addresses)>0 BEGIN SELECT RAISE(ABORT,'synthetic metadata failure'); END").unwrap();
        }
        assert_eq!(hd(g,"account_create_hd",40,json!({"birthday":fixture(40)["birthday"]})).unwrap_err(),"STORAGE_ERROR");
        if commit_failure {conn.execute_batch("ROLLBACK").unwrap();}
        else {conn.execute_batch("DROP TRIGGER hd_fail").unwrap();}
        assert_eq!(policy_rows(&conn,&tables),before);
        crate::wallet::storage_close(g).unwrap();
        let g=crate::wallet::initialize_path(&path,"zcash-js-network/1",PARAMS,&[3;32]).unwrap();
        assert_eq!(call(g,"account_list",json!({})).unwrap(),json!([]));
        let a=hd(g,"account_create_hd",40,json!({"birthday":fixture(40)["birthday"]})).unwrap();assert_eq!(a["accountIndex"],0);
        crate::wallet::storage_close(g).unwrap();
        let metadata:String=conn.query_row("SELECT metadata FROM ext_viewing_accounts",[],|r|r.get(0)).unwrap();
        let value:Value=serde_json::from_str(&metadata).unwrap();assert_eq!(value.as_object().unwrap().len(),3);
        for candidate in [&path, &format!("{path}-journal")] {
            if let Ok(bytes)=std::fs::read(candidate) {
                let usk=UnifiedSpendingKey::from_seed(&crate::Document::parse(PARAMS).unwrap(),&[40;32],zip32::AccountId::ZERO).unwrap();
                for secret in [vec![40;32],hex::encode([40;32]).into_bytes(),usk.sapling().to_bytes().to_vec(),usk.orchard().to_bytes().to_vec(),usk.transparent().to_bytes()] {assert!(!bytes.windows(secret.len()).any(|w|w==secret),"synthetic secret persisted");}
            }
        }
    }
}
