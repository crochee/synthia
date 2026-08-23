//! Integration tests for OTLP exporter protocol auto-selection.
//!
//! Verifies [`synthia_telemetry::detect_protocol`] picks gRPC vs HTTP based on
//! the `SYNTHIA_OTLP_ENDPOINT` URL scheme and the gRPC/HTTP standard ports
//! (4317 / 4318), preserving backward compatibility with the prior gRPC-only
//! behavior.

use synthia_telemetry::{OtlpProtocol, detect_protocol};

#[test]
fn http_scheme_with_4318_port_selects_http() {
    assert_eq!(
        detect_protocol("http://localhost:4318/v1/traces"),
        OtlpProtocol::Http
    );
}

#[test]
fn grpc_scheme_selects_grpc() {
    assert_eq!(detect_protocol("grpc://localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn https_scheme_selects_grpc() {
    assert_eq!(
        detect_protocol("https://collector.example.com:4317"),
        OtlpProtocol::Grpc
    );
}

#[test]
fn no_scheme_defaults_to_grpc() {
    assert_eq!(detect_protocol("localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn http_scheme_with_4317_port_selects_grpc_for_backward_compat() {
    assert_eq!(detect_protocol("http://localhost:4317"), OtlpProtocol::Grpc);
}

#[test]
fn http_scheme_with_4318_port_no_path_selects_http() {
    assert_eq!(detect_protocol("http://localhost:4318"), OtlpProtocol::Http);
}

#[test]
fn grpc_scheme_forces_grpc_regardless_of_http_port() {
    // `grpc://` is explicit and MUST force gRPC even on the HTTP standard port.
    assert_eq!(detect_protocol("grpc://collector:4318"), OtlpProtocol::Grpc);
}

#[test]
fn http_scheme_with_other_port_selects_http() {
    assert_eq!(
        detect_protocol("http://collector.example.com:9999/v1/traces"),
        OtlpProtocol::Http
    );
}

#[test]
fn http_scheme_without_port_selects_http() {
    assert_eq!(
        detect_protocol("http://collector.example.com/v1/traces"),
        OtlpProtocol::Http
    );
}

#[test]
fn leading_whitespace_is_trimmed() {
    assert_eq!(
        detect_protocol("  http://localhost:4318  "),
        OtlpProtocol::Http
    );
}
