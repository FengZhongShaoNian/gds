use std::{io::Cursor, path::PathBuf, sync::Arc};

use super::{
    CAPTURE_APP_ID, LaunchMode, STANDARD_APP_ID, StartupOutcome, read_bounded_stdin,
    resolve_startup_with_reader,
};
use crate::app::cli::Arguments;

fn base_args() -> Arguments {
    Arguments {
        file: None,
        stdin: false,
        clipboard: false,
        output: None,
        copy_on_save: false,
        early_exit: false,
        enable_logs: false,
    }
}

#[test]
fn empty_stdin_causes_silent_exit_before_gui() {
    let args = Arguments {
        stdin: true,
        copy_on_save: true,
        early_exit: true,
        ..base_args()
    };
    let outcome =
        resolve_startup_with_reader(&args, Cursor::new(Vec::<u8>::new())).expect("empty stdin");
    assert_eq!(outcome, StartupOutcome::SilentExit);

    let dash_args = Arguments {
        file: Some(PathBuf::from("-")),
        ..base_args()
    };
    let dash_outcome = resolve_startup_with_reader(&dash_args, Cursor::new(Vec::<u8>::new()))
        .expect("empty dash stdin");
    assert_eq!(dash_outcome, StartupOutcome::SilentExit);
}

#[test]
fn capture_modes_assign_capture_app_id_while_viewer_uses_standard_id() {
    let stdin_args = Arguments {
        stdin: true,
        early_exit: true,
        ..base_args()
    };
    let StartupOutcome::Launch(stdin_cfg) =
        resolve_startup_with_reader(&stdin_args, Cursor::new(b"\x89PNG\r\n\x1a\n".to_vec()))
            .expect("non-empty stdin")
    else {
        panic!("expected launch for non-empty stdin");
    };
    assert_eq!(stdin_cfg.app_id, CAPTURE_APP_ID);
    assert!(stdin_cfg.is_capture);
    assert!(stdin_cfg.workflow.early_exit);
    assert_eq!(
        stdin_cfg.mode,
        LaunchMode::Stdin(Arc::from(b"\x89PNG\r\n\x1a\n".as_slice()))
    );

    let viewer_args = Arguments {
        file: Some(PathBuf::from("/tmp/photo.png")),
        ..base_args()
    };
    let StartupOutcome::Launch(viewer_cfg) =
        resolve_startup_with_reader(&viewer_args, Cursor::new(Vec::<u8>::new()))
            .expect("file launch")
    else {
        panic!("expected launch for file");
    };
    assert_eq!(viewer_cfg.app_id, STANDARD_APP_ID);
    assert!(!viewer_cfg.is_capture);
    assert_eq!(
        viewer_cfg.mode,
        LaunchMode::OpenFile(PathBuf::from("/tmp/photo.png"))
    );
}

#[test]
fn relative_cli_file_and_output_paths_resolve_against_cwd() {
    let cwd = std::env::current_dir().expect("cwd");
    let args = Arguments {
        file: Some(PathBuf::from("./photo.png")),
        output: Some(PathBuf::from("out.png")),
        ..base_args()
    };
    let StartupOutcome::Launch(cfg) =
        resolve_startup_with_reader(&args, Cursor::new(Vec::<u8>::new())).expect("relative paths")
    else {
        panic!("expected launch");
    };
    assert_eq!(cfg.mode, LaunchMode::OpenFile(cwd.join("photo.png")));
    assert_eq!(cfg.workflow.output, Some(cwd.join("out.png")));
}

#[test]
fn bounded_stdin_rejects_oversized_payload() {
    let err =
        read_bounded_stdin(Cursor::new(vec![1u8; 32]), 16).expect_err("oversized stdin must fail");
    assert!(err.to_string().contains("exceeds 16 byte limit"));
}
