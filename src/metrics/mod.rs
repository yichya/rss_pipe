use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use http::{StatusCode, header};
use http_body_util::Full;
use hyper::{Request, Response, body::Incoming};

use crate::{common, storage};

static GLOBAL_HTTP_200: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_304: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_502: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_503: AtomicU64 = AtomicU64::new(0);
static GLOBAL_PIPE_ERR: AtomicU64 = AtomicU64::new(0);

pub fn status_code_200() {
    GLOBAL_HTTP_200.fetch_add(1, Ordering::Relaxed);
}

pub fn status_code_304() {
    GLOBAL_HTTP_304.fetch_add(1, Ordering::Relaxed);
}

pub fn status_code_502() {
    GLOBAL_HTTP_502.fetch_add(1, Ordering::Relaxed);
}

pub fn status_code_503() {
    GLOBAL_HTTP_503.fetch_add(1, Ordering::Relaxed);
}

pub fn pipe_error() {
    GLOBAL_PIPE_ERR.fetch_add(1, Ordering::Relaxed);
}

pub struct Metrics {
    db: String,
    statistics: Option<Vec<String>>,
}

impl Metrics {
    pub fn new(db: &str, statistics: Option<Vec<String>>) -> Self {
        Self {
            db: db.to_owned(),
            statistics,
        }
    }

    pub async fn handle_metrics(&self) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let metrics_value = storage::transaction(&self.db, |tx| {
            let unread_count = storage::items::get_total_items(tx, "where is_read = 0");
            format!(
                "# RSS Pipe Metrics\n\
                rss_pipe_status_code_count{{status_code=\"200\"}} {}\n\
                rss_pipe_status_code_count{{status_code=\"304\"}} {}\n\
                rss_pipe_status_code_count{{status_code=\"502\"}} {}\n\
                rss_pipe_status_code_count{{status_code=\"503\"}} {}\n\
                rss_pipe_error_count{{}} {}\n\
                rss_pipe_unread_count{{}} {}\n",
                GLOBAL_HTTP_200.load(Ordering::Relaxed),
                GLOBAL_HTTP_304.load(Ordering::Relaxed),
                GLOBAL_HTTP_502.load(Ordering::Relaxed),
                GLOBAL_HTTP_503.load(Ordering::Relaxed),
                GLOBAL_PIPE_ERR.load(Ordering::Relaxed),
                unread_count,
            )
        });
        Response::builder()
            .header(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8; escaping=values",
            )
            .body(Full::from(metrics_value))
            .map_err(|e| e.into())
    }

    pub async fn handle_statistics(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let path = req.uri().path();
        let index_str = path.split('/').next_back().unwrap_or("");
        let index: usize = match index_str.parse() {
            Ok(i) => i,
            Err(_) => return common::not_found(),
        };

        let statistics = match &self.statistics {
            Some(v) => v,
            None => return common::not_found(),
        };

        let sql = match statistics.get(index) {
            Some(s) => s.clone(),
            None => return common::not_found(),
        };

        match storage::execute_query(&self.db, &sql) {
            Ok(result) => {
                let html = build_statistics_html(&result, &sql, index);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Full::new(Bytes::from(html)))
                    .map_err(|e| e.into())
            }
            Err(e) => {
                println!("!! statistics query error: {}", e);
                common::internal_server_error()
            }
        }
    }
}

fn build_statistics_html(query_result: &storage::QueryResult, sql: &str, index: usize) -> String {
    const TEMPLATE: &str = include_str!("statistics.html");
    let columns = &query_result.columns;
    let rows = &query_result.rows;

    let table_headers: String = columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            format!(
                "<th onclick='toggleFilter({})' style='cursor: pointer; user-select: none;'>{} <span class='sort-indicator' data-col='{}'></span></th>",
                i, col, i
            )
        })
        .collect();

    let mut max_col_widths: Vec<usize> = vec![0; columns.len()];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            max_col_widths[i] = max_col_widths[i].max(cell.len());
        }
    }
    let max_width_col_idx = max_col_widths
        .iter()
        .enumerate()
        .max_by_key(|(_, w)| *w)
        .map(|(i, _)| i)
        .unwrap_or(0);

    let table_rows: String = rows
        .iter()
        .map(|row| {
            let cells: String = row
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let class = if i == max_width_col_idx {
                        " class='ellipsis'"
                    } else {
                        ""
                    };
                    format!("<td{} title='{}'>{}</td>", class, cell.replace('"', "&quot;"), cell)
                })
                .collect();
            format!("<tr>{}</tr>", cells)
        })
        .collect();

    let tbody_content = if rows.is_empty() {
        format!(
            "<tr><td colspan='{}' class='no-data'>No data found</td></tr>",
            columns.len()
        )
    } else {
        table_rows
    };

    TEMPLATE
        .replace("{INDEX}", &index.to_string())
        .replace("{SQL}", sql)
        .replace("{HEADERS}", &table_headers)
        .replace("{ROWS}", &tbody_content)
}
