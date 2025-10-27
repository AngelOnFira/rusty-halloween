use defmt::{debug, warn};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use heapless::Vec;
use smart_leds::RGB8;

// Re-export Instruction from protocol
pub use crate::protocol::Instruction;

/// Maximum number of instructions to buffer
const MAX_INSTRUCTIONS: usize = 100;

/// Timing window for instruction execution (microseconds)
/// Instructions within ±50ms of current time are executed
const TIMING_WINDOW_US: i64 = 50_000;

/// Instruction status indicating what action to take
#[derive(Debug)]
pub enum InstructionStatus {
    /// Sleep for some duration in microseconds before checking again
    Sleep(u64),
    /// Set the LED color immediately
    SetColor(RGB8),
}

/// A list of instructions sorted by timestamp
#[derive(Debug, Clone)]
pub struct Instructions {
    /// Sorted list of instructions
    instructions: Vec<Instruction, MAX_INSTRUCTIONS>,
}

impl Instructions {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    /// Get current synchronized time in microseconds
    async fn get_current_time() -> u64 {
        crate::time_sync::now_us().await
    }

    /// Add an instruction to the list
    pub fn add_instruction(&mut self, instruction: Instruction) -> Result<(), ()> {
        // Try to push the instruction
        self.instructions.push(instruction).map_err(|_| ())?;

        // Sort by timestamp
        self.instructions.sort_unstable_by_key(|i| i.timestamp);

        Ok(())
    }

    /// Get instructions between current time and current time + duration (in seconds)
    pub async fn get_next_seconds_instructions(&self, seconds: u64) -> Vec<Instruction, MAX_INSTRUCTIONS> {
        let current_time = Self::get_current_time().await;
        let end_time = current_time + (seconds * 1_000_000);

        let mut result = Vec::new();

        for instruction in &self.instructions {
            if instruction.timestamp >= current_time && instruction.timestamp < end_time {
                let _ = result.push(instruction.clone());
            }
        }

        result
    }

    /// Calculate how much buffer is left in the instructions list (in microseconds)
    pub async fn get_buffer_left(&self) -> u64 {
        if self.instructions.is_empty() {
            return 0;
        }

        let current_time = Self::get_current_time().await;
        let next_instruction = &self.instructions[0];

        if next_instruction.timestamp <= current_time {
            return 0;
        }

        next_instruction.timestamp - current_time
    }

    /// Get the next instruction, or find out if we should sleep
    ///
    /// Timing windows:
    /// - If instruction is >50ms in the future: return Sleep
    /// - If instruction is >50ms in the past: drop it and log late execution
    /// - If instruction is within ±50ms: return SetColor
    pub async fn get_next_instruction(&mut self) -> InstructionStatus {
        // If there's no next instruction, sleep for 100ms
        if self.instructions.is_empty() {
            return InstructionStatus::Sleep(100_000);
        }

        let current_time = Self::get_current_time().await;
        let next_instruction = &self.instructions[0];

        // Calculate time difference (can be negative if in the past)
        let time_diff = next_instruction.timestamp as i64 - current_time as i64;

        // If instruction is too far in the past, drop it
        if time_diff < -TIMING_WINDOW_US {
            let _dropped = self.instructions.remove(0);
            warn!(
                "Dropped late instruction: {}us late",
                -time_diff
            );
            return InstructionStatus::Sleep(100_000);
        }

        // If instruction is too far in the future, sleep until closer
        if time_diff > TIMING_WINDOW_US {
            // Sleep until 10ms before the instruction time
            let sleep_time = (time_diff - 10_000).max(0) as u64;
            return InstructionStatus::Sleep(sleep_time);
        }

        // Instruction is within timing window, execute it
        let instruction = self.instructions.remove(0);
        debug!(
            "Executing instruction at {}us (diff={}us)",
            instruction.timestamp,
            time_diff
        );
        InstructionStatus::SetColor(instruction.color.0)
    }

    /// Combine a new list of instructions with existing ones
    /// Only adds instructions with new timestamps
    pub fn combine_instructions(&mut self, new_instructions: &[Instruction]) {
        for instruction in new_instructions {
            // Check if we already have this instruction
            let exists = self.instructions.iter().any(|i| {
                i.timestamp == instruction.timestamp && i.color == instruction.color
            });

            if !exists {
                if self.add_instruction(instruction.clone()).is_err() {
                    warn!("Instruction buffer full, dropping instruction");
                    break;
                }
            }
        }

        debug!("Combined instructions, total count: {}", self.instructions.len());
    }

    /// Clear all instructions
    pub fn clear(&mut self) {
        self.instructions.clear();
    }

    /// Get number of buffered instructions
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Check if instruction buffer is empty
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

/// Global instructions instance
pub static INSTRUCTIONS: Mutex<CriticalSectionRawMutex, Option<Instructions>> = Mutex::new(None);

/// Initialize instructions
pub async fn init_instructions() {
    let mut instructions = INSTRUCTIONS.lock().await;
    *instructions = Some(Instructions::new());
    defmt::info!("Instructions initialized");
}

/// Add instruction
pub async fn add_instruction(instruction: Instruction) -> Result<(), ()> {
    let mut instructions = INSTRUCTIONS.lock().await;
    if let Some(instr) = instructions.as_mut() {
        instr.add_instruction(instruction)
    } else {
        Err(())
    }
}

/// Get next instruction
pub async fn get_next_instruction() -> InstructionStatus {
    let mut instructions = INSTRUCTIONS.lock().await;
    if let Some(instr) = instructions.as_mut() {
        instr.get_next_instruction().await
    } else {
        InstructionStatus::Sleep(100_000)
    }
}

/// Combine instructions
pub async fn combine_instructions(new_instructions: &[Instruction]) {
    let mut instructions = INSTRUCTIONS.lock().await;
    if let Some(instr) = instructions.as_mut() {
        instr.combine_instructions(new_instructions);
    }
}

/// Get buffer remaining
pub async fn get_buffer_left() -> u64 {
    let instructions = INSTRUCTIONS.lock().await;
    if let Some(instr) = instructions.as_ref() {
        instr.get_buffer_left().await
    } else {
        0
    }
}

/// Clear all instructions
pub async fn clear_instructions() {
    let mut instructions = INSTRUCTIONS.lock().await;
    if let Some(instr) = instructions.as_mut() {
        instr.clear();
    }
}
