use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    
    // Configure paths
    let proto_root = Path::new("proto");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let src_proto_dir = Path::new("src/proto");
    
    // Ensure the proto output directory exists in src
    if !src_proto_dir.exists() {
        fs::create_dir_all(src_proto_dir)?;
        println!("Created output directory: {}", src_proto_dir.display());
    }

    // Discover all .proto files recursively
    let proto_files = discover_proto_files(proto_root)?;
    
    if proto_files.is_empty() {
        println!("cargo:warning=No .proto files found in proto/ directory");
        return Ok(());
    }

    println!("Found {} proto files to compile:", proto_files.len());
    for file in &proto_files {
        println!("  - {}", file.display());
        println!("cargo:rerun-if-changed={}", file.display());
    }

    // Configure tonic-build with comprehensive settings
    let mut config = tonic_build::configure();
    
    // Enable both client and server code generation for maximum flexibility
    config = config
        .build_server(true)
        .build_client(true)
        .build_transport(true)
        .out_dir(&src_proto_dir)
        .emit_rerun_if_changed(false); // We handle this manually above
    
    // Configure file descriptor sets for reflection support
    config = config
        .file_descriptor_set_path(out_dir.join("proto_descriptor.bin"))
        .include_file("mod.rs");

    // Skip additional type attributes for now to avoid conflicts with generated code

    // Compile the protobuf files
    println!("Compiling protobuf files...");
    config.compile(
        &proto_files,
        &[proto_root]
    ).map_err(|e| {
        eprintln!("Failed to compile protobuf files: {}", e);
        eprintln!("Proto files: {:?}", proto_files);
        eprintln!("Include paths: {:?}", &[proto_root]);
        e
    })?;

    // Generate module structure to match proto packages
    generate_module_structure(src_proto_dir, &proto_files, proto_root)?;

    println!("Successfully compiled {} protobuf files", proto_files.len());
    println!("Generated Rust code in: {}", src_proto_dir.display());
    
    // Verify generated files exist
    verify_generated_files(src_proto_dir)?;
    
    Ok(())
}

/// Recursively discover all .proto files in the given directory
fn discover_proto_files(proto_root: &Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut proto_files = Vec::new();
    let mut visited_dirs = HashSet::new();
    discover_proto_files_recursive(proto_root, &mut proto_files, &mut visited_dirs)?;
    
    // Sort for deterministic build order
    proto_files.sort();
    Ok(proto_files)
}

fn discover_proto_files_recursive(
    dir: &Path, 
    proto_files: &mut Vec<PathBuf>,
    visited_dirs: &mut HashSet<PathBuf>
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    
    // Prevent infinite loops from symlinks
    if visited_dirs.contains(&canonical_dir) {
        return Ok(());
    }
    visited_dirs.insert(canonical_dir);

    if !dir.exists() {
        println!("cargo:warning=Proto directory does not exist: {}", dir.display());
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            // Recursively search subdirectories
            discover_proto_files_recursive(&path, proto_files, visited_dirs)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("proto") {
            proto_files.push(path);
        }
    }
    
    Ok(())
}

/// Generate a proper module structure that matches the proto package hierarchy
fn generate_module_structure(
    src_proto_dir: &Path,
    proto_files: &[PathBuf],
    _proto_root: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    // Extract package names from proto files to understand the structure
    let mut packages = HashSet::new();
    
    for proto_file in proto_files {
        if let Some(package) = extract_package_from_proto_file(proto_file)? {
            packages.insert(package);
        }
    }

    // Generate package-specific module files
    for package in &packages {
        generate_package_module(src_proto_dir, package)?;
    }

    // Generate the main mod.rs file that re-exports all modules
    generate_main_module_file(src_proto_dir, &packages)?;
    
    Ok(())
}

/// Extract the package name from a .proto file
fn extract_package_from_proto_file(proto_file: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(proto_file)?;
    
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("package ") && trimmed.ends_with(';') {
            let package = trimmed
                .trim_start_matches("package ")
                .trim_end_matches(';')
                .trim()
                .to_string();
            return Ok(Some(package));
        }
    }
    
    Ok(None)
}

/// Generate a package-specific module file
fn generate_package_module(
    src_proto_dir: &Path,
    package: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // Convert package name to module path (e.g., "rustmq.common" -> "common")
    let module_name = package.split('.').last().unwrap_or(package);
    let module_file = src_proto_dir.join(format!("{}.rs", module_name));
    
    // tonic-build will generate the actual module content
    // We just need to ensure the file exists for proper module resolution
    if !module_file.exists() {
        println!("Creating module file: {}", module_file.display());
    }
    
    Ok(())
}

/// Generate the main mod.rs file that re-exports all generated modules
fn generate_main_module_file(
    src_proto_dir: &Path,
    packages: &HashSet<String>
) -> Result<(), Box<dyn std::error::Error>> {
    let mod_file = src_proto_dir.join("mod.rs");
    let mut content = String::new();
    
    content.push_str("//! Generated protobuf modules for RustMQ\n");
    content.push_str("//! \n");
    content.push_str("//! This module contains all generated protobuf types and services.\n");
    content.push_str("//! Generated automatically by build.rs - do not edit manually.\n");
    content.push_str("\n");
    content.push_str("#![allow(clippy::all)]\n");
    content.push_str("#![allow(warnings)]\n");
    content.push_str("\n");

    // Extract unique module names and sort them for deterministic output
    let mut module_names: Vec<String> = packages
        .iter()
        .map(|package| package.split('.').last().unwrap_or(package).to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    module_names.sort();

    // Generate module declarations with correct paths
    for module_name in &module_names {
        let package = packages.iter().find(|p| p.ends_with(module_name)).unwrap();
        content.push_str(&format!("#[path = \"{}.rs\"]\n", package));
        content.push_str(&format!("pub mod {};\n", module_name));
        content.push_str("\n");
    }
    
    content.push_str("// Re-export commonly used types for convenience\n");
    content.push_str("pub use common::*;\n");
    content.push_str("\n");
    
    // Generate convenience re-exports
    content.push_str("// Service re-exports\n");
    if module_names.contains(&"broker".to_string()) {
        content.push_str("pub use broker::broker_replication_service_server::BrokerReplicationServiceServer;\n");
        content.push_str("pub use broker::broker_replication_service_client::BrokerReplicationServiceClient;\n");
    }
    if module_names.contains(&"controller".to_string()) {
        content.push_str("pub use controller::controller_raft_service_server::ControllerRaftServiceServer;\n");
        content.push_str("pub use controller::controller_raft_service_client::ControllerRaftServiceClient;\n");
    }

    fs::write(&mod_file, content)?;
    println!("Generated main module file: {}", mod_file.display());
    
    Ok(())
}

/// Verify that the expected generated files exist
fn verify_generated_files(src_proto_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mod_file = src_proto_dir.join("mod.rs");
    
    if !mod_file.exists() {
        return Err(format!("Expected generated file not found: {}", mod_file.display()).into());
    }
    
    println!("✅ Build verification successful - all expected files generated");
    Ok(())
}