//! The restart grace's flags (validators whose last epoch was bad) survive a
//! commit, a reload and a peer snapshot on both backends, and while nobody is
//! flagged the state root is what it was before the feature.

use mersennet::mersennet_orders::MersennetOrdersState;
use mersennet::staking::ValidatorSetParams;
use mersennet::state::PersistentState;
use mersennet::state_redb::RedbState;
use mersennet::state_trait::StateBackend;
use revm::primitives::{Address, U256};
use tempfile::tempdir;

fn open(backend: &str, dir: &std::path::Path) -> Box<dyn StateBackend> {
    match backend {
        "sled" => Box::new(PersistentState::open(dir).expect("sled")),
        "redb" => Box::new(RedbState::open(dir).expect("redb")),
        other => panic!("{other}"),
    }
}

#[test]
fn the_restart_grace_flag_persists_and_travels_in_snapshots() {
    for backend in ["sled", "redb"] {
        let dir = tempdir().unwrap();
        let st = open(backend, dir.path());
        let mut orders = MersennetOrdersState::default();
        orders.staking.set_params(ValidatorSetParams {
            activation_height: 1,
            ..ValidatorSetParams::default()
        });
        let id = Address::from_slice(&[2u8; 20]);
        orders
            .staking
            .register_validator(
                Address::from_slice(&[1u8; 20]),
                id,
                U256::from(5_000u64) * U256::from(10u64).pow(U256::from(18u64)),
                700,
                5,
            )
            .unwrap();
        orders.staking.active_set = vec![id];
        st.commit_mersennet_orders(&orders).unwrap();
        let root_unflagged = st.compute_state_root();

        orders.staking.last_bad.insert(id);
        st.commit_mersennet_orders(&orders).unwrap();
        if backend == "sled" {
            assert_ne!(
                st.compute_state_root(),
                root_unflagged,
                "[sled] the flag is state"
            );
        }
        let mut loaded = MersennetOrdersState::default();
        st.load_mersennet_orders(&mut loaded).unwrap();
        assert!(loaded.staking.last_bad.contains(&id), "[{backend}] reload");

        let bytes = st
            .export_snapshot_bytes(10, st.compute_state_root())
            .unwrap();
        let dir2 = tempdir().unwrap();
        let st2 = open(backend, dir2.path());
        st2.import_snapshot_bytes(&bytes).unwrap();
        let mut restored = MersennetOrdersState::default();
        st2.load_mersennet_orders(&mut restored).unwrap();
        assert!(
            restored.staking.last_bad.contains(&id),
            "[{backend}] snapshot"
        );

        orders.staking.last_bad.clear();
        st.commit_mersennet_orders(&orders).unwrap();
        assert_eq!(
            st.compute_state_root(),
            root_unflagged,
            "[{backend}] nobody flagged: the root as before"
        );
        let mut cleared = MersennetOrdersState::default();
        st.load_mersennet_orders(&mut cleared).unwrap();
        assert!(cleared.staking.last_bad.is_empty(), "[{backend}] cleared");
    }
}
