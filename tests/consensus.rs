use zakura_network_bindings::consensus_branch;
use zcash_protocol::consensus::BranchId;

const FORMAT: &str = "zcash-js-network/1";
const DOC: &[u8] = br#"{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}"#;

#[test]
fn explicit_schedule_selects_real_consensus_branch() {
    assert_eq!(consensus_branch(FORMAT, DOC, 9), Ok(u32::from(BranchId::Sprout)));
    assert_eq!(consensus_branch(FORMAT, DOC, 10), Ok(u32::from(BranchId::Overwinter)));
    assert_eq!(consensus_branch(FORMAT, DOC, 20), Ok(u32::from(BranchId::Sapling)));
    assert_eq!(consensus_branch(FORMAT, DOC, 100), Ok(u32::from(BranchId::Nu6_3)));
}

#[test]
fn document_rejections() {
    let doc = std::str::from_utf8(DOC).unwrap();
    for bad in [doc.to_owned() + "\n", doc.replace("\"Sapling\":20", "\"Sapling\":9"),
        doc.replace("\"Overwinter\":10", "\"Overwinter\":null"),
        doc.replace("\"Overwinter\":10", "\"Overwinter\":01"),
        doc.replace("\"Overwinter\":10", "\"Overwinter\":4294967296")] {
        assert!(consensus_branch(FORMAT, bad.as_bytes(), 20).is_err());
    }
    assert!(consensus_branch("unknown", DOC, 20).is_err());
    assert!(consensus_branch(FORMAT, &[255], 20).is_err());
    assert!(consensus_branch(FORMAT, &[b' '; 257], 20).is_err());
}

#[test]
fn every_upgrade_boundary_without_marker_defaults() {
    use zcash_protocol::consensus::NetworkUpgrade::*;
    let upgrades = [Overwinter, Sapling, Blossom, Heartwood, Canopy, Nu5, Nu6, Nu6_1, Nu6_2, Nu6_3];
    let names = ["Overwinter", "Sapling", "Blossom", "Heartwood", "Canopy", "Nu5", "Nu6", "Nu6_1", "Nu6_2", "Nu6_3"];
    for family in ["main", "test", "regtest"] {
        for heights in [std::array::from_fn::<_, 10, _>(|i| Some((i as u32 + 1) * 10)), [None; 10], [Some(0); 10], [Some(u32::MAX); 10]] {
            let doc = format!("{{\"encoding\":\"{family}\"{}}}", names.iter().zip(heights).map(|(key, h)| format!(",\"{key}\":{}", h.map_or("null".into(), |n| n.to_string()))).collect::<String>());
            for height in [0, 9, 10, 11, 19, 20, 21, 29, 30, 31, 39, 40, 41, 49, 50, 51, 59, 60, 61, 69, 70, 71, 79, 80, 81, 89, 90, 91, 99, 100, 101, u32::MAX] {
                let expected = heights.iter().rposition(|h| h.is_some_and(|h| h <= height)).map_or(BranchId::Sprout, |i| upgrades[i].branch_id());
                assert_eq!(consensus_branch(FORMAT, doc.as_bytes(), height), Ok(u32::from(expected)));
            }
        }
    }
}
