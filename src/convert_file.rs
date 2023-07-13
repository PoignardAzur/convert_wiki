use std::fs::File;
use std::io::{copy, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn convert_file(file_path: &Path, title: &str, content: &str) {
    let mut file = File::create(file_path).unwrap();
    write!(file, "# {}\n\n", title).unwrap();

    // run command, redirecting stdin and stdout to file_path
    let mut child_process = Command::new("pandoc")
        .arg("-f")
        .arg("mediawiki")
        .arg("-t")
        .arg("markdown")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child_process.stdin.as_mut().unwrap();
    stdin.write_all(content.as_bytes()).unwrap();
    std::mem::drop(child_process.stdin.take());

    copy(&mut child_process.stdout.unwrap(), &mut file);
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;
    use std::fs::read_to_string;

    #[test]
    fn test_convert_file() {
        let file_path = Path::new("test_convert_file.md");
        convert_file(file_path, "Article title", "The Text of the file");
        dbg!(std::fs::read_to_string(file_path).unwrap());
    }
}
