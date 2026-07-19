# synthia-session-reexport-cleanup

Eliminate the dual-`Session`/dual-`SessionManager` name-shadowing trap in
`synthia_session::lib.rs` and add a three-layer guard (doc tests,
integration test, CI script) to prevent re-introduction.
