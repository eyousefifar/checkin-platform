//! Security contract tests — no real credentials or private addresses.

use axum::http::Uri;
use pksp_api::http_request_path;

#[test]
fn http_request_path_omits_query_marker() {
    let uri: Uri = "/api/health?token=benign-query-marker-not-a-secret"
        .parse()
        .unwrap();
    let label = http_request_path(&uri);
    assert_eq!(label, "/api/health");
    assert!(
        !label.contains("token") && !label.contains("benign-query-marker"),
        "query must not appear in path label: {label}"
    );
}

#[test]
fn http_request_path_keeps_nested_paths() {
    let uri: Uri = "/api/employees/12?token=x".parse().unwrap();
    assert_eq!(http_request_path(&uri), "/api/employees/12");
}
