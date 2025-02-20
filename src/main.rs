#![warn(clippy::all, clippy::pedantic)]

use dset::{ process_safetensors_file, xio::walk_directory };
use std::env;
use std::path::Path;
use glob::glob;
use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the logger to output diagnostic information.
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename or directory>", args[0]);
        return Ok(());
    }
    let path = Path::new(&args[1]);

    if path.is_dir() {
        walk_directory(path, "safetensors", |file_path| {
            let path_buf = file_path.to_path_buf();
            async move { process_safetensors_file(&path_buf).await }
        }).await?;
    } else if let Some(path_str) = path.to_str() {
        if path_str.contains('*') {
            for entry in glob(path_str).context("Failed to read glob pattern")? {
                match entry {
                    Ok(path) => {
                        process_safetensors_file(&path).await?;
                    }
                    Err(e) => println!("Error processing entry: {e:?}"),
                }
            }
        } else {
            process_safetensors_file(path).await?;
        }
    } else {
        return Err(anyhow::anyhow!("Invalid path provided"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::{self, File};
    use tokio::io::AsyncWriteExt;
    use tokio::time::{sleep, Duration};

    async fn create_dummy_safetensors(path: &Path) -> anyhow::Result<()> {
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).await?;
        let mut file = File::create(path).await?;
        // Write a minimal valid safetensors header
        let header = r#"{"__metadata__":{"foo":"bar"}}"#;
        let header_len = header.len() as u64;
        let mut header_bytes = header_len.to_le_bytes().to_vec();
        header_bytes.extend(header.as_bytes());
        file.write_all(&header_bytes).await?;
        file.flush().await?;
        // Drop the file handle explicitly
        drop(file);
        // Small delay to ensure filesystem operations complete
        sleep(Duration::from_millis(50)).await;
        Ok(())
    }

    #[tokio::test]
    async fn test_process_single_file() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let file_path = temp_dir.path().join("test.safetensors");
        create_dummy_safetensors(&file_path).await?;
        
        let result = process_safetensors_file(&file_path).await;
        if result.is_err() {
            eprintln!("Error processing file: {:?}", result);
        }
        assert!(result.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_process_directory() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let file1 = temp_dir.path().join("test1.safetensors");
        let file2 = temp_dir.path().join("subdir").join("test2.safetensors");
        
        create_dummy_safetensors(&file1).await?;
        create_dummy_safetensors(&file2).await?;

        let result = walk_directory(temp_dir.path(), "safetensors", |file_path| {
            let path_buf = file_path.to_path_buf();
            async move { process_safetensors_file(&path_buf).await }
        }).await;
        
        assert!(result.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_path() -> anyhow::Result<()> {
        let invalid_path = Path::new("nonexistent.safetensors");
        let result = process_safetensors_file(invalid_path).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_glob_pattern() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let file1 = temp_dir.path().join("test1.safetensors");
        let file2 = temp_dir.path().join("test2.safetensors");
        
        create_dummy_safetensors(&file1).await?;
        create_dummy_safetensors(&file2).await?;

        let pattern = temp_dir.path().join("*.safetensors");
        let pattern_str = pattern.to_str().unwrap();

        for entry in glob(pattern_str)? {
            match entry {
                Ok(path) => {
                    let result = process_safetensors_file(&path).await;
                    assert!(result.is_ok());
                }
                Err(e) => panic!("Failed to process glob entry: {e:?}"),
            }
        }
        Ok(())
    }
}
