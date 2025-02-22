#![warn(clippy::all, clippy::pedantic)]

use dset::{ process_safetensors_file, xio::walk_directory };
use std::env;
use std::path::{Path, PathBuf};
use glob::glob;
use anyhow::Context;

/// Normalize a path by converting it to absolute and cleaning up any . or .. components
fn normalize_path(path: &Path) -> anyhow::Result<PathBuf> {
    // First convert to absolute path if needed
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    
    // Try to canonicalize first (this handles symlinks too)
    match std::fs::canonicalize(&abs_path) {
        Ok(canonical) => Ok(canonical),
        Err(e) => {
            // If canonicalization fails, try to clean up the path manually
            let mut components = Vec::new();
            let mut had_error = false;
            
            for component in abs_path.components() {
                match component {
                    std::path::Component::Prefix(p) => components.push(std::path::Component::Prefix(p)),
                    std::path::Component::RootDir => components.push(std::path::Component::RootDir),
                    std::path::Component::Normal(x) => components.push(std::path::Component::Normal(x)),
                    std::path::Component::CurDir => (), // skip
                    std::path::Component::ParentDir => {
                        if components.len() <= 1 {
                            // Can't go up from root
                            had_error = true;
                            break;
                        }
                        // Only pop if we have something to pop
                        if !components.is_empty() {
                            components.pop();
                        }
                    }
                }
            }
            
            if had_error {
                // If we had an error in manual cleanup, return the original error
                Err(e.into())
            } else {
                Ok(components.iter().collect())
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the logger to output diagnostic information.
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename or directory>", args[0]);
        return Ok(());
    }
    
    let path = normalize_path(Path::new(&args[1]))?;

    if path.is_dir() {
        walk_directory(&path, "safetensors", |file_path| {
            let path_buf = match normalize_path(file_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Warning: Failed to normalize path {}: {}", file_path.display(), e);
                    file_path.to_path_buf()
                }
            };
            async move {
                match process_safetensors_file(&path_buf).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        eprintln!("Warning: Failed to process file {}: {}", path_buf.display(), e);
                        Ok(()) // Continue processing other files
                    }
                }
            }
        }).await?;
    } else if let Some(path_str) = path.to_str() {
        if path_str.contains('*') {
            for entry in glob(path_str).context("Failed to read glob pattern")? {
                match entry {
                    Ok(path) => {
                        let abs_path = normalize_path(&path).unwrap_or(path);
                        if let Err(e) = process_safetensors_file(&abs_path).await {
                            eprintln!("Warning: Failed to process file {}: {}", abs_path.display(), e);
                        }
                    }
                    Err(e) => println!("Error processing entry: {e:?}"),
                }
            }
        } else {
            if let Err(e) = process_safetensors_file(&path).await {
                eprintln!("Warning: Failed to process file {}: {}", path.display(), e);
            }
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

    async fn create_dummy_file(path: &Path, content: &str) -> anyhow::Result<()> {
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).await?;
        let mut file = File::create(path).await?;
        file.write_all(content.as_bytes()).await?;
        file.flush().await?;
        drop(file);
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

    #[tokio::test]
    async fn test_only_process_safetensors() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        
        // Create various file types
        let safetensors_file = temp_dir.path().join("model.safetensors");
        let toml_file = temp_dir.path().join("config.toml");
        let txt_file = temp_dir.path().join("readme.txt");
        let fake_safetensors = temp_dir.path().join("fake.safetensors.txt");

        create_dummy_safetensors(&safetensors_file).await?;
        create_dummy_file(&toml_file, "[config]\nkey = 'value'").await?;
        create_dummy_file(&txt_file, "This is a text file").await?;
        create_dummy_file(&fake_safetensors, "Not a real safetensors file").await?;

        // Test directory walking
        let processed_files = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let processed_files_clone = processed_files.clone();

        let result = walk_directory(temp_dir.path(), "safetensors", move |file_path| {
            let processed_files = processed_files_clone.clone();
            let path_buf = file_path.to_path_buf();
            async move {
                processed_files.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                process_safetensors_file(&path_buf).await
            }
        }).await;

        assert!(result.is_ok());
        // Only one file should have been processed (the real safetensors file)
        assert_eq!(processed_files.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Test glob pattern
        let pattern = temp_dir.path().join("*.safetensors");
        let pattern_str = pattern.to_str().unwrap();
        let mut glob_processed = 0;

        for entry in glob(pattern_str)? {
            match entry {
                Ok(path) => {
                    let result = process_safetensors_file(&path).await;
                    assert!(result.is_ok());
                    glob_processed += 1;
                }
                Err(e) => panic!("Failed to process glob entry: {e:?}"),
            }
        }

        // Only one file should match the glob pattern
        assert_eq!(glob_processed, 1);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_relative_paths() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let subdir = temp_dir.path().join("subdir");
        let file_path = subdir.join("test.safetensors");
        
        create_dummy_safetensors(&file_path).await?;
        
        // Change to the temp directory to test relative paths
        let original_dir = env::current_dir()?;
        env::set_current_dir(temp_dir.path())?;

        // Test with relative path
        let result = walk_directory(Path::new("."), "safetensors", move |file_path| {
            let path_buf = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
            async move { process_safetensors_file(&path_buf).await }
        }).await;
        assert!(result.is_ok());

        // Test with absolute path
        let result = walk_directory(temp_dir.path(), "safetensors", move |file_path| {
            let path_buf = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
            async move { process_safetensors_file(&path_buf).await }
        }).await;
        assert!(result.is_ok());

        // Test with mixed paths (some relative, some absolute)
        let mixed_dir = temp_dir.path().join("mixed");
        fs::create_dir_all(&mixed_dir).await?;
        let abs_file = mixed_dir.join("abs.safetensors");
        let rel_file = mixed_dir.join("rel.safetensors");
        
        create_dummy_safetensors(&abs_file).await?;
        create_dummy_safetensors(&rel_file).await?;

        let processed_files = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let processed_files_clone = processed_files.clone();

        let result = walk_directory(&mixed_dir, "safetensors", move |file_path| {
            let processed_files = processed_files_clone.clone();
            let path_buf = std::fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
            async move {
                processed_files.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                process_safetensors_file(&path_buf).await
            }
        }).await;

        assert!(result.is_ok());
        assert_eq!(processed_files.load(std::sync::atomic::Ordering::SeqCst), 2);

        // Restore original directory
        env::set_current_dir(original_dir)?;
        
        Ok(())
    }
}
