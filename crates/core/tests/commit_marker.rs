//! The interrupted-commit marker must behave identically on both storage
//! backends: set by `begin_commit`, cleared by `end_commit`, and visible on
//! reopen when a commit never finished. Until 2026-09-25 the redb backend
//! inherited the trait's no-op defaults, so a hard kill mid-commit resumed
//! silently from inconsistent state (field report on f67d812).

use mersennet::state::PersistentState;
use mersennet::state_redb::RedbState;
use mersennet::state_trait::StateBackend;
use tempfile::tempdir;

fn open(backend: &str, dir: &std::path::Path) -> Box<dyn StateBackend> {
    match backend {
        "sled" => Box::new(PersistentState::open(dir).expect("sled")),
        "redb" => Box::new(RedbState::open(dir).expect("redb")),
        other => panic!("{other}"),
    }
}

#[test]
fn interrupted_commit_marker_survives_reopen_on_both_backends() {
    for backend in ["sled", "redb"] {
        let dir = tempdir().expect("dir");
        {
            let state = open(backend, dir.path());
            assert_eq!(
                state.interrupted_commit().unwrap(),
                None,
                "[{backend}] fresh store"
            );
            state.begin_commit(42).unwrap();
            state.end_commit().unwrap();
            assert_eq!(
                state.interrupted_commit().unwrap(),
                None,
                "[{backend}] a completed commit leaves no marker"
            );
            state.begin_commit(43).unwrap();
            // …process dies here.
        }
        let state = open(backend, dir.path());
        assert_eq!(
            state.interrupted_commit().unwrap(),
            Some(43),
            "[{backend}] the marker of an unfinished commit is visible after reopen"
        );
        state.end_commit().unwrap();
        assert_eq!(
            state.interrupted_commit().unwrap(),
            None,
            "[{backend}] cleared"
        );
    }
}

#[test]
fn redb_open_creates_the_state_directory() {
    let dir = tempdir().expect("dir");
    let nested = dir.path().join("data").join("state");
    assert!(!nested.exists());
    let state = RedbState::open(&nested).expect("open creates parent dirs");
    assert_eq!(state.persisted_height().unwrap(), None);
    assert!(nested.join("mersennet.redb").exists());
}
