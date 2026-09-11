use zakura_network_bindings::wallet;

fn document() -> Vec<u8> {
    br#"{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}"#.to_vec()
}

#[test]
fn real_owned_wallet_migrates_closes_and_reopens() {
    let root = std::env::var("WALLET_TEST_ROOT").expect("fresh owned scratch required");
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = format!("{root}/native-{}-{stamp}.db", std::process::id());
    assert!(!std::path::Path::new(&path).exists());
    let params = document();
    let generation = wallet::initialize_path(&path, "zcash-js-network/1", &params, &[3; 32]).unwrap();
    assert_eq!(wallet::storage_binding(generation).unwrap(), params);
    wallet::storage_close(generation).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let migrations: u32 = db.query_row("SELECT count(*) FROM schemer_migrations", [], |r| r.get(0)).unwrap();
    assert!(migrations > 60, "actual pinned wallet migrations");
    assert_eq!(db.query_row("SELECT count(*) FROM accounts", [], |r| r.get::<_, u32>(0)).unwrap(), 0);
    assert_eq!(db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0)).unwrap(), "ok");
    db.close().unwrap();
    assert_eq!(wallet::initialize_path(&path, "zcash-js-network/1", &params, &[4; 32]).unwrap_err(), "NETWORK_MISMATCH");
    let changed = String::from_utf8(params.clone()).unwrap().replace("\"Nu6_3\":100", "\"Nu6_3\":101").into_bytes();
    assert_eq!(wallet::initialize_path(&path, "zcash-js-network/1", &changed, &[3; 32]).unwrap_err(), "NETWORK_MISMATCH");
    let reopened = wallet::initialize_path(&path, "zcash-js-network/1", &params, &[3; 32]).unwrap();
    assert_ne!(generation, reopened);
    assert_eq!(wallet::storage_binding(generation).unwrap_err(), "STALE_HANDLE");
    wallet::storage_close(reopened).unwrap();
    assert_eq!(wallet::storage_close(reopened).unwrap_err(), "STALE_HANDLE");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("INSERT INTO schemer_migrations(id) VALUES(zeroblob(16))", []).unwrap();
    db.close().unwrap();
    assert_eq!(wallet::initialize_path(&path, "zcash-js-network/1", &params, &[3; 32]).unwrap_err(), "SCHEMA_MISMATCH");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("DELETE FROM schemer_migrations WHERE id=zeroblob(16)", []).unwrap();
    db.execute_batch("PRAGMA user_version=999;").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(wallet::initialize_path(&path, "zcash-js-network/1", &params, &[3; 32]).unwrap_err(), "SCHEMA_MISMATCH");
    assert_eq!(std::fs::read(&path).unwrap(), before, "unknown schema unchanged");
}
