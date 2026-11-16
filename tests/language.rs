//! Everything in the repository is English. Korean may appear only in `*.ko.md` files (explanation for readers), in
//! tests (sample content, patterns), and in a README's language switcher link. Characters are written as escapes so
//! this file does not trip over itself.
use std::path::Path;
use std::process::Command;

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7a3)
}

#[test]
fn korean_only_in_ko_md_files_and_tests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git ls-files failed");
    // "[<Korean for Korean>](README.ko.md)": the switcher link at the top of README.md.
    let switcher = "[\u{d55c}\u{ad6d}\u{c5b4}](README.ko.md)";
    let mut found = Vec::new();
    for path in String::from_utf8_lossy(&out.stdout).lines() {
        let dirs: Vec<&str> = path.split('/').collect();
        if path.ends_with(".ko.md") || dirs[..dirs.len() - 1].contains(&"tests") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(path)) else { continue };
        for (n, line) in text.lines().enumerate() {
            if line.replace(switcher, "").chars().any(is_hangul) {
                found.push(format!("{path}:{}: {}", n + 1, line.trim().chars().take(80).collect::<String>()));
            }
        }
    }
    assert!(found.is_empty(), "Korean text outside a .ko.md file or a test:\n{}", found.join("\n"));
}
