use std::path::PathBuf;

use clap::Parser as _;

use super::Arguments;

#[test]
fn cli_parses_screenshot_pipeline_and_direct_file_arguments() {
    let bare = Arguments::try_parse_from(["gds-images"]).expect("bare launch");
    assert_eq!(bare.file, None);
    assert!(!bare.stdin);
    assert!(!bare.clipboard);

    let pipe = Arguments::try_parse_from([
        "gds-images",
        "--stdin",
        "--copy-on-save",
        "--early-exit",
        "-o",
        "/tmp/out.png",
    ])
    .expect("stdin capture args");
    assert!(pipe.stdin);
    assert!(pipe.copy_on_save);
    assert!(pipe.early_exit);
    assert_eq!(pipe.output, Some(PathBuf::from("/tmp/out.png")));

    let dash = Arguments::try_parse_from(["gds-images", "-"]).expect("dash stdin");
    assert_eq!(dash.file, Some(PathBuf::from("-")));

    let clip = Arguments::try_parse_from(["gds-images", "--clipboard"]).expect("clipboard arg");
    assert!(clip.clipboard);
}
