use zakura_network_bindings::transaction_id;

#[test]
fn exact_source_vectors_and_lossy_rejection() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!("transaction-vectors.json")).unwrap();
    for v in vectors.as_array().unwrap() {
        let bytes = hex::decode(v["hex"].as_str().unwrap()).unwrap();
        let id = transaction_id(&bytes, v["branch"].as_u64().unwrap() as u32).unwrap();
        assert_eq!(hex::encode(id), v["txid"].as_str().unwrap());
    }
    let lossy = hex::decode("0400008085202f89000000000000000000000100000000000000000000").unwrap();
    assert_eq!(transaction_id(&lossy, 0x76b809bb).unwrap_err(), "serialization differs from input");
}
