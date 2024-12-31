use std::fs;
use std::io;
use std::path::Path;
use test_helpers::{create_tmp_folder, file_contents_matches};
use std::fs::File;
use std::io::Write;

const EMPTY_FOLDER: &str = "NothingInHere";
const BACKUP_FOLDER_NAME: &str = "backups";
const THE_FILE: &str = "testfile.txt";
const THE_TEXT: &str = "backmeup susie";

#[test]
fn test_copies_file() -> io::Result<()> {
    let source = create_source()?;
    let _ = fs::remove_dir_all(&source);
    make_test_file(&source, THE_FILE, THE_TEXT)?;
    let dest = create_tmp_folder(BACKUP_FOLDER_NAME)?;

    copy_folder(&source, &dest)?;

    let test_file_path = Path::new(&dest).join(THE_FILE);
    assert!(test_file_path.exists(), "test file should be copied to backup folder");

    Ok(())
}

#[test]
fn test_copy_empty_folder() -> io::Result<()> {
    let source = create_source()?;
    let _ = fs::remove_dir_all(&source);

    let empty_folder_path = Path::new(&source).join(EMPTY_FOLDER);
    fs::create_dir_all(&empty_folder_path)?;

    let dest = create_tmp_folder(BACKUP_FOLDER_NAME)?;

    copy_folder(&source, &dest)?;

    check_empty_folder_copied(&dest)?;

    Ok(())
}

fn check_empty_folder_copied(dest: &str) -> io::Result<()> {
    let dir_path = Path::new(dest).join(EMPTY_FOLDER);
    let dir = fs::read_dir(&dir_path)?;
    assert_eq!(dir.count(), 0, "empty folder in source should be empty in backup");
    Ok(())
}

fn create_source() -> io::Result<String> {
    let source = create_tmp_folder("orig")?;
    Ok(source)
}

fn make_test_file(folder_path: &str, filename: &str, contents: &str) -> io::Result<()> {
    let deep_test_file_name = Path::new(folder_path).join(filename);
    let mut file = File::create(deep_test_file_name)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

pub fn copy_folder(source: &str, dest: &str) -> io::Result<()> {
    println!("backing up folder {} into {}", source, dest);
    let contents = fs::read_dir(source)?;

    for entry in contents {
        let entry = entry?;
        let path = entry.path();
        let dest_path = Path::new(dest).join(entry.file_name());

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            copy_folder(path.to_str().unwrap(), dest_path.to_str().unwrap())?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    Ok(())
}
