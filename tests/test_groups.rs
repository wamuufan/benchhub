use anyhow::Result;
use benchhub::db::Db;
use benchhub::models::RunResult;

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_groups() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let db_path = tmp_dir.path().join("test_groups.db");
    let db = Db::new(db_path)?;
    let mut run1 = RunResult::default();
    run1.benchmark_id = "test_bench_1".to_string();
    let id1 = db.insert_run(&run1)?;

    let mut run2 = RunResult::default();
    run2.benchmark_id = "test_bench_2".to_string();
    let id2 = db.insert_run(&run2)?;

    let mut run3 = RunResult::default();
    run3.benchmark_id = "test_bench_3".to_string();
    let id3 = db.insert_run(&run3)?;

    // Set runs group
    db.set_runs_group(&[id1, id2], "Undervolt")?;

    // Verify distinct groups
    let groups = db.get_distinct_groups()?;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0], "Undervolt");

    // Verify groups in runs
    let fetch1 = db.get_run_by_id(id1)?.unwrap();
    assert_eq!(fetch1.group_name.as_deref(), Some("Undervolt"));

    let fetch2 = db.get_run_by_id(id2)?.unwrap();
    assert_eq!(fetch2.group_name.as_deref(), Some("Undervolt"));

    let fetch3 = db.get_run_by_id(id3)?.unwrap();
    assert_eq!(fetch3.group_name, None);

    // Remove from group
    db.remove_runs_from_group(&[id1])?;
    let fetch1 = db.get_run_by_id(id1)?.unwrap();
    assert_eq!(fetch1.group_name, None);

    // Verify group distinct again
    let groups = db.get_distinct_groups()?;
    assert_eq!(groups.len(), 1);

    // Delete group
    db.delete_group("Undervolt")?;
    let fetch2 = db.get_run_by_id(id2)?.unwrap();
    assert_eq!(fetch2.group_name, None);

    let groups = db.get_distinct_groups()?;
    assert!(groups.is_empty());

    // Ensure runs still exist
    assert!(db.get_run_by_id(id2)?.is_some());

    Ok(())
}
