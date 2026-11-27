use std::collections::{HashMap, HashSet, VecDeque};

use opendash_core::redirects::{
    ConflictPolicy, CsvImportRowStatus, CsvRowError, MAX_CSV_PREVIEW_ROWS,
    MAX_MANUAL_REDIRECTS_TOTAL, MAX_REDIRECT_CHAIN_HOPS, ManualRedirectInput, ParsedCsvRow,
    check_not_self_redirect, extract_effective_target_path, parse_redirects_csv,
};
use sqlx::{SqliteConnection, SqlitePool};

use super::{RedirectStorageError, RedirectStorageResult, resolve_site_origin_in_conn};
use crate::storage::permalinks::load_live_auto_redirect_edges_in_conn;

#[derive(Debug, Clone)]
pub struct StoredCsvPreviewRow {
    pub row_number: usize,
    pub source_path: String,
    pub target_url: String,
    pub status_code: Option<u16>,
    pub enabled: Option<bool>,
    pub note: String,
    pub status: CsvImportRowStatus,
    pub error: Option<CsvRowError>,
}

#[derive(Debug, Clone)]
pub struct CsvImportExecution {
    pub dry_run: bool,
    pub total_rows: usize,
    pub create_count: usize,
    pub update_count: usize,
    pub skip_count: usize,
    pub error_count: usize,
    pub conflicts_only: bool,
    pub applied_paths: Vec<String>,
    pub rows: Vec<StoredCsvPreviewRow>,
}

struct RedirectGraphState {
    manual_edges: HashMap<String, Option<String>>,
    auto_edges: HashMap<String, String>,
    effective_edges: HashMap<String, Option<String>>,
    reverse_adj: HashMap<String, Vec<String>>,
}

impl RedirectGraphState {
    fn from_loaded(
        manual_edges: HashMap<String, Option<String>>,
        auto_edges: HashMap<String, String>,
    ) -> Self {
        let mut effective_edges: HashMap<String, Option<String>> = auto_edges
            .iter()
            .map(|(k, v)| (k.clone(), Some(v.clone())))
            .collect();
        for (src, tgt) in &manual_edges {
            effective_edges.insert(src.clone(), tgt.clone());
        }
        let mut reverse_adj: HashMap<String, Vec<String>> = HashMap::new();
        for (src, opt_tgt) in &effective_edges {
            if let Some(tgt) = opt_tgt {
                reverse_adj
                    .entry(tgt.clone())
                    .or_default()
                    .push(src.clone());
            }
        }
        Self {
            manual_edges,
            auto_edges,
            effective_edges,
            reverse_adj,
        }
    }

    fn forward_depth(&self, src: &str, start_target: &str) -> Option<usize> {
        let mut visited = HashSet::new();
        visited.insert(src);
        let mut cur = start_target;
        let mut depth = 0usize;
        loop {
            if cur == src || !visited.insert(cur) {
                return None;
            }
            if 1 + depth > MAX_REDIRECT_CHAIN_HOPS {
                return None;
            }
            match self.effective_edges.get(cur) {
                Some(Some(next)) => {
                    depth += 1;
                    if 1 + depth > MAX_REDIRECT_CHAIN_HOPS {
                        return None;
                    }
                    cur = next.as_str();
                }
                _ => return Some(depth),
            }
        }
    }

    fn backward_depth(&self, src: &str) -> usize {
        let mut visited = HashSet::new();
        visited.insert(src);
        let mut queue = VecDeque::new();
        queue.push_back((src, 0usize));
        let mut max_depth = 0usize;
        while let Some((node, dist)) = queue.pop_front() {
            if dist >= MAX_REDIRECT_CHAIN_HOPS {
                continue;
            }
            if let Some(preds) = self.reverse_adj.get(node) {
                for prev in preds {
                    if visited.insert(prev.as_str()) {
                        let next_dist = dist + 1;
                        max_depth = max_depth.max(next_dist);
                        if max_depth > MAX_REDIRECT_CHAIN_HOPS {
                            return max_depth;
                        }
                        queue.push_back((prev.as_str(), next_dist));
                    }
                }
            }
        }
        max_depth
    }

    fn check_edge(&self, src: &str, target: &str, public_origin: Option<&str>) -> bool {
        let Some(effective_target) = extract_effective_target_path(target, public_origin) else {
            return true;
        };
        if effective_target == src {
            return false;
        }
        let Some(fwd) = self.forward_depth(src, &effective_target) else {
            return false;
        };
        let bwd = self.backward_depth(src);
        bwd + 1 + fwd <= MAX_REDIRECT_CHAIN_HOPS
    }

    fn remove_effective_reverse(&mut self, src: &str, old_target: Option<String>) {
        if let Some(tgt) = old_target {
            if let Some(preds) = self.reverse_adj.get_mut(&tgt) {
                preds.retain(|s| s != src);
            }
        }
    }

    fn add_effective_reverse(&mut self, src: &str, new_target: Option<String>) {
        if let Some(tgt) = new_target {
            let preds = self.reverse_adj.entry(tgt).or_default();
            if !preds.iter().any(|s| s == src) {
                preds.push(src.to_owned());
            }
        }
    }

    fn apply_manual_rule(
        &mut self,
        src: &str,
        target: &str,
        enabled: bool,
        public_origin: Option<&str>,
    ) {
        let prev_effective = self.effective_edges.get(src).cloned().flatten();
        self.remove_effective_reverse(src, prev_effective);

        if enabled {
            let eff = extract_effective_target_path(target, public_origin);
            self.manual_edges.insert(src.to_owned(), eff.clone());
            self.effective_edges.insert(src.to_owned(), eff.clone());
            self.add_effective_reverse(src, eff);
        } else {
            self.manual_edges.remove(src);
            if let Some(auto_tgt) = self.auto_edges.get(src).cloned() {
                self.effective_edges
                    .insert(src.to_owned(), Some(auto_tgt.clone()));
                self.add_effective_reverse(src, Some(auto_tgt));
            } else {
                self.effective_edges.remove(src);
            }
        }
    }
}

async fn load_graph_state_in_conn(
    conn: &mut SqliteConnection,
    exclude_id: Option<i64>,
    public_origin: Option<&str>,
) -> RedirectStorageResult<RedirectGraphState> {
    let manual_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT source_path, target_url
           FROM manual_redirects
          WHERE enabled = 1 AND (? IS NULL OR id != ?)",
    )
    .bind(exclude_id)
    .bind(exclude_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut manual_edges = HashMap::with_capacity(manual_rows.len());
    for (src, tgt) in manual_rows {
        manual_edges.insert(src, extract_effective_target_path(&tgt, public_origin));
    }

    let auto_edges = load_live_auto_redirect_edges_in_conn(conn)
        .await
        .map_err(RedirectStorageError::from_api_err)?;

    Ok(RedirectGraphState::from_loaded(manual_edges, auto_edges))
}

pub(crate) async fn validate_redirect_graph_in_conn(
    conn: &mut SqliteConnection,
    candidates: &[ManualRedirectInput],
    exclude_id: Option<i64>,
    public_origin: Option<&str>,
) -> RedirectStorageResult<()> {
    let mut graph = load_graph_state_in_conn(conn, exclude_id, public_origin).await?;
    for cand in candidates {
        if cand.enabled {
            if !graph.check_edge(&cand.source_path, &cand.target_url, public_origin) {
                return Err(RedirectStorageError::CycleOrChainExceeded);
            }
            graph.apply_manual_rule(&cand.source_path, &cand.target_url, true, public_origin);
        } else {
            graph.apply_manual_rule(&cand.source_path, &cand.target_url, false, public_origin);
        }
    }
    Ok(())
}

pub async fn import_manual_redirects_csv(
    db: &SqlitePool,
    csv_text: &str,
    dry_run: bool,
    conflict_policy: ConflictPolicy,
    public_origin: Option<&str>,
) -> RedirectStorageResult<CsvImportExecution> {
    let parsed_rows = parse_redirects_csv(csv_text)?;
    let total_rows = parsed_rows.len();

    let mut tx = db.begin().await?;
    if !dry_run {
        sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
            .execute(&mut *tx)
            .await?;
    }

    let existing_sources: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT source_path FROM manual_redirects")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let existing_count = existing_sources.len();

    let effective_origin_owned = resolve_site_origin_in_conn(&mut tx, public_origin).await?;
    let effective_origin = effective_origin_owned.as_deref();

    let mut graph = load_graph_state_in_conn(&mut tx, None, effective_origin).await?;
    let mut seen_in_batch: HashSet<String> = HashSet::with_capacity(total_rows);
    let mut create_count = 0usize;
    let mut update_count = 0usize;
    let mut skip_count = 0usize;
    let mut error_count = 0usize;
    let mut non_conflict_errors = 0usize;

    let mut all_rows: Vec<StoredCsvPreviewRow> = Vec::with_capacity(total_rows);
    let mut writes: Vec<ParsedCsvRow> = Vec::new();

    for (idx, row_res) in parsed_rows.into_iter().enumerate() {
        let row_number = idx + 1;
        let mut row = match row_res {
            Ok(parsed) => parsed,
            Err(err) => {
                error_count += 1;
                non_conflict_errors += 1;
                all_rows.push(StoredCsvPreviewRow {
                    row_number,
                    source_path: String::new(),
                    target_url: String::new(),
                    status_code: None,
                    enabled: None,
                    note: String::new(),
                    status: CsvImportRowStatus::Error,
                    error: Some(err),
                });
                continue;
            }
        };

        if let Err(err) = check_not_self_redirect(
            &row.input.source_path,
            &row.input.target_url,
            effective_origin,
        ) {
            error_count += 1;
            non_conflict_errors += 1;
            all_rows.push(make_error_row(&row.input, row.row_number, err.into()));
            continue;
        }

        if !seen_in_batch.insert(row.input.source_path.clone()) {
            error_count += 1;
            non_conflict_errors += 1;
            all_rows.push(make_error_row(
                &row.input,
                row.row_number,
                CsvRowError::DuplicateSourceInCsv,
            ));
            continue;
        }

        let exists_in_db = existing_sources.contains(&row.input.source_path);
        let row_status = if exists_in_db {
            match conflict_policy {
                ConflictPolicy::Skip => {
                    skip_count += 1;
                    all_rows.push(StoredCsvPreviewRow {
                        row_number: row.row_number,
                        source_path: row.input.source_path,
                        target_url: row.input.target_url,
                        status_code: Some(row.input.status_code.as_u16()),
                        enabled: Some(row.input.enabled),
                        note: row.input.note,
                        status: CsvImportRowStatus::SkipExisting,
                        error: None,
                    });
                    continue;
                }
                ConflictPolicy::Error => {
                    error_count += 1;
                    all_rows.push(make_error_row(
                        &row.input,
                        row.row_number,
                        CsvRowError::ConflictExistingSource,
                    ));
                    continue;
                }
                ConflictPolicy::Overwrite => CsvImportRowStatus::Update,
            }
        } else {
            if existing_count + create_count >= MAX_MANUAL_REDIRECTS_TOTAL {
                error_count += 1;
                non_conflict_errors += 1;
                all_rows.push(make_error_row(
                    &row.input,
                    row.row_number,
                    CsvRowError::QuotaExceeded,
                ));
                continue;
            }
            CsvImportRowStatus::Create
        };

        if row.input.enabled
            && !graph.check_edge(
                &row.input.source_path,
                &row.input.target_url,
                effective_origin,
            )
        {
            error_count += 1;
            non_conflict_errors += 1;
            all_rows.push(make_error_row(
                &row.input,
                row.row_number,
                CsvRowError::CycleOrChainExceeded,
            ));
            continue;
        }

        graph.apply_manual_rule(
            &row.input.source_path,
            &row.input.target_url,
            row.input.enabled,
            effective_origin,
        );

        match row_status {
            CsvImportRowStatus::Create => create_count += 1,
            CsvImportRowStatus::Update => update_count += 1,
            _ => {}
        }

        all_rows.push(StoredCsvPreviewRow {
            row_number: row.row_number,
            source_path: row.input.source_path.clone(),
            target_url: row.input.target_url.clone(),
            status_code: Some(row.input.status_code.as_u16()),
            enabled: Some(row.input.enabled),
            note: row.input.note.clone(),
            status: row_status,
            error: None,
        });
        row.row_number = row_number;
        writes.push(row);
    }

    let conflicts_only = error_count > 0 && non_conflict_errors == 0;
    let mut applied_paths = Vec::new();

    if !dry_run && error_count == 0 {
        for item in &writes {
            sqlx::query(
                "INSERT INTO manual_redirects (source_path, target_url, status_code, enabled, note)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(source_path) DO UPDATE SET
                    target_url = excluded.target_url,
                    status_code = excluded.status_code,
                    enabled = excluded.enabled,
                    note = excluded.note,
                    version = manual_redirects.version + 1,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            )
            .bind(&item.input.source_path)
            .bind(&item.input.target_url)
            .bind(item.input.status_code.as_i64())
            .bind(i64::from(item.input.enabled))
            .bind(&item.input.note)
            .execute(&mut *tx)
            .await?;

            sqlx::query("DELETE FROM not_found_logs WHERE path = ?")
                .bind(&item.input.source_path)
                .execute(&mut *tx)
                .await?;
            applied_paths.push(item.input.source_path.clone());
        }
        tx.commit().await?;
    } else {
        tx.rollback().await?;
    }

    let rows = cap_preview_rows(all_rows);
    Ok(CsvImportExecution {
        dry_run,
        total_rows,
        create_count,
        update_count,
        skip_count,
        error_count,
        conflicts_only,
        applied_paths,
        rows,
    })
}

fn make_error_row(
    input: &ManualRedirectInput,
    row_number: usize,
    err: CsvRowError,
) -> StoredCsvPreviewRow {
    StoredCsvPreviewRow {
        row_number,
        source_path: input.source_path.clone(),
        target_url: input.target_url.clone(),
        status_code: Some(input.status_code.as_u16()),
        enabled: Some(input.enabled),
        note: input.note.clone(),
        status: CsvImportRowStatus::Error,
        error: Some(err),
    }
}

fn cap_preview_rows(all: Vec<StoredCsvPreviewRow>) -> Vec<StoredCsvPreviewRow> {
    if all.len() <= MAX_CSV_PREVIEW_ROWS {
        return all;
    }
    let mut errors = Vec::new();
    let mut others = Vec::new();
    for r in all {
        if r.status == CsvImportRowStatus::Error {
            errors.push(r);
        } else {
            others.push(r);
        }
    }
    errors.extend(others);
    errors.truncate(MAX_CSV_PREVIEW_ROWS);
    errors
}
