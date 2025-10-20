use esp_idf_sys::{esp_mesh_get_tsf_time, esp_random};
use serde::{Deserialize, Serialize};
use smart_leds::RGB8;

/// A list of unique instructions sorted by timestamp
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Instructions {
    pub instructions: Vec<Instruction>,
}

/// Instruction status indicating what action to take
pub enum InstructionStatus {
    /// If we should sleep for some amount of time before the next instruction
    Sleep(i64),
    /// If we should set the color immediately
    SetColor(RGB8),
}

/// A single instruction with a timestamp and color
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    pub timestamp: i64,
    pub color: RGB8,
}

impl Instructions {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    pub fn get_current_time(&self) -> i64 {
        unsafe { esp_mesh_get_tsf_time() }
    }

    /// Add an instruction to the list
    pub fn add_instruction(&mut self, instruction: Instruction) {
        self.instructions.push(instruction);
        self.instructions.sort_by_key(|i| i.timestamp);
    }

    /// Get the instructions between the current time and the current time + seconds
    pub fn get_next_seconds_instructions(&self, seconds: i64) -> Vec<Instruction> {
        let current_time = self.get_current_time();
        self.instructions
            .iter()
            .filter(|i| i.timestamp >= current_time && i.timestamp < current_time + seconds)
            .cloned()
            .collect()
    }

    /// Calculate how much of a buffer is left in the instructions list
    pub fn get_buffer_left(&self) -> i64 {
        let current_time = self.get_current_time();
        let next_instruction = self.instructions.first();
        if next_instruction.is_none() {
            return 0;
        }

        let next_instruction = next_instruction.unwrap();

        let buffer_left = next_instruction.timestamp - current_time;

        if buffer_left < 0 {
            return 0;
        }

        buffer_left
    }

    /// Get the next instruction, or find out if we should sleep for some amount
    /// of time. There are a few cases:
    ///
    /// - If we check too early (the instruction is more than 50ms in the
    ///   future), we should return an InstructionStatus::Sleep
    /// - If we check too late (the instruction is more than 50ms in the past),
    ///   drop that instruction, and log that we're late.
    /// - If we check at the right time (the instruction is within 50ms of the
    ///   current time), we should return an InstructionStatus::SetColor
    ///
    /// In any case where we set the colour, we should clean this instruction
    /// from the list.
    pub fn get_next_instruction(&mut self) -> InstructionStatus {
        // Get the next instruction
        let next_instruction = self.instructions.first();

        // If there's no next instruction, we should sleep for 100ms
        if next_instruction.is_none() {
            return InstructionStatus::Sleep(100);
        }

        // Get the next instruction
        let next_instruction = next_instruction.unwrap();

        // If the next instruction is in the past, we should drop it. We add a
        // 50ms buffer to account for the time it takes to process the
        // instruction, and this much drift shouldn't be noticeable.
        if next_instruction.timestamp < self.get_current_time() - 50 {
            self.instructions.remove(0);
            return InstructionStatus::Sleep(100);

            // TODO: Log that we're late
        }

        // If the next instruction is in the future, we should return an InstructionStatus::Sleep
        if next_instruction.timestamp > self.get_current_time() + 50 {
            return InstructionStatus::Sleep(
                next_instruction.timestamp - self.get_current_time() - 10,
            );
        }

        // If the next instruction is at the current time, we should return an InstructionStatus::SetColor
        let instruction = self.instructions.remove(0);
        return InstructionStatus::SetColor(instruction.color);
    }

    /// Combine a new list of instructions that will be buffered. A list will
    /// either come from the mesh root, or the mesh root might store the list it
    /// just generated. For this, there might be overlap with the current list,
    /// so we should only add timestamps that we don't have.
    pub fn combine_instructions(&mut self, instructions: Vec<Instruction>) {
        // Remove any instructions that are already in the list
        let new_instructions: Vec<Instruction> = instructions
            .iter()
            .filter(|i| !self.instructions.contains(i))
            .cloned()
            .collect();

        // Add the new instructions
        self.instructions.extend(new_instructions);

        // Sort the instructions by timestamp
        self.instructions.sort_by_key(|i| i.timestamp);
    }

    /// Generate a random list of new instructions for the next number of
    /// seconds passed in. This should only be called by the root node.
    pub fn generate_random_instructions(&mut self, seconds: i64) {
        let mut instructions = Vec::new();

        // Start with the current time
        let mut current_time = self.get_current_time();

        while current_time < self.get_current_time() + seconds {
            // Generate a random color
            let color = unsafe {
                RGB8::new(
                    (esp_random() % 256) as u8,
                    (esp_random() % 256) as u8,
                    (esp_random() % 256) as u8,
                )
            };

            // Add the instruction to the list
            instructions.push(Instruction {
                timestamp: current_time,
                color,
            });

            // Add a random delay between 100ms and 1000ms
            unsafe {
                current_time += (esp_random() % 1000 + 200) as i64;
            }
        }

        // Add the instructions to the list
        self.combine_instructions(instructions);
    }
}
