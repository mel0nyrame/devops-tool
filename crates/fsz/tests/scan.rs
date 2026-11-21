use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scan_output(path: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_fsz"))
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn directory_overview_excludes_links_from_rows_and_total() {
    let fixture = std::env::temp_dir().join(format!("fsz-scan-{}", std::process::id()));
    fs::create_dir(&fixture).unwrap();
    let fixture = Fixture(fixture);
    let root = fixture.0.join("scan root");
    let sub = root.join("sub");
    let outside = fixture.0.join("outside");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(root.join("first"), [0; 3]).unwrap();
    fs::write(sub.join("second"), [0; 5]).unwrap();
    fs::write(outside.join("extra"), [0; 13]).unwrap();

    let direct_link = root.join("direct-link");
    for link in [&direct_link, &sub.join("nested-link")] {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, link).unwrap();
        #[cfg(windows)]
        {
            let output = Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(link)
                .arg(&outside)
                .output()
                .unwrap();
            assert!(output.status.success(), "{:?}", output);
        }
    }

    let stdout = scan_output(&root);
    let rows: Vec<_> = stdout
        .lines()
        .filter_map(|line| {
            let mut cells = line.strip_prefix('│')?.split('┆').map(str::trim);
            Some((cells.next()?, cells.next()?))
        })
        .collect();
    assert_eq!(rows, [("NAME", "SIZE"), ("sub/", "5 B"), ("first", "3 B")]);
    assert!(stdout.lines().any(|line| line == "TOTAL  8 B"), "{stdout}");

    #[cfg(unix)]
    {
        let stdout = scan_output(&direct_link);
        assert!(stdout.lines().any(|line| line == "TOTAL  13 B"), "{stdout}");
    }
}
