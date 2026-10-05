use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

type TestResult = Result<(), Box<dyn Error>>;

fn padless(config: Option<&Path>, args: &[&str]) -> io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_padless"));
    if let Some(path) = config {
        command.arg("--config").arg(path);
    }
    command.args(args).output()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn config_file(name: &str, contents: &str) -> io::Result<PathBuf> {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cli-tests");
    fs::create_dir_all(&directory)?;
    let path = directory.join(name);
    fs::write(&path, contents)?;
    Ok(path)
}

#[test]
fn lookup_uses_windows_table_by_default() -> TestResult {
    let path = config_file("defaults.toml", "")?;
    let output = padless(Some(&path), &["lookup", "0233"])?;
    assert!(output.status.success());
    assert_eq!(stdout(&output), "\u{E9}\tU+00E9\twindows table\n");
    Ok(())
}

#[test]
fn lookup_distinguishes_leading_zero() -> TestResult {
    let path = config_file("leading-zero.toml", "")?;
    let output = padless(Some(&path), &["lookup", "130"])?;
    assert_eq!(stdout(&output), "\u{E9}\tU+00E9\twindows table\n");
    let output = padless(Some(&path), &["lookup", "0130"])?;
    assert_eq!(stdout(&output), "\u{201A}\tU+201A\twindows table\n");
    Ok(())
}

#[test]
fn lookup_prefers_aliases() -> TestResult {
    let path = config_file(
        "aliases.toml",
        "table = \"unicode\"\n[aliases]\n\"42\" = \"\u{2192}x\"\n",
    )?;
    let output = padless(Some(&path), &["lookup", "42"])?;
    assert_eq!(stdout(&output), "\u{2192}x\tU+2192 U+0078\talias\n");
    let output = padless(Some(&path), &["lookup", "8364"])?;
    assert_eq!(stdout(&output), "\u{20AC}\tU+20AC\tunicode table\n");
    Ok(())
}

#[test]
fn lookup_rejects_invalid_and_unmapped_codes() -> TestResult {
    let path = config_file("unicode.toml", "table = \"unicode\"\n")?;
    let output = padless(Some(&path), &["lookup", "12a"])?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not a valid code"));
    let output = padless(Some(&path), &["lookup", "55296"])?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains("does not produce a character"));
    Ok(())
}

#[test]
fn config_check_reports_errors_and_warnings() -> TestResult {
    let valid = config_file("valid.toml", "trigger = \"right-alt\"\n")?;
    let output = padless(Some(&valid), &["config", "check"])?;
    assert!(output.status.success());
    assert!(stdout(&output).ends_with(": ok\n"));
    assert!(stderr(&output).contains("warning: trigger right-alt is AltGr"));

    let invalid = config_file("invalid.toml", "min_digits = 0\n")?;
    let output = padless(Some(&invalid), &["config", "check"])?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains("min_digits must be between 1 and 10"));
    Ok(())
}

#[test]
fn explicit_missing_config_is_an_error() -> TestResult {
    let output = padless(Some(Path::new("does-not-exist.toml")), &["config", "check"])?;
    assert!(!output.status.success());
    assert!(stderr(&output).contains("cannot read does-not-exist.toml"));
    Ok(())
}

#[test]
fn config_path_prints_the_location() -> TestResult {
    let output = padless(None, &["config", "path"])?;
    assert!(output.status.success());
    assert!(stdout(&output).trim_end().ends_with("config.toml"));
    Ok(())
}
