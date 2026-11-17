use crate::models::{HistorySortOrder, RunResult, TelemetryData};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const RUNS_SELECT_COLUMNS: &str = "id, benchmark_id, category, preset_or_version, gpu_mode, score, status, timestamp, duration_secs, avg_cpu_usage, peak_cpu_usage, avg_cpu_temp, peak_cpu_temp, avg_cpu_freq_mhz, peak_cpu_freq_mhz, avg_gpu_usage, peak_gpu_usage, avg_gpu_temp, peak_gpu_temp, avg_gpu_freq_mhz, peak_gpu_freq_mhz, avg_vram_freq_mhz, peak_vram_freq_mhz, avg_gpu_power_w, peak_gpu_power_w, avg_power_w, peak_power_w, avg_ac_power_w, peak_ac_power_w, avg_ram_gb, peak_ram_gb, avg_vram_gb, peak_vram_gb, cpu_throttling, gpu_throttling, system_info_summary, power_profile, log_path, is_methodology, methodology_parent_id, group_name";

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    #[tracing::instrument]
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open SQLite database at {:?}", db_path))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                benchmark_id TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'General',
                preset_or_version TEXT NOT NULL DEFAULT '',
                gpu_mode TEXT NOT NULL DEFAULT 'auto',
                score REAL,
                status TEXT NOT NULL DEFAULT 'completed',
                timestamp INTEGER NOT NULL,
                duration_secs REAL NOT NULL DEFAULT 0.0,
                avg_cpu_usage REAL DEFAULT 0.0,
                peak_cpu_usage REAL DEFAULT 0.0,
                avg_cpu_temp REAL DEFAULT 0.0,
                peak_cpu_temp REAL DEFAULT 0.0,
                avg_cpu_freq_mhz INTEGER DEFAULT 0,
                peak_cpu_freq_mhz INTEGER DEFAULT 0,
                avg_gpu_usage REAL DEFAULT 0.0,
                peak_gpu_usage REAL DEFAULT 0.0,
                avg_gpu_temp REAL DEFAULT 0.0,
                peak_gpu_temp REAL DEFAULT 0.0,
                avg_gpu_freq_mhz INTEGER DEFAULT 0,
                peak_gpu_freq_mhz INTEGER DEFAULT 0,
                avg_vram_freq_mhz INTEGER DEFAULT 0,
                peak_vram_freq_mhz INTEGER DEFAULT 0,
                avg_gpu_power_w REAL DEFAULT 0.0,
                peak_gpu_power_w REAL DEFAULT 0.0,
                avg_power_w REAL DEFAULT 0.0,
                peak_power_w REAL DEFAULT 0.0,
                avg_ac_power_w REAL DEFAULT 0.0,
                peak_ac_power_w REAL DEFAULT 0.0,
                avg_ram_gb REAL DEFAULT 0.0,
                peak_ram_gb REAL DEFAULT 0.0,
                avg_vram_gb REAL DEFAULT 0.0,
                peak_vram_gb REAL DEFAULT 0.0,
                cpu_throttling TEXT NOT NULL DEFAULT 'none',
                gpu_throttling TEXT NOT NULL DEFAULT 'none',
                system_info_summary TEXT NOT NULL DEFAULT '',
                power_profile TEXT NOT NULL DEFAULT 'unknown',
                log_path TEXT NOT NULL DEFAULT ''
            );
             CREATE TABLE IF NOT EXISTS telemetry_points (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id INTEGER NOT NULL,
                 timestamp_ms INTEGER NOT NULL,
                 cpu_temp REAL,
                 gpu_temp REAL,
                 cpu_freq_mhz REAL,
                 gpu_freq_mhz REAL DEFAULT 0.0,
                 vram_freq_mhz REAL DEFAULT 0.0,
                 power_watt REAL,
                 gpu_power_watt REAL DEFAULT 0.0,
                 ac_power_watt REAL DEFAULT 0.0,
                 ram_usage_mb REAL,
                 vram_usage_mb REAL DEFAULT 0.0,
                 cpu_throttle TEXT DEFAULT 'none',
                 gpu_throttle TEXT DEFAULT 'none',
                 FOREIGN KEY (run_id) REFERENCES runs(id) ON DELETE CASCADE
             );
             CREATE INDEX IF NOT EXISTS idx_telemetry_run_id ON telemetry_points(run_id);",
        )
        .context("Failed to initialize database tables")?;

        Self::migrate(&conn).context("Failed to execute database migrations")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    #[tracing::instrument(skip_all)]
    fn migrate_missing_columns(
        conn: &Connection,
        table: &str,
        migrations: &[(&str, &str)],
    ) -> Result<()> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({});", table))?;
        let existing_cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();

        for (col, def) in migrations {
            if !existing_cols.iter().any(|c| c == col) {
                let query = format!("ALTER TABLE {} ADD COLUMN {} {};", table, col, def);
                if let Err(e) = conn.execute(&query, []) {
                    let err_msg = e.to_string().to_lowercase();
                    if err_msg.contains("duplicate column") {
                        tracing::debug!("Column {} already exists", col);
                    } else {
                        tracing::error!("Migration error ({}.{}): {}", table, col, e);
                    }
                }
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn migrate(conn: &Connection) -> Result<()> {
        let run_migrations: &[(&str, &str)] = &[
            ("category", "TEXT NOT NULL DEFAULT 'General'"),
            ("preset_or_version", "TEXT NOT NULL DEFAULT ''"),
            ("gpu_mode", "TEXT NOT NULL DEFAULT 'auto'"),
            ("status", "TEXT NOT NULL DEFAULT 'completed'"),
            ("duration_secs", "REAL NOT NULL DEFAULT 0.0"),
            ("avg_cpu_usage", "REAL DEFAULT 0.0"),
            ("peak_cpu_usage", "REAL DEFAULT 0.0"),
            ("avg_cpu_temp", "REAL DEFAULT 0.0"),
            ("peak_cpu_temp", "REAL DEFAULT 0.0"),
            ("avg_cpu_freq_mhz", "INTEGER DEFAULT 0"),
            ("peak_cpu_freq_mhz", "INTEGER DEFAULT 0"),
            ("avg_gpu_usage", "REAL DEFAULT 0.0"),
            ("peak_gpu_usage", "REAL DEFAULT 0.0"),
            ("avg_gpu_temp", "REAL DEFAULT 0.0"),
            ("peak_gpu_temp", "REAL DEFAULT 0.0"),
            ("avg_gpu_freq_mhz", "INTEGER DEFAULT 0"),
            ("peak_gpu_freq_mhz", "INTEGER DEFAULT 0"),
            ("avg_vram_freq_mhz", "INTEGER DEFAULT 0"),
            ("peak_vram_freq_mhz", "INTEGER DEFAULT 0"),
            ("avg_gpu_power_w", "REAL DEFAULT 0.0"),
            ("peak_gpu_power_w", "REAL DEFAULT 0.0"),
            ("avg_power_w", "REAL DEFAULT 0.0"),
            ("peak_power_w", "REAL DEFAULT 0.0"),
            ("avg_ac_power_w", "REAL DEFAULT 0.0"),
            ("peak_ac_power_w", "REAL DEFAULT 0.0"),
            ("avg_ram_gb", "REAL DEFAULT 0.0"),
            ("peak_ram_gb", "REAL DEFAULT 0.0"),
            ("avg_vram_gb", "REAL DEFAULT 0.0"),
            ("peak_vram_gb", "REAL DEFAULT 0.0"),
            ("cpu_throttling", "TEXT NOT NULL DEFAULT 'none'"),
            ("gpu_throttling", "TEXT NOT NULL DEFAULT 'none'"),
            ("system_info_summary", "TEXT NOT NULL DEFAULT ''"),
            ("power_profile", "TEXT NOT NULL DEFAULT 'unknown'"),
            ("log_path", "TEXT NOT NULL DEFAULT ''"),
            ("is_methodology", "INTEGER NOT NULL DEFAULT 0"),
            ("methodology_parent_id", "INTEGER DEFAULT NULL"),
            ("group_name", "TEXT DEFAULT NULL"),
        ];
        Self::migrate_missing_columns(conn, "runs", run_migrations)?;

        let mut stmt = conn.prepare("PRAGMA table_info(runs);")?;
        let existing_cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        for (old_col, new_col) in [
            ("avg_temp", "avg_cpu_temp"),
            ("max_temp", "peak_cpu_temp"),
            ("avg_power", "avg_power_w"),
        ] {
            if existing_cols.iter().any(|c| c == old_col) {
                let _ = conn.execute(
                    &format!(
                        "UPDATE runs SET {} = {} WHERE {} IS NOT NULL;",
                        new_col, old_col, old_col
                    ),
                    [],
                );
            }
        }

        let telemetry_migrations: &[(&str, &str)] = &[
            ("cpu_usage", "REAL DEFAULT 0.0"),
            ("gpu_usage", "REAL DEFAULT 0.0"),
            ("gpu_freq_mhz", "REAL DEFAULT 0.0"),
            ("vram_freq_mhz", "REAL DEFAULT 0.0"),
            ("gpu_power_watt", "REAL DEFAULT 0.0"),
            ("ac_power_watt", "REAL DEFAULT 0.0"),
            ("vram_usage_mb", "REAL DEFAULT 0.0"),
            ("cpu_throttle", "TEXT DEFAULT 'none'"),
            ("gpu_throttle", "TEXT DEFAULT 'none'"),
        ];
        Self::migrate_missing_columns(conn, "telemetry_points", telemetry_migrations)
    }

    fn insert_run_with_conn(conn: &Connection, run: &RunResult) -> Result<i64> {
        conn.execute(
            "INSERT INTO runs (
                benchmark_id, category, preset_or_version, gpu_mode, score, status,
                timestamp, duration_secs, avg_cpu_usage, peak_cpu_usage, avg_cpu_temp, peak_cpu_temp,
                avg_cpu_freq_mhz, peak_cpu_freq_mhz, avg_gpu_usage, peak_gpu_usage, avg_gpu_temp,
                peak_gpu_temp, avg_gpu_freq_mhz, peak_gpu_freq_mhz, avg_vram_freq_mhz, peak_vram_freq_mhz,
                avg_gpu_power_w, peak_gpu_power_w, avg_power_w, peak_power_w,
                avg_ac_power_w, peak_ac_power_w, avg_ram_gb, peak_ram_gb,
                avg_vram_gb, peak_vram_gb,
                cpu_throttling, gpu_throttling,
                system_info_summary, power_profile, log_path,
                is_methodology, methodology_parent_id, group_name
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40)",
            params![
                run.benchmark_id,
                run.category,
                run.preset_or_version,
                run.gpu_mode,
                run.score,
                run.status,
                run.timestamp,
                run.duration_secs,
                run.avg_cpu_usage,
                run.peak_cpu_usage,
                run.avg_cpu_temp,
                run.peak_cpu_temp,
                run.avg_cpu_freq_mhz,
                run.peak_cpu_freq_mhz,
                run.avg_gpu_usage,
                run.peak_gpu_usage,
                run.avg_gpu_temp,
                run.peak_gpu_temp,
                run.avg_gpu_freq_mhz,
                run.peak_gpu_freq_mhz,
                run.avg_vram_freq_mhz,
                run.peak_vram_freq_mhz,
                run.avg_gpu_power_w,
                run.peak_gpu_power_w,
                run.avg_power_w,
                run.peak_power_w,
                run.avg_ac_power_w,
                run.peak_ac_power_w,
                run.avg_ram_gb,
                run.peak_ram_gb,
                run.avg_vram_gb,
                run.peak_vram_gb,
                run.cpu_throttling,
                run.gpu_throttling,
                run.system_info_summary,
                run.power_profile,
                run.log_path,
                run.is_methodology,
                run.methodology_parent_id,
                &run.group_name,
            ],
        )
        .context("Failed to insert run into database")?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_run(&self, run: &RunResult) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Self::insert_run_with_conn(&conn, run)
    }

    pub fn update_run(&self, run: &RunResult) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE runs SET
                category = ?1,
                preset_or_version = ?2,
                gpu_mode = ?3,
                score = ?4,
                status = ?5,
                duration_secs = ?6,
                avg_cpu_usage = ?7,
                peak_cpu_usage = ?8,
                avg_cpu_temp = ?9,
                peak_cpu_temp = ?10,
                avg_cpu_freq_mhz = ?11,
                peak_cpu_freq_mhz = ?12,
                avg_gpu_usage = ?13,
                peak_gpu_usage = ?14,
                avg_gpu_temp = ?15,
                peak_gpu_temp = ?16,
                avg_gpu_freq_mhz = ?17,
                peak_gpu_freq_mhz = ?18,
                avg_vram_freq_mhz = ?19,
                peak_vram_freq_mhz = ?20,
                avg_gpu_power_w = ?21,
                peak_gpu_power_w = ?22,
                avg_power_w = ?23,
                peak_power_w = ?24,
                avg_ac_power_w = ?25,
                peak_ac_power_w = ?26,
                avg_ram_gb = ?27,
                peak_ram_gb = ?28,
                avg_vram_gb = ?29,
                peak_vram_gb = ?30,
                cpu_throttling = ?31,
                gpu_throttling = ?32,
                system_info_summary = ?33,
                power_profile = ?34,
                log_path = ?35,
                is_methodology = ?36,
                methodology_parent_id = ?37,
                group_name = ?38
            WHERE id = ?39",
            params![
                run.category,
                run.preset_or_version,
                run.gpu_mode,
                run.score,
                run.status,
                run.duration_secs,
                run.avg_cpu_usage,
                run.peak_cpu_usage,
                run.avg_cpu_temp,
                run.peak_cpu_temp,
                run.avg_cpu_freq_mhz,
                run.peak_cpu_freq_mhz,
                run.avg_gpu_usage,
                run.peak_gpu_usage,
                run.avg_gpu_temp,
                run.peak_gpu_temp,
                run.avg_gpu_freq_mhz,
                run.peak_gpu_freq_mhz,
                run.avg_vram_freq_mhz,
                run.peak_vram_freq_mhz,
                run.avg_gpu_power_w,
                run.peak_gpu_power_w,
                run.avg_power_w,
                run.peak_power_w,
                run.avg_ac_power_w,
                run.peak_ac_power_w,
                run.avg_ram_gb,
                run.peak_ram_gb,
                run.avg_vram_gb,
                run.peak_vram_gb,
                run.cpu_throttling,
                run.gpu_throttling,
                run.system_info_summary,
                run.power_profile,
                run.log_path,
                run.is_methodology,
                run.methodology_parent_id,
                &run.group_name,
                run.id,
            ],
        )
        .context("Failed to update run in database")?;
        Ok(())
    }

    pub fn delete_run(&self, id: i64) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction().context("Failed to start transaction")?;

        let mut is_methodology = false;
        if let Ok(mut stmt) = tx.prepare("SELECT is_methodology FROM runs WHERE id = ?1") {
            if let Ok(mut rows) = stmt.query(params![id]) {
                if let Ok(Some(row)) = rows.next() {
                    is_methodology = row.get::<_, i64>(0).unwrap_or(0) != 0;
                }
            }
        }

        if is_methodology {
            tx.execute(
                "UPDATE runs SET methodology_parent_id = NULL WHERE methodology_parent_id = ?1",
                params![id],
            )
            .context("Failed to detach children from methodology")?;
        }

        tx.execute(
            "DELETE FROM telemetry_points WHERE run_id = ?1",
            params![id],
        )
        .context("Failed to delete telemetry points for run")?;

        tx.execute("DELETE FROM runs WHERE id = ?1", params![id])
            .context("Failed to delete run record from database")?;

        tx.commit().context("Failed to commit transaction")?;
        Ok(())
    }

    pub fn create_methodology_record(
        &self,
        methodology_run: &RunResult,
        child_ids: &[i64],
    ) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction().context("Failed to start transaction")?;
        let id = Self::insert_run_with_conn(&tx, methodology_run)?;
        {
            let mut stmt = tx
                .prepare("UPDATE runs SET methodology_parent_id = ?1 WHERE id = ?2")
                .context("Failed to prepare methodology child update")?;
            for child_id in child_ids {
                stmt.execute(params![id, child_id])
                    .context("Failed to update methodology_parent_id for child")?;
            }
        }
        tx.commit()
            .context("Failed to commit methodology record transaction")?;
        Ok(id)
    }

    pub fn get_methodology_children(&self, methodology_id: i64) -> Result<Vec<RunResult>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let query = format!(
            "SELECT {} FROM runs WHERE methodology_parent_id = ?1 ORDER BY timestamp ASC",
            RUNS_SELECT_COLUMNS
        );
        let mut stmt = conn.prepare(&query)?;
        let run_iter = stmt.query_map(params![methodology_id], Self::map_row_to_run_result)?;
        let mut results = Vec::new();
        for run in run_iter {
            results.push(run?);
        }
        Ok(results)
    }

    pub fn get_history(&self) -> Result<Vec<RunResult>> {
        self.get_filtered_history("", "", "", "", HistorySortOrder::DateDesc)
    }

    fn map_row_to_run_result(row: &rusqlite::Row) -> rusqlite::Result<RunResult> {
        Ok(RunResult {
            id: row.get(0)?,
            benchmark_id: row.get(1)?,
            category: row.get(2).unwrap_or_else(|_| "General".to_string()),
            preset_or_version: row.get(3).unwrap_or_default(),
            gpu_mode: row.get(4).unwrap_or_else(|_| "auto".to_string()),
            score: row.get(5)?,
            status: row.get(6)?,
            timestamp: row.get(7)?,
            duration_secs: row.get(8).unwrap_or(0.0),
            avg_cpu_usage: row.get(9).unwrap_or(0.0),
            peak_cpu_usage: row.get(10).unwrap_or(0.0),
            avg_cpu_temp: row.get(11).unwrap_or(0.0),
            peak_cpu_temp: row.get(12).unwrap_or(0.0),
            avg_cpu_freq_mhz: row.get(13).unwrap_or(0),
            peak_cpu_freq_mhz: row.get(14).unwrap_or(0),
            avg_gpu_usage: row.get(15).unwrap_or(0.0),
            peak_gpu_usage: row.get(16).unwrap_or(0.0),
            avg_gpu_temp: row.get(17).unwrap_or(0.0),
            peak_gpu_temp: row.get(18).unwrap_or(0.0),
            avg_gpu_freq_mhz: row.get(19).unwrap_or(0),
            peak_gpu_freq_mhz: row.get(20).unwrap_or(0),
            avg_vram_freq_mhz: row.get(21).unwrap_or(0),
            peak_vram_freq_mhz: row.get(22).unwrap_or(0),
            avg_gpu_power_w: row.get(23).unwrap_or(0.0),
            peak_gpu_power_w: row.get(24).unwrap_or(0.0),
            avg_power_w: row.get(25).unwrap_or(0.0),
            peak_power_w: row.get(26).unwrap_or(0.0),
            avg_ac_power_w: row.get(27).unwrap_or(0.0),
            peak_ac_power_w: row.get(28).unwrap_or(0.0),
            avg_ram_gb: row.get(29).unwrap_or(0.0),
            peak_ram_gb: row.get(30).unwrap_or(0.0),
            avg_vram_gb: row.get(31).unwrap_or(0.0),
            peak_vram_gb: row.get(32).unwrap_or(0.0),
            cpu_throttling: row.get(33).unwrap_or_else(|_| "none".to_string()),
            gpu_throttling: row.get(34).unwrap_or_else(|_| "none".to_string()),
            system_info_summary: row.get(35).unwrap_or_default(),
            power_profile: row.get(36).unwrap_or_else(|_| "unknown".to_string()),
            log_path: row.get(37).unwrap_or_default(),
            is_methodology: row.get::<_, i64>(38).unwrap_or(0) != 0,
            methodology_parent_id: row.get(39).ok(),
            group_name: row.get(40).ok(),
        })
    }

    pub fn get_run_by_id(&self, id: i64) -> Result<Option<RunResult>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let query = format!("SELECT {} FROM runs WHERE id = ?1", RUNS_SELECT_COLUMNS);
        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query_map(params![id], Self::map_row_to_run_result)?;

        if let Some(row_res) = rows.next() {
            Ok(Some(row_res?))
        } else {
            Ok(None)
        }
    }

    pub fn get_runs_by_ids(&self, ids: &[i64]) -> Result<Vec<RunResult>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let query = format!(
            "SELECT {} FROM runs WHERE id IN ({}) ORDER BY timestamp ASC",
            RUNS_SELECT_COLUMNS, placeholders
        );
        let mut stmt = conn.prepare(&query)?;

        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let run_iter = stmt.query_map(params.as_slice(), Self::map_row_to_run_result)?;

        let mut results = Vec::new();
        for run in run_iter {
            results.push(run?);
        }
        Ok(results)
    }

    pub fn get_filtered_history(
        &self,
        search: &str,
        category: &str,
        gpu_mode: &str,
        status: &str,
        sort_order: HistorySortOrder,
    ) -> Result<Vec<RunResult>> {
        self.get_filtered_history_with_limit(
            search,
            category,
            gpu_mode,
            status,
            sort_order,
            Some(300),
        )
    }

    /// Returns all run records ordered by timestamp descending without the UI row limit,
    /// intended for complete CSV history exports.
    pub fn get_all_history_for_export(&self) -> Result<Vec<RunResult>> {
        self.get_filtered_history_with_limit(
            "",
            "all",
            "all",
            "all",
            HistorySortOrder::DateDesc,
            None,
        )
    }

    pub fn get_filtered_history_with_limit(
        &self,
        search: &str,
        category: &str,
        gpu_mode: &str,
        status: &str,
        sort_order: HistorySortOrder,
        limit: Option<usize>,
    ) -> Result<Vec<RunResult>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let mut query = format!("SELECT {} FROM runs WHERE 1=1", RUNS_SELECT_COLUMNS);

        let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let search_trimmed = search.trim();
        if !search_trimmed.is_empty() {
            query.push_str(" AND (benchmark_id LIKE ? OR preset_or_version LIKE ? OR category LIKE ? OR system_info_summary LIKE ?)");
            let pattern = format!("%{}%", search_trimmed);
            query_params.push(Box::new(pattern.clone()));
            query_params.push(Box::new(pattern.clone()));
            query_params.push(Box::new(pattern.clone()));
            query_params.push(Box::new(pattern));
        }

        let cat_trimmed = category.trim();
        if !cat_trimmed.is_empty()
            && !cat_trimmed.eq_ignore_ascii_case("all")
            && !cat_trimmed.eq_ignore_ascii_case("Category: All")
        {
            if cat_trimmed.eq_ignore_ascii_case("CPU") {
                query.push_str(" AND category LIKE '%CPU%'");
            } else if cat_trimmed.eq_ignore_ascii_case("GPU") {
                query.push_str(" AND category LIKE '%GPU%'");
            } else if cat_trimmed.contains("Render") {
                query.push_str(" AND category LIKE '%Render%'");
            } else if cat_trimmed.contains("System") {
                query.push_str(" AND category LIKE '%System%'");
            } else {
                query.push_str(" AND (category = ? OR category LIKE ?)");
                let pattern = format!("%{}%", cat_trimmed);
                query_params.push(Box::new(cat_trimmed.to_string()));
                query_params.push(Box::new(pattern));
            }
        }

        let gpu_trimmed = gpu_mode.trim();
        if !gpu_trimmed.is_empty()
            && !gpu_trimmed.eq_ignore_ascii_case("all")
            && !gpu_trimmed.eq_ignore_ascii_case("GPU: All")
        {
            let lower_gpu = gpu_trimmed.to_lowercase();
            if lower_gpu.contains("nvidia")
                || lower_gpu.contains("dedicated")
                || lower_gpu.contains("dgpu")
            {
                query.push_str(" AND (LOWER(gpu_mode) LIKE '%nvidia%' OR LOWER(gpu_mode) LIKE '%dedicated%' OR LOWER(gpu_mode) LIKE '%dgpu%')");
            } else if lower_gpu.contains("igpu") || lower_gpu.contains("integrated") {
                query.push_str(
                    " AND (LOWER(gpu_mode) LIKE '%igpu%' OR LOWER(gpu_mode) LIKE '%integrated%')",
                );
            } else {
                query.push_str(" AND gpu_mode LIKE ?");
                query_params.push(Box::new(format!("%{}%", gpu_trimmed)));
            }
        }

        let status_trimmed = status.trim();
        if !status_trimmed.is_empty()
            && !status_trimmed.eq_ignore_ascii_case("all")
            && !status_trimmed.eq_ignore_ascii_case("Status: All")
        {
            let lower_status = status_trimmed.to_lowercase();
            if lower_status.contains("completed") || lower_status.contains("success") {
                query.push_str(
                    " AND (LOWER(status) LIKE '%completed%' OR LOWER(status) LIKE '%success%')",
                );
            } else if lower_status.contains("error") || lower_status.contains("failed") {
                query.push_str(
                    " AND (LOWER(status) LIKE '%error%' OR LOWER(status) LIKE '%failed%')",
                );
            } else if lower_status.contains("cancel") || lower_status.contains("stopped") {
                query.push_str(
                    " AND (LOWER(status) LIKE '%cancel%' OR LOWER(status) LIKE '%stopped%')",
                );
            } else {
                query.push_str(" AND status LIKE ?");
                query_params.push(Box::new(format!("%{}%", status_trimmed)));
            }
        }

        query.push_str(&format!(" ORDER BY {}", sort_order.to_sql_order_clause()));
        if let Some(max_rows) = limit {
            query.push_str(&format!(" LIMIT {}", max_rows));
        }

        let mut stmt = conn.prepare(&query)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> =
            query_params.iter().map(|b| b.as_ref()).collect();

        let run_iter = stmt.query_map(params_slice.as_slice(), Self::map_row_to_run_result)?;

        let mut results = Vec::new();
        for run in run_iter {
            results.push(run?);
        }
        Ok(results)
    }

    pub fn clear_history(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction().context("Failed to start transaction")?;
        tx.execute("DELETE FROM telemetry_points", [])
            .context("Failed to clear telemetry_points table")?;
        tx.execute("DELETE FROM runs", [])
            .context("Failed to clear runs table")?;
        tx.commit()
            .context("Failed to commit clear_history transaction")?;
        Ok(())
    }

    pub fn insert_telemetry_point(&self, run_id: i64, sample: &TelemetryData) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO telemetry_points (run_id, timestamp_ms, cpu_temp, gpu_temp, cpu_usage, gpu_usage, cpu_freq_mhz, gpu_freq_mhz, vram_freq_mhz, power_watt, gpu_power_watt, ac_power_watt, ram_usage_mb, vram_usage_mb, cpu_throttle, gpu_throttle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                run_id,
                sample.timestamp,
                sample.cpu_temp,
                sample.gpu_temp,
                sample.cpu_usage,
                sample.gpu_usage,
                sample.cpu_freq,
                sample.gpu_freq_mhz,
                sample.vram_freq_mhz,
                sample.power_w,
                sample.gpu_power_w,
                sample.ac_power_w,
                sample.ram_usage_mb,
                sample.vram_usage_mb,
                sample.cpu_throttle,
                sample.gpu_throttle,
            ],
        )
        .with_context(|| format!("Failed to insert telemetry sample for run_id {}", run_id))?;
        Ok(())
    }

    pub fn insert_telemetry_sample(&self, run_id: i64, sample: &TelemetryData) -> Result<()> {
        self.insert_telemetry_point(run_id, sample)
    }

    pub fn insert_telemetry_batch(&self, run_id: i64, samples: &[TelemetryData]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO telemetry_points (run_id, timestamp_ms, cpu_temp, gpu_temp, cpu_usage, gpu_usage, cpu_freq_mhz, gpu_freq_mhz, vram_freq_mhz, power_watt, gpu_power_watt, ac_power_watt, ram_usage_mb, vram_usage_mb, cpu_throttle, gpu_throttle)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;
            for sample in samples {
                stmt.execute(params![
                    run_id,
                    sample.timestamp,
                    sample.cpu_temp,
                    sample.gpu_temp,
                    sample.cpu_usage,
                    sample.gpu_usage,
                    sample.cpu_freq,
                    sample.gpu_freq_mhz,
                    sample.vram_freq_mhz,
                    sample.power_w,
                    sample.gpu_power_w,
                    sample.ac_power_w,
                    sample.ram_usage_mb,
                    sample.vram_usage_mb,
                    &sample.cpu_throttle,
                    &sample.gpu_throttle,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_telemetry_points(&self, run_id: i64) -> Result<Vec<TelemetryData>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT timestamp_ms, cpu_temp, gpu_temp, cpu_usage, gpu_usage, cpu_freq_mhz, gpu_freq_mhz, vram_freq_mhz, power_watt, gpu_power_watt, ac_power_watt, ram_usage_mb, vram_usage_mb, cpu_throttle, gpu_throttle FROM telemetry_points WHERE run_id = ?1 ORDER BY timestamp_ms ASC")
            .with_context(|| format!("Failed to prepare telemetry points query for run_id {}", run_id))?;
        let point_iter = stmt
            .query_map([run_id], |row| {
                Ok(TelemetryData {
                    timestamp: row.get(0)?,
                    cpu_temp: row.get(1)?,
                    gpu_temp: row.get(2)?,
                    cpu_usage: row.get(3).unwrap_or(0.0),
                    gpu_usage: row.get(4).unwrap_or(0.0),
                    cpu_freq: row.get(5)?,
                    gpu_freq_mhz: row.get(6).unwrap_or(0.0),
                    vram_freq_mhz: row.get(7).unwrap_or(0.0),
                    power_w: row.get(8)?,
                    gpu_power_w: row.get(9).unwrap_or(0.0),
                    ac_power_w: row.get(10).unwrap_or(0.0),
                    ram_usage_mb: row.get(11)?,
                    vram_usage_mb: row.get(12).unwrap_or(0.0),
                    cpu_throttle: row.get(13).unwrap_or_else(|_| "none".to_string()),
                    gpu_throttle: row.get(14).unwrap_or_else(|_| "none".to_string()),
                })
            })
            .with_context(|| format!("Failed to query telemetry points for run_id {}", run_id))?;

        let mut results = Vec::new();
        for p in point_iter {
            results.push(p?);
        }
        Ok(results)
    }

    pub fn set_runs_group(&self, run_ids: &[i64], group_name: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE runs SET group_name = ?1 WHERE id = ?2")?;
            for id in run_ids {
                stmt.execute(params![group_name, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_runs_from_group(&self, run_ids: &[i64]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE runs SET group_name = NULL WHERE id = ?1")?;
            for id in run_ids {
                stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_group(&self, group_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE runs SET group_name = NULL WHERE group_name = ?1",
            params![group_name],
        )?;
        Ok(())
    }

    pub fn rename_group(&self, old_name: &str, new_name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE runs SET group_name = ?2 WHERE group_name = ?1",
            params![old_name, new_name],
        )?;
        Ok(())
    }

    pub fn get_distinct_groups(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT DISTINCT group_name FROM runs WHERE group_name IS NOT NULL AND group_name != '' ORDER BY group_name ASC",
        )?;
        let group_iter = stmt.query_map([], |row| row.get(0))?;
        let mut groups = Vec::new();
        for name in group_iter.flatten() {
            groups.push(name);
        }
        Ok(groups)
    }
}
