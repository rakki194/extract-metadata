# extract-metadata

A command-line utility for batch extracting metadata from safetensors files. This tool can process individual files or recursively scan directories to extract metadata from all safetensors files.

## Features

- Process single safetensors files or entire directories
- Support for glob patterns to match multiple files
- Recursive directory scanning
- Asynchronous processing for better performance
- Detailed metadata extraction from safetensors files

## Installation

Ensure you have Rust installed on your system, then you can install using cargo:

```bash
cargo install extract-metadata
```

Or build from source:

```bash
git clone https://github.com/rakki194/extract-metadata
cd extract-metadata
cargo build --release
```

## Usage

The tool supports several usage patterns:

1. Process a single file:

    ```bash
    extract-metadata path/to/model.safetensors
    ```

2. Process all safetensors files in a directory (recursive):

    ```bash
    extract-metadata path/to/directory
    ```

3. Use glob patterns to match specific files:

    ```bash
    extract-metadata "models/*.safetensors"
    ```

## Dependencies

- tokio - Async runtime
- anyhow - Error handling
- glob - File pattern matching
- env_logger - Logging functionality
- dset - Internal safetensors processing
- xio - File system operations

## Error Handling

The tool includes robust error handling and will:

- Skip invalid files while continuing to process others
- Provide clear error messages for invalid paths or patterns
- Handle IO errors gracefully

## License

MIT License

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
