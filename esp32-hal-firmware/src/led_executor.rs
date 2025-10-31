extern crate alloc;

use alloc::collections::BinaryHeap;
use alloc::vec::Vec;
use core::cmp::Ordering;

use embassy_time::{Duration, Timer};
use smart_leds::RGB8;
use crate::esp32_types::Esp32Instruction;

/// Maximum number of LEDs supported
pub const MAX_LEDS: usize = 35;

/// A timed instruction with ordering for priority queue
#[derive(Clone, Debug)]
struct QueuedInstruction {
    timestamp: u64,
    color: Option<RGB8>,  // None means this instruction doesn't change color
}

impl Eq for QueuedInstruction {}

impl PartialEq for QueuedInstruction {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }
}

impl Ord for QueuedInstruction {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (earliest timestamp first)
        other.timestamp.cmp(&self.timestamp)
    }
}

impl PartialOrd for QueuedInstruction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// LED command executor with timed instruction queue
pub struct LedExecutor {
    instruction_queue: BinaryHeap<QueuedInstruction>,
    current_color: RGB8,
    num_leds: usize,
}

impl LedExecutor {
    pub fn new(num_leds: usize) -> Self {
        Self {
            instruction_queue: BinaryHeap::new(),
            current_color: RGB8::default(),
            num_leds,
        }
    }

    /// Add a batch of instructions to the queue
    pub fn add_instructions(&mut self, instructions: Vec<Esp32Instruction>) {
        for instr in instructions {
            self.instruction_queue.push(QueuedInstruction {
                timestamp: instr.timestamp,
                color: instr.to_color(),
            });
        }
    }

    /// Add a single instruction to the queue
    pub fn add_instruction(&mut self, instruction: Esp32Instruction) {
        self.instruction_queue.push(QueuedInstruction {
            timestamp: instruction.timestamp,
            color: instruction.to_color(),
        });
    }

    /// Get the number of queued instructions
    pub fn queue_len(&self) -> usize {
        self.instruction_queue.len()
    }

    /// Clear all queued instructions
    pub fn clear_queue(&mut self) {
        self.instruction_queue.clear();
    }

    /// Execute instructions that should run at or before the current show time
    ///
    /// Returns a Vec of RGB8 values to write to the LED strip, or None if no changes
    pub fn execute_due_instructions(&mut self, current_show_time_ms: u64) -> Option<Vec<RGB8>> {
        let mut changed = false;

        // Process all instructions that are due
        while let Some(next) = self.instruction_queue.peek() {
            if next.timestamp <= current_show_time_ms {
                let instr = self.instruction_queue.pop().unwrap();
                if let Some(color) = instr.color {
                    self.current_color = color;
                    changed = true;
                }
            } else {
                break; // No more due instructions
            }
        }

        if changed {
            Some(self.generate_led_data())
        } else {
            None
        }
    }

    /// Generate LED data for the entire strip based on current state
    fn generate_led_data(&self) -> Vec<RGB8> {
        let mut data = Vec::with_capacity(self.num_leds);
        for _ in 0..self.num_leds {
            data.push(self.current_color);
        }
        data
    }

    /// Get the current color
    pub fn current_color(&self) -> RGB8 {
        self.current_color
    }

    /// Get the timestamp of the next instruction (if any)
    pub fn next_instruction_time(&self) -> Option<u64> {
        self.instruction_queue.peek().map(|i| i.timestamp)
    }

    /// Background task that continuously executes instructions
    ///
    /// This task will call the provided callback whenever LED data changes
    pub async fn execution_loop<F, Fut>(
        &mut self,
        get_time: F,
        mut on_update: impl FnMut(Vec<RGB8>) -> Fut,
    ) -> !
    where
        F: Fn() -> u64,
        Fut: core::future::Future<Output = ()>,
    {
        loop {
            let current_time = get_time();

            // Execute any due instructions
            if let Some(led_data) = self.execute_due_instructions(current_time) {
                on_update(led_data).await;
            }

            // Calculate sleep duration based on next instruction
            let sleep_duration = if let Some(next_time) = self.next_instruction_time() {
                let time_until = if next_time > current_time {
                    next_time - current_time
                } else {
                    1 // Already due, check again soon
                };

                // Cap sleep at 100ms to handle new instructions promptly
                Duration::from_millis(time_until.min(100))
            } else {
                // No instructions queued, check every 100ms
                Duration::from_millis(100)
            };

            Timer::after(sleep_duration).await;
        }
    }
}

impl Default for LedExecutor {
    fn default() -> Self {
        Self::new(MAX_LEDS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instruction_ordering() {
        let mut executor = LedExecutor::new(10);

        // Add instructions out of order
        executor.add_instruction(Esp32Instruction {
            timestamp: 1000,
            r: Some(255),
            g: Some(0),
            b: Some(0),
            off: None,
        });
        executor.add_instruction(Esp32Instruction {
            timestamp: 500,
            r: Some(0),
            g: Some(255),
            b: Some(0),
            off: None,
        });
        executor.add_instruction(Esp32Instruction {
            timestamp: 2000,
            r: None,
            g: None,
            b: None,
            off: Some(true),
        });

        // Should execute in timestamp order
        assert_eq!(executor.next_instruction_time(), Some(500));

        executor.execute_due_instructions(500);
        assert_eq!(executor.next_instruction_time(), Some(1000));

        executor.execute_due_instructions(1000);
        assert_eq!(executor.next_instruction_time(), Some(2000));

        executor.execute_due_instructions(2000);
        assert_eq!(executor.next_instruction_time(), None);
    }
}
