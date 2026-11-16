//! End-to-end checks for the `sorce` binary.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_and_doctor_work() {
    Command::cargo_bin("sorce")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("sorce 0.1.0"));

    Command::cargo_bin("sorce")
        .unwrap()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("built-in engine"))
        .stdout(predicate::str::contains("external backends"));
}

#[test]
fn formats_json_lists_png() {
    let output = Command::cargo_bin("sorce")
        .unwrap()
        .args(["formats", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(json.is_array());
    assert!(text.contains("png"));
}

#[test]
fn bare_png_to_pdf_uses_convert_shorthand() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pixel.png");
    let output = dir.path().join("pixel.pdf");

    image::RgbaImage::from_pixel(16, 16, image::Rgba([220, 40, 90, 255]))
        .save(&input)
        .unwrap();

    Command::cargo_bin("sorce")
        .unwrap()
        .arg(&input)
        .args(["-o", output.to_str().unwrap()])
        .assert()
        .success();

    let bytes = std::fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

#[test]
fn inspect_json_reports_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("tiny.png");
    image::RgbaImage::from_pixel(7, 5, image::Rgba([1, 2, 3, 255]))
        .save(&input)
        .unwrap();

    let output = Command::cargo_bin("sorce")
        .unwrap()
        .args(["inspect", "--json"])
        .arg(&input)
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let record = json.as_array().unwrap().first().unwrap();
    assert_eq!(record["width"], 7);
    assert_eq!(record["height"], 5);
    assert_eq!(record["format"], "PNG");
}
