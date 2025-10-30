//! Memory diagnostics and profiling utilities for ESP32-S2
//!
//! This module provides functions to monitor heap usage, detect fragmentation,
//! and track memory consumption throughout the application lifecycle.

use esp_idf_sys::*;

/// Print a detailed memory statistics report
pub fn print_memory_stats(label: &str) {
    unsafe {
        let free = esp_get_free_heap_size();
        let min_free = heap_caps_get_minimum_free_size(MALLOC_CAP_DEFAULT);
        let largest = heap_caps_get_largest_free_block(MALLOC_CAP_DEFAULT);

        info!("=== MEMORY: {} ===", label);
        info!("  Free heap: {} bytes ({} KB)", free, free / 1024);
        info!("  Min free ever: {} bytes ({} KB)", min_free, min_free / 1024);
        info!("  Largest block: {} bytes ({} KB)", largest, largest / 1024);

        // Fragmentation check
        if free > 0 {
            let fragmentation = 100.0 - (largest as f32 / free as f32 * 100.0);
            info!("  Fragmentation: {:.1}%", fragmentation);

            if fragmentation > 25.0 {
                warn!("  ⚠ High fragmentation detected!");
            }
        }

        // Per-capability breakdown
        let dma_free = heap_caps_get_free_size(MALLOC_CAP_DMA);
        let internal_free = heap_caps_get_free_size(MALLOC_CAP_INTERNAL);
        let cap32_free = heap_caps_get_free_size(MALLOC_CAP_32BIT);
        let spiram_free = heap_caps_get_free_size(MALLOC_CAP_SPIRAM);

        info!("  Capability breakdown:");
        info!("    Internal RAM: {} KB", internal_free / 1024);
        info!("    PSRAM (SPIRAM): {} KB", spiram_free / 1024);
        info!("    DMA-capable: {} KB", dma_free / 1024);
        info!("    32-bit: {} KB", cap32_free / 1024);

        // Detailed heap structure
        let mut heap_info: multi_heap_info_t = std::mem::zeroed();
        heap_caps_get_info(&mut heap_info as *mut _, MALLOC_CAP_DEFAULT);

        info!("  Heap structure:");
        info!("    Allocated: {} bytes in {} blocks",
              heap_info.total_allocated_bytes, heap_info.allocated_blocks);
        info!("    Free: {} bytes in {} blocks",
              heap_info.total_free_bytes, heap_info.free_blocks);

        info!("=======================");
    }
}

/// Print a compact one-line memory summary
pub fn print_memory_summary(label: &str) {
    unsafe {
        let free = esp_get_free_heap_size();
        let min_free = heap_caps_get_minimum_free_size(MALLOC_CAP_DEFAULT);
        let largest = heap_caps_get_largest_free_block(MALLOC_CAP_DEFAULT);

        info!("[MEM] {}: Free={}KB, Min={}KB, Largest={}KB",
              label,
              free / 1024,
              min_free / 1024,
              largest / 1024);
    }
}

/// Print memory change since last measurement
pub fn print_memory_delta(label: &str, previous_free: u32) {
    unsafe {
        let current_free = esp_get_free_heap_size();
        let delta = current_free as i32 - previous_free as i32;

        if delta < 0 {
            info!("[MEM] {}: {}KB → {}KB ({} KB consumed)",
                  label,
                  previous_free / 1024,
                  current_free / 1024,
                  -delta / 1024);
        } else {
            info!("[MEM] {}: {}KB → {}KB (+{} KB freed)",
                  label,
                  previous_free / 1024,
                  current_free / 1024,
                  delta / 1024);
        }
    }
}

/// Get current free heap size (for manual tracking)
pub fn get_free_heap() -> u32 {
    unsafe { esp_get_free_heap_size() }
}

/// Check heap integrity and report any corruption
pub fn check_heap_integrity(label: &str) -> bool {
    unsafe {
        let result = heap_caps_check_integrity_all(true); // true = print errors
        if !result {
            error!("[MEM] {} - HEAP CORRUPTION DETECTED!", label);
        }
        result
    }
}

/// Print the heap low watermark (minimum free since boot)
pub fn print_heap_watermark() {
    unsafe {
        let min_free = heap_caps_get_minimum_free_size(MALLOC_CAP_DEFAULT);
        info!("[MEM] Heap low watermark: {} bytes ({} KB)", min_free, min_free / 1024);

        if min_free < 40 * 1024 {
            warn!("⚠ Low heap watermark! Minimum free was only {} KB", min_free / 1024);
        }
    }
}

/// Print all heap regions with their capabilities
pub fn print_heap_regions() {
    unsafe {
        info!("=== HEAP REGIONS ===");

        let caps = [
            (MALLOC_CAP_DEFAULT, "DEFAULT"),
            (MALLOC_CAP_INTERNAL, "INTERNAL"),
            (MALLOC_CAP_SPIRAM, "SPIRAM/PSRAM"),
            (MALLOC_CAP_DMA, "DMA"),
            (MALLOC_CAP_32BIT, "32BIT"),
            (MALLOC_CAP_8BIT, "8BIT"),
        ];

        for (cap, name) in &caps {
            let free = heap_caps_get_free_size(*cap);
            let min = heap_caps_get_minimum_free_size(*cap);
            let largest = heap_caps_get_largest_free_block(*cap);

            if free > 0 {
                info!("  {}: free={} KB, min={} KB, largest={} bytes",
                      name, free / 1024, min / 1024, largest);
            }
        }

        info!("===================");
    }
}

/// Print detailed heap info (verbose - use sparingly)
pub fn print_heap_details() {
    unsafe {
        info!("=== DETAILED HEAP INFO ===");

        let mut heap_info: multi_heap_info_t = std::mem::zeroed();
        heap_caps_get_info(&mut heap_info as *mut _, MALLOC_CAP_DEFAULT);

        info!("  Total allocated: {} bytes ({} KB)",
              heap_info.total_allocated_bytes,
              heap_info.total_allocated_bytes / 1024);
        info!("  Total free: {} bytes ({} KB)",
              heap_info.total_free_bytes,
              heap_info.total_free_bytes / 1024);
        info!("  Largest free block: {} bytes ({} KB)",
              heap_info.largest_free_block,
              heap_info.largest_free_block / 1024);
        info!("  Minimum free bytes: {} ({} KB)",
              heap_info.minimum_free_bytes,
              heap_info.minimum_free_bytes / 1024);
        info!("  Allocated blocks: {}", heap_info.allocated_blocks);
        info!("  Free blocks: {}", heap_info.free_blocks);
        info!("  Total blocks: {}", heap_info.total_blocks);

        let fragmentation = if heap_info.total_free_bytes > 0 {
            100.0 - (heap_info.largest_free_block as f32 / heap_info.total_free_bytes as f32 * 100.0)
        } else {
            0.0
        };
        info!("  Fragmentation: {:.2}%", fragmentation);

        info!("==========================");
    }
}

/// Helper macro to track memory at a specific point
#[macro_export]
macro_rules! track_memory {
    ($label:expr) => {
        $crate::diagnostics::print_memory_summary($label);
    };
}

/// Helper macro to track memory delta
#[macro_export]
macro_rules! track_memory_delta {
    ($label:expr, $prev:expr) => {
        $crate::diagnostics::print_memory_delta($label, $prev);
    };
}
