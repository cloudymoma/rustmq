// Simple WASM module for testing ETL transformations
// Transforms input text to uppercase
// This is a no_std module for wasm32-unknown-unknown target

#![no_std]

use core::slice;

// Global allocator - simple bump allocator
static mut HEAP: [u8; 65536] = [0; 65536];
static mut HEAP_OFFSET: usize = 0;

/// Allocate memory in WASM
/// Returns pointer to allocated memory
#[no_mangle]
pub extern "C" fn allocate(size: i32) -> i32 {
    unsafe {
        let size = size as usize;
        let ptr = HEAP_OFFSET;

        // Align to 4 bytes
        let aligned_size = (size + 3) & !3;

        if HEAP_OFFSET + aligned_size > HEAP.len() {
            return -1; // Out of memory
        }

        HEAP_OFFSET += aligned_size;
        ptr as i32
    }
}

/// Deallocate memory (no-op for bump allocator)
#[no_mangle]
pub extern "C" fn deallocate(_ptr: i32) {
    // Bump allocator doesn't support deallocation
}

/// Transform function: converts input to uppercase
/// Input format: raw bytes
/// Output format: [length: 4 bytes][data: N bytes]
#[no_mangle]
pub extern "C" fn transform(input_ptr: i32, input_len: i32) -> i32 {
    unsafe {
        // Read input
        let input = slice::from_raw_parts(input_ptr as *const u8, input_len as usize);

        // Allocate memory for output (4 bytes length + data)
        let output_size = 4 + input.len();
        let output_ptr = allocate(output_size as i32);

        if output_ptr == -1 {
            return -1; // Out of memory
        }

        // Get output buffer
        let output_slice = slice::from_raw_parts_mut(output_ptr as *mut u8, output_size);

        // Write output length (first 4 bytes)
        let len_bytes = (input.len() as i32).to_le_bytes();
        output_slice[0..4].copy_from_slice(&len_bytes);

        // Transform: convert to uppercase and write directly to output
        for (i, &byte) in input.iter().enumerate() {
            if byte >= b'a' && byte <= b'z' {
                output_slice[4 + i] = byte - 32; // Convert to uppercase
            } else {
                output_slice[4 + i] = byte;
            }
        }

        output_ptr
    }
}

// Panic handler for no_std
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
