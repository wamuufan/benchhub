use benchhub::db::Db;
use benchhub::engine::BenchmarkEngine;
use benchhub::models::RunResult;
use benchhub::service::BenchHubService;
use std::sync::Arc;

#[tokio::test]
#[allow(clippy::field_reassign_with_default)]
async fn test_methodology_creation_and_deletion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Db::new(db_path).unwrap();
    let engine = BenchmarkEngine::new(temp_dir.path().to_path_buf());
    let service = BenchHubService::new(
        db.clone(),
        engine,
        Arc::new(vec![]),
        temp_dir.path().to_path_buf(),
    );

    let mut run1 = RunResult::default();
    run1.benchmark_id = "7zip".to_string();
    run1.score = Some(10000.0);
    run1.duration_secs = 10.0;
    run1.avg_cpu_temp = 60.0;
    let id1 = db.insert_run(&run1).unwrap();

    let mut run2 = RunResult::default();
    run2.benchmark_id = "7zip".to_string();
    run2.score = Some(11000.0);
    run2.duration_secs = 12.0;
    run2.avg_cpu_temp = 62.0;
    let id2 = db.insert_run(&run2).unwrap();

    let mut run3 = RunResult::default();
    run3.benchmark_id = "7zip".to_string();
    run3.score = Some(12000.0);
    run3.duration_secs = 14.0;
    run3.avg_cpu_temp = 64.0;
    let id3 = db.insert_run(&run3).unwrap();

    let methodology = service.create_methodology(&[id1, id2, id3]).unwrap();

    assert_eq!(methodology.score, Some(11000.0));
    assert_eq!(methodology.duration_secs, 12.0);
    assert_eq!(methodology.avg_cpu_temp, 62.0);
    assert!(methodology.is_methodology);

    let children = service.get_methodology_children(methodology.id).unwrap();
    assert_eq!(children.len(), 3);
    assert!(children.iter().any(|c| c.id == id1));

    service.delete_history(methodology.id).unwrap();

    let run1_after = db.get_run_by_id(id1).unwrap().unwrap();
    assert_eq!(run1_after.methodology_parent_id, None);

    let methodology_after = db.get_run_by_id(methodology.id).unwrap();
    assert!(methodology_after.is_none());
}
