use std::fs::File;
use std::io::{copy, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use tracing::{info_span, trace};

pub fn convert_file(file_path: &Path, title: &str, content: &str) {
    let _span = info_span!("convert_file", title = title).entered();

    trace!("Creating file '{}'", file_path.to_string_lossy());
    let mut file = File::create(file_path).unwrap();

    trace!("Writing title to file");
    write!(file, "# {}\n\n", title).unwrap();

    // run command, redirecting stdin and stdout to file_path
    trace!("Running pandoc command");
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

    let stdin = child_process.stdin.as_mut().unwrap();
    stdin.write_all(content.as_bytes()).unwrap();
    std::mem::drop(child_process.stdin.take());

    trace!("Writing output to file");
    copy(&mut child_process.stdout.unwrap(), &mut file).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_file() {
        let file_path = Path::new("test_convert_file.md");
        convert_file(file_path, "Article title", "The Text of the file");
        dbg!(std::fs::read_to_string(file_path).unwrap());
    }
}
