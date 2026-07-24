use std::path::PathBuf;

use super::*;

#[tokio::test]
async fn test_server_creation() {
    let router = create_server(PathBuf::from("/tmp")).await;
    assert!(std::mem::size_of_val(&router) > 0);
}
