//! Tests for the V4A parser.

use std::path::PathBuf;

use super::{
    error::ParseError,
    parser::parse_v4a,
    types::{Hunk, HunkLine, PatchOp},
};

#[test]
fn parse_add_file() {
    let patch = "*** Begin Patch\n*** Add File: foo.txt\n+hello\n+world\n*** End Patch\n";
    let ops = parse_v4a(patch).unwrap();
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        PatchOp::Add { path, content } => {
            assert_eq!(path, &PathBuf::from("foo.txt"));
            assert_eq!(content, "hello\nworld\n");
        }
        _ => panic!("expected Add op"),
    }
}

#[test]
fn parse_update_file_with_hunk() {
    let patch = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n fn foo() {\n-    bar()\n+    baz()\n }\n*** End Patch\n";
    let ops = parse_v4a(patch).unwrap();
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        PatchOp::Update {
            path,
            hunks,
            move_to,
        } => {
            assert_eq!(path, &PathBuf::from("src/lib.rs"));
            assert_eq!(hunks.len(), 1);
            // Lines are stored in source order; the first `fn foo() {` and
            // trailing `}` are context, the bar/baz line is the change.
            assert_eq!(
                hunks[0].lines,
                vec![
                    HunkLine::Context("fn foo() {".to_string()),
                    HunkLine::Deletion("    bar()".to_string()),
                    HunkLine::Insertion("    baz()".to_string()),
                    HunkLine::Context("}".to_string()),
                ]
            );
            assert!(move_to.is_none());
        }
        _ => panic!("expected Update op"),
    }
}

#[test]
fn parse_update_file_with_move() {
    let patch = "*** Begin Patch\n*** Update File: old.rs\n*** Move to: new.rs\n@@\n-a\n+b\n*** End Patch\n";
    let ops = parse_v4a(patch).unwrap();
    match &ops[0] {
        PatchOp::Update { path, move_to, .. } => {
            assert_eq!(path, &PathBuf::from("old.rs"));
            assert_eq!(move_to, &Some(PathBuf::from("new.rs")));
        }
        _ => panic!("expected Update op"),
    }
}

#[test]
fn parse_delete_file() {
    let patch = "*** Begin Patch\n*** Delete File: trash.txt\n*** End Patch\n";
    let ops = parse_v4a(patch).unwrap();
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        PatchOp::Delete { path } => {
            assert_eq!(path, &PathBuf::from("trash.txt"))
        }
        _ => panic!("expected Delete op"),
    }
}

#[test]
fn parse_multiple_ops() {
    let patch = "*** Begin Patch\n\
                 *** Add File: a.txt\n+x\n\
                 *** Update File: b.txt\n@@\n-old\n+new\n\
                 *** Delete File: c.txt\n\
                 *** End Patch\n";
    let ops = parse_v4a(patch).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(matches!(&ops[0], PatchOp::Add { .. }));
    assert!(matches!(&ops[1], PatchOp::Update { .. }));
    assert!(matches!(&ops[2], PatchOp::Delete { .. }));
}

#[test]
fn parse_rejects_missing_begin_marker() {
    let err = parse_v4a("*** End Patch\n").unwrap_err();
    assert_eq!(err, ParseError::MissingBeginMarker);
}

#[test]
fn parse_rejects_missing_end_marker() {
    let err =
        parse_v4a("*** Begin Patch\n*** Add File: a.txt\n+x\n").unwrap_err();
    assert_eq!(err, ParseError::MissingEndMarker);
}

#[test]
fn parse_rejects_empty_patch() {
    let err = parse_v4a("*** Begin Patch\n*** End Patch\n").unwrap_err();
    assert_eq!(err, ParseError::EmptyPatch);
}

#[test]
fn parse_rejects_unknown_header() {
    let err =
        parse_v4a("*** Begin Patch\n*** Bogus Op: a.txt\n*** End Patch\n")
            .unwrap_err();
    assert!(matches!(err, ParseError::UnknownOpHeader(_)));
}

#[test]
fn parse_handles_crlf() {
    let patch =
        "*** Begin Patch\r\n*** Add File: a.txt\r\n+x\r\n*** End Patch\r\n";
    let ops = parse_v4a(patch).unwrap();
    assert_eq!(ops.len(), 1);
}

#[test]
fn hunk_old_and_new_text() {
    let h = Hunk {
        lines: vec![
            HunkLine::Context("line1".to_string()),
            HunkLine::Insertion("inserted".to_string()),
            HunkLine::Deletion("deleted".to_string()),
        ],
        end_of_file: false,
    };
    assert_eq!(h.old_text(), "line1\ndeleted\n");
    assert_eq!(h.new_text(), "line1\ninserted\n");
}

#[test]
fn hunk_interleaved_context_deletion() {
    // Scenario 021-style: context/del/context must reconstruct
    // surrounding text in source order, not context-then-deletion.
    let h = Hunk {
        lines: vec![
            HunkLine::Context("line1".to_string()),
            HunkLine::Deletion("line2".to_string()),
            HunkLine::Context("line3".to_string()),
        ],
        end_of_file: false,
    };
    assert_eq!(h.old_text(), "line1\nline2\nline3\n");
    assert_eq!(h.new_text(), "line1\nline3\n");
}
