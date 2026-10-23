use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

const DUP: &str = r#"extends Node

func spawn_wave(index: int) -> void:
	var count := 5 + index * 2
	for i in range(count):
		var enemy := enemy_scene.instantiate()
		enemy.health = 10 + index * 5
		add_child(enemy)
	print("wave %d" % index)


func spawn_boss_wave(level: int) -> void:
	var total := 5 + level * 2
	for j in range(total):
		var boss := enemy_scene.instantiate()
		boss.health = 10 + level * 5
		add_child(boss)
	print("wave %d" % level)


func unrelated(a, b):
	if a > b:
		return a
	return b
"#;

fn run(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("similarity-gd").unwrap();
    cmd.arg(dir.path());
    cmd
}

fn setup(name: &str, content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(name), content).unwrap();
    dir
}

#[test]
fn detects_duplicate_functions() {
    let dir = setup("spawner.gd", DUP);
    run(&dir)
        .args(["--min-lines", "3", "--threshold", "0.8"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spawn_wave"))
        .stdout(predicate::str::contains("spawn_boss_wave"))
        .stdout(predicate::str::contains("Total duplicate pairs found: 1"));
}

#[test]
fn detects_inner_class_and_static_functions() {
    let src = "class A:\n\tfunc f(x):\n\t\tvar y = x * 2\n\t\tprint(y)\n\t\treturn y\n\nstatic func g(x):\n\tvar y = x * 2\n\tprint(y)\n\treturn y\n";
    let dir = setup("a.gd", src);
    run(&dir)
        .args(["--threshold", "0.9", "--no-size-penalty"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Total duplicate pairs found: 1"));
}

#[test]
fn print_shows_code() {
    let dir = setup("spawner.gd", DUP);
    run(&dir)
        .args(["--print", "--threshold", "0.8"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enemy_scene.instantiate()"));
}

#[test]
fn threshold_filters_results() {
    let src = DUP.replace("\tprint(\"wave %d\" % level)", "\tprint(\"boss\")\n\tlevel += 1");
    let dir = setup("spawner.gd", &src);
    run(&dir)
        .args(["--threshold", "0.99"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No duplicate functions found"));
}

#[test]
fn fail_on_duplicates_exit_code() {
    let dir = setup("spawner.gd", DUP);
    run(&dir).args(["--threshold", "0.8", "--fail-on-duplicates"]).assert().failure();
}

#[test]
fn ignores_non_gd_files() {
    let dir = setup("spawner.txt", DUP);
    run(&dir).assert().success().stdout(predicate::str::contains("No GDScript files found"));
}

#[test]
fn works_on_example_file() {
    Command::cargo_bin("similarity-gd")
        .unwrap()
        .args(["../../examples/duplicate_gdscript.gd", "--threshold", "0.8"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spawn_wave"));
}
