// WASM module with expensive computation for fuel exhaustion testing

#![no_std]

use core::slice;

static mut HEAP: [u8; 65536] = [0; 65536];
static mut HEAP_OFFSET: usize = 0;

#[no_mangle]
pub extern "C" fn allocate(size: i32) -> i32 {
    unsafe {
        let size = size as usize;
        let ptr = HEAP_OFFSET;
        let aligned_size = (size + 3) & !3;

        if HEAP_OFFSET + aligned_size > HEAP.len() {
            return -1;
        }

        HEAP_OFFSET += aligned_size;
        ptr as i32
    }
}

#[no_mangle]
pub extern "C" fn deallocate(_ptr: i32) {}

/// Expensive computation that consumes a lot of fuel
/// Calculates fibonacci numbers recursively (very inefficient)
fn fibonacci(n: u32) -> u32 {
    if n <= 1 {
        n
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

/// Transform function with expensive computation
#[no_mangle]
pub extern "C" fn transform(input_ptr: i32, input_len: i32) -> i32 {
    unsafe {
        // Read input
        let input = slice::from_raw_parts(input_ptr as *const u8, input_len as usize);

        // Expensive computation: calculate fibonacci for each byte
        let mut result: u32 = 0;
        for &byte in input {
            // Fibonacci of byte value (capped at 30 to avoid overflow)
            let fib_input = (byte % 31) as u32;
            result = result.wrapping_add(fibonacci(fib_input));
        }

        // Allocate output
        let output_size = 4 + 4; // length + result
        let output_ptr = allocate(output_size);

        if output_ptr == -1 {
            return -1;
        }

        // Write output length
        let len_bytes = 4i32.to_le_bytes();
        let output_slice = slice::from_raw_parts_mut(output_ptr as *mut u8, output_size as usize);
        output_slice[0..4].copy_from_slice(&len_bytes);

        // Write result
        let result_bytes = result.to_le_bytes();
        output_slice[4..8].copy_from_slice(&result_bytes);

        output_ptr
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
